use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, Error, ErrorKind};

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use std::time::{Duration, Instant};
use crate::utils::{DEFAULT_TIMEOUT, END_OF_TRANSMISSION, server_encrypt_img, send_with_retry, recv_with_timeout, recv_reliable, send_reliable, extract_variable};
use rand::Rng; 
use serde_json::{json, Value, to_string};
use colored::*; // Import the trait for coloring
use uuid::Uuid;




#[derive(Clone)]
pub struct NodeInfo {
    pub load: i32,
    pub id: u16,
    pub addr: SocketAddr,
    pub db_version: u32,
}


pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    // chunk_size: Arc<usize>,
    elected: Arc<Mutex<bool>>,   // Elected state wrapped in Mutex for safe mutable access
    failed: Arc<Mutex<bool>>,
    load: Arc<Mutex<i32>>,
    id: Arc<Mutex<u16>>, // is the port of the addr, server ports have to be unique
    num_workers: Arc<Mutex<u32>>,
    // internal_socket: Arc<UdpSocket>,
    
    // For stats
    requests:Arc<Mutex<u32>>,
    accepted:Arc<Mutex<u32>>,
    completed:Arc<Mutex<u32>>,
    failed_number_of_times:Arc<Mutex<u32>>,
    failures:Arc<Mutex<u32>>,
    total_task_time:Arc<Mutex<Duration>>,

    // Distributed DB 
    collections: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    db_data_version: Arc<Mutex<u32>>,
    
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        num_workers:u32,
        address: SocketAddr,
        nodes: Option<HashMap<String, SocketAddr>>,
        _chunk_size: usize,
        table_names: Option<Vec<&str>>,
    ) -> Result<Arc<Self>> {
        // let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let initial_nodes: HashMap<String, NodeInfo> = nodes
            .unwrap_or_else(HashMap::new)
            .into_iter()
            .map(|(name, addr)| {
                // Create NodeInfo for each node with the given addr, load, and id
                (
                    name,
                    NodeInfo {
                        load:0,
                        id:addr.port(),
                        addr,
                        db_version: 0,
                    }
                )
            })
            .collect();        
        let socket = UdpSocket::bind(address).await?;
        let failed = false;
              
        // initialize any table names that should exist
        let mut collections = HashMap::new();
        for table_name in table_names.unwrap_or_else(Vec::new) {
            collections.insert(table_name.to_string(), Vec::new());
        }

        Ok(Arc::new(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            // chunk_size:Arc::new(chunk_size),
            elected: Arc::new(Mutex::new(true)),
            failed: Arc::new(Mutex::new(failed)),
            load: Arc::new(Mutex::new(0)),
            id: Arc::new(Mutex::new(address.port())),
            num_workers: Arc::new(Mutex::new(num_workers)),
          
            // Stats init
            requests: Arc::new(Mutex::new(0)),
            accepted: Arc::new(Mutex::new(0)),
            completed: Arc::new(Mutex::new(0)),
            failed_number_of_times: Arc::new(Mutex::new(0)),
            failures: Arc::new(Mutex::new(0)),
            total_task_time: Arc::new(Mutex::new(Duration::default())),

            // Distributed DB
            collections: Arc::new(Mutex::new(collections)),
            db_data_version: Arc::new(Mutex::new(0)),
        }))
    }

    /// Starts the server to listen for incoming requests, elect a leader, and process data
    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        println!("Listening for incoming info requests on {:?}", self.public_socket.local_addr());

        // initialize request queue, multi-sender, multi receiver.
        // let (tx, mut rx) = mpsc::channel(1000);
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let num_workers = self.num_workers.lock().await.clone();

        // main receiving layer thread
        let recv_self = self.clone();
        let recv_queue = queue.clone();
        tokio::spawn(async move {
            loop {
                println!("looping1");
                let mut buffer: Vec<u8> = vec![0u8; 65535]; // Buffer to hold incoming UDP packets
                let (size, addr) = match recv_with_timeout(&recv_self.public_socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                    Ok((size, addr)) => (size, addr), // Successfully received data
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        continue; // Early exit or handle the error in some other way
                    },
                    Err(e) => {
                        eprintln!("Failed to receive data: {:?}", e);
                        continue; // Early exit or handle the error in some other way
                    }
                };

                // Failure mechanism
                let random_value = rand::thread_rng().gen_range(1..=10);
                println!("looping2");
                // If the random value is 0, do something
                if random_value == 0 && !*recv_self.failed.lock().await  {  // start failure election
                    println!("looping2.1");
                    let mut failed = recv_self.failed.lock().await;
                    *failed = false; // Reset election state initially
                    println!("looping2.11");
                    let _ = recv_self.get_info(); // Retrieve updated info from all nodes
                    println!("looping2.12");
                    *failed = recv_self.election_alg(None).await; // Elect a node to fail
                    println!("looping2.13");
                    if *failed {
                        *recv_self.failed_number_of_times.lock().await += 1;
                        println!("Node {} with is now failed.", recv_self.public_socket.local_addr().unwrap());
                    }
                    println!("looping2.2");
                }
                println!("looping3");
                
                // while failed do nothing at all
                if *recv_self.failed.lock().await {
                    println!("looping3.1");
                    let random_value = rand::thread_rng().gen_range(0..=10);
                    if random_value == 0 {
                        println!("looping3.2");
                        println!("Node {} is back up from failure.", recv_self.public_socket.local_addr().unwrap());
                        let mut failed = recv_self.failed.lock().await;
                        *failed = false;
                        println!("looping3.3");
                    }
                    else {
                        // stay failed
                        println!("Node is dead");
                        continue;
                    }
                }
                println!("looping4");
                // Clone buffer data to process it in a separate task
                let packet = buffer[..size].to_vec();
                let received_msg: String = String::from_utf8_lossy(&packet).into_owned();

                // Send to queue
                recv_queue.lock().await.push_back((received_msg, addr, Instant::now()));
                // if tx.send((received_msg, addr)).await.is_err() {
                //     eprintln!("Receiver task: failed to send to processing queue.");
                // }
                println!("looping5");
            }
        });

        // main receiving layer thread
        for _ in 0..num_workers {
            let proc_self = self.clone();
            let proc_queue = queue.clone();

            // Processing layer thread
            tokio::spawn(async move {
                loop {
                    // Pop from queue
                    let (received_msg, addr, recv_time) = match proc_queue.lock().await.pop_back() {
                        Some((received_msg, addr, recv_time)) => (received_msg, addr, recv_time),
                        None => {
                            continue;
                        },
                    };

                    // Drop old requests, focus on ones the clients are still waiting on
                    if recv_time.elapsed() > Duration::from_secs(DEFAULT_TIMEOUT) {
                        continue;
                    }

                    // Avoid infinite electing, only elect for public services
                    if received_msg != "Request: Stats"  && received_msg != "Request: UpdateInfo" {
                        // count new request for service
                        *proc_self.requests.lock().await += 1;
                    }

                    // If its a DB operation, sync data with other nodes.
                    if received_msg.starts_with("Request: CreateCollection") || received_msg.starts_with("Request: AddDocument") || received_msg.starts_with("Request: DeleteDocument") || received_msg.starts_with("Request: ReadCollection") {
                    // election slows things down a lot
                        let node = proc_self.clone();
                        tokio::spawn(async move {
                            node.elect_leader(Some(true)).await;
                        });
                    }

                    // Stats msgs and updateInfo msgs pass directly
                    if received_msg == "Request: Stats"  || received_msg == "Request: UpdateInfo" || *proc_self.elected.lock().await { // only if elected, or its a stats request
                        println!("Handling something");
                        let node = proc_self.clone();
                        if received_msg == "Request: Encrypt"  {
                                println!("looping6");

                                // Spawn a task to handle the connection and data processing
                                println!("looping7");
                                tokio::spawn(async move {
                                    let start_time = Instant::now(); // Record start time
                                    if let Err(e) = node.handle_encryption(addr).await {
                                        eprintln!("Error handling Encrypt: {:?}", e);
                                        *node.failures.lock().await += 1; 
                                    }
                                    else{
                                        let elapsed: Duration = start_time.elapsed();
                                        *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
                                        println!("Encrypt Done for {}", addr);
                                    }
                                });
                        }
                        else if received_msg == "Request: Stats"  {
                            println!("looping8");
                            // Spawn a task to handle the connection and data processing
                            tokio::spawn(async move {
                                // let start_time = Instant::now(); // Record start time
                                if let Err(e) = node.handle_stats(addr).await {
                                    eprintln!("Error handling Stats: {:?}", e);
                                }
                                else{
                                    println!("Stats Done for {}", addr);
                                }

                            });
                            
                        }
                        else if received_msg == "Request: UpdateInfo" {
                            println!("received UpdateInfo");
                            if let Err(e) = proc_self.handle_info_request(addr).await {
                                eprintln!("Error handling UpdateInfo: {:?}", e);
                            }
                        }


                        // Distributed DB stuff
                        else if received_msg == "Request: CreateCollection" {
                            if let Err(e) = proc_self.db_add_table(addr).await {
                                eprintln!("Error handling add table service: {:?}", e);
                            } else {
                                println!("AddCollection Done for {}", addr);
                            }
                        }
                        else if received_msg.starts_with("Request: AddDocument") {  // only check the first part of "Request: AddDocument<tablename>"
                            if let Err(e) = proc_self.db_add_entry(&received_msg.clone(), addr).await {
                                eprintln!("Error handling add doc service: {:?}", e);
                            } else {
                                println!("AddDocument Done for {}", addr);
                            }
                        }
                        else if received_msg.starts_with("Request: UpdateDocument") {  // only check the first part of "Request: AddDocument<tablename>"
                            if let Err(e) = proc_self.db_update_entry(&received_msg.clone(), addr).await {
                                eprintln!("Error handling update doc service: {:?}", e);
                            } else {
                                println!("UpdateDocument Done for {}", addr);
                            }
                        }
                        else if received_msg.starts_with("Request: DeleteDocument") {  // only check the first part of "Request: AddDocument<tablename>"
                            if let Err(e) = proc_self.db_delete_entry(&received_msg.clone(), addr).await {
                                eprintln!("Error handling delete doc service: {:?}", e);
                            } else {
                                println!("DeleteDocument Done for {}", addr);
                            }
                        }
                        else if received_msg.starts_with("Request: ReadCollection") { // only check the first part of "Request: ReadCollection<tablename>"
                            tokio::spawn(async move {
                                // let start_time = Instant::now(); // Record start time
                                if let Err(e) = node.db_read_table(&received_msg.clone(), addr).await {
                                    eprintln!("Error handling read table service: {:?}", e);
                                }
                                else{
                                    println!("ReadCollection Done for {}", addr);
                                }

                            });
                        }
                    }
                }
                
            });
        }
        tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C signal handler");
        Ok(())
    }
              

 // election stuff
    // Retrieves updated information from all nodes using TCP messages
    async fn get_info(self: &Arc<Self>) {
        let node_addresses: Vec<(String, SocketAddr)> = {
            let nodes = self.nodes.lock().await;
            nodes.iter().map(|(id, info)| (id.clone(), info.addr)).collect()
        };
    
        // Create a UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind UDP socket");
    
        for (node_id, addr) in node_addresses {
            if addr == self.public_socket.local_addr().unwrap() {
                continue;
            }
            let request_msg: &str = "Request: UpdateInfo";
            // println!("{} {}", "sending getinfo to".yellow(), addr);
    
            // Send the request to the node
            send_with_retry(&socket, request_msg.as_bytes(), addr, 10).await.unwrap();
            // println!("{}", "sent UpdateInfo".yellow());
    
            // Buffer to hold the response
            // let mut buffer = [0u8; 1024];
            
            // Receive the response from the node
             // REPLACE with recv_reliable
            // match recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
            let _ = match recv_reliable(&socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
                Ok((packet, size, _)) => {
                    let data = &packet[..size];
                    // Convert data to JSON
                    let json_obj: Value = match serde_json::from_slice(data) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("CRITICAL: Failed to parse JSON in getinfo: {:?}", e);
                            return; // Handle the error appropriately, e.g., skip processing this data
                        }
                    };

                    // let updated load = json_obj['load']
                    let updated_load:i32 = json_obj.get("load").unwrap().as_i64().unwrap() as i32;
                    // sync DB 
                    if json_obj.get("db_version").unwrap().as_u64().unwrap() > *self.db_data_version.lock().await as u64 {
                        // update   self.collections: Arc<Mutex<HashMap<String, Vec<Value>>>>, from json_obj.get("collections") which is created from serializing self.collections.lock().await.clone(),
                        if let Some(new_collections) = json_obj.get("collections").and_then(|v| v.as_object()) {
                            let mut collections_lock = self.collections.lock().await;
                    
                            // Clear current collections and populate them with the new data
                            collections_lock.clear();
                            for (key, value) in new_collections {
                                if let Some(array) = value.as_array() {
                                    collections_lock.insert(key.clone(), array.clone());
                                } else {
                                    eprintln!("Expected an array for collection '{}', but found {:?}", key, value);
                                    // Handle the error or skip this collection
                                }
                            }
                        } else {
                            eprintln!("Failed to update collections: 'collections' field is missing or not an object");
                            // Handle error appropriately (e.g., skip updating or log error)
                        }
                        *self.db_data_version.lock().await = json_obj.get("db_version").unwrap().as_u64().unwrap() as u32;
                    }
    
                    let mut nodes = self.nodes.lock().await;
                    if let Some(node_info) = nodes.get_mut(&node_id) {
                        node_info.load = updated_load;
                        let msg = format!("Updated info for node {}: load = {}", node_info.id, node_info.load);
                        println!("{}", msg.yellow());
                    }
                    // (packet, size, recv_addr)
                },
                Ok((_, _, recv_addr)) => {
                    eprintln!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    // (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("{}", "getinfo operation timed out".yellow());
                },
                Err(e) => {
                    eprintln!("Failed to receive response from {}: {:?}", addr, e);
                }
            };
        }
    }

    /// Handles incoming TCP requests for node information
    async fn handle_info_request(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        println!("Received info request from {}", addr);

        let load = *self.completed.lock().await as i32 - *self.requests.lock().await as i32; // order is reversedd as the min value is selected
        // update own load, in var and in node table
        *self.load.lock().await = load;
        let myid = self.id.lock().await.clone().to_string();
        let mut nodes = self.nodes.lock().await;
        if let Some(node_info) = nodes.get_mut(&myid) {
            node_info.load = load;
        }

        let response = to_string(&json!({
            "load": load,
            "id": myid,
            "collections": self.collections.lock().await.clone(),
            "db_version" : self.db_data_version.lock().await.clone(),
        })).unwrap();
        let socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind UDP socket");

        // Send the response back to the requesting node
        // send_with_retry(&socket, response.as_bytes(), addr, 5).await?;
        send_reliable(&socket, response.as_bytes(), addr).await?;
        println!("Sent info response to {}: {}", addr, response);

        Ok(())
    }

    /// Elects the leader node based on the lowest load value, breaking ties with the lowest id
     
    async fn elect_leader(self: &Arc<Self>, for_db:Option<bool>) {
        println!("{}", "elect_leader1".yellow());
        let mut elected = self.elected.lock().await;
        println!("elected locked");

        // elected = true if there are no known neighbors
        if self.nodes.lock().await.is_empty() {
            *elected = true;
            println!("{} {}","No neighbors, elected:".yellow(), elected);
        }
        
        // println!("{}", "elect_leader2".yellow());
        if *self.requests.lock().await > 1 {
            self.get_info().await; // Retrieve updated info from all nodes
        }
        // println!("{}", "elect_leader3".yellow());
        
        *elected = self.election_alg(for_db).await; // Elect a leader based on load and id values
        println!("{} {}","elected value:".yellow(), elected);
        // *self.electing.lock().await = false;
    }

    async fn election_alg(self: &Arc<Self>, for_db:Option<bool>) -> bool {
        // println!("{}", "election1".yellow());
        let nodes = self.nodes.lock().await;
        let mut lowest_load = self.load.lock().await.clone();
        let mut elected_node = self.id.lock().await.clone();
        let mut highest_db_version = self.db_data_version.lock().await.clone();
        // println!("{}", "election2".yellow());

        for (_, node_info) in nodes.iter() {
            if Some(true) == for_db {
                if node_info.db_version > highest_db_version {
                    highest_db_version = node_info.db_version;
                    elected_node = node_info.id;
                }
            }
            else {
                if node_info.load < lowest_load || (node_info.load == lowest_load && node_info.id < elected_node) {
                // if node_info.id < elected_node {
                    lowest_load = node_info.load;
                    elected_node = node_info.id;
                }
            }
        }
        // println!("{}", "election3".yellow());

        let elected = *self.id.lock().await == elected_node;
        // println!("{} {}","elected node:".yellow(), elected_node);
        if Some(true) == for_db {
            if *self.db_data_version.lock().await >= highest_db_version {
                return true;
            }
            else {
                return false;
            }
        }
        return elected;        
    }
 // end election stuff


    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_encryption(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        println!("Processing request from client: {}", addr);

        // Establish a connection to the client for sending responses
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        println!("Established connection with client on {}", socket.local_addr()?);

        // Send "OK" message to the client to indicate we are ready to receive data
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for Encrypt to {}", addr);

        let (aggregated_data, _, _) = recv_reliable(&socket, Some(Duration::from_secs(1))).await?;
        // Increment accepted
        *self.accepted.lock().await += 1;

        // Process the aggregated data
        let processed_data = self.process_img(aggregated_data).await;
        send_reliable(&socket, &processed_data, addr).await?;

        *self.completed.lock().await += 1;
        println!("Done handling Encrypt for: {}", addr);
        Ok(())
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_stats(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        println!("Processing stats request from client: {}", addr);

        // Establish a connection to the client for sending responses
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
    
        // Lock and retrieve values from the shared stats variables
        let requests = self.requests.lock().await.clone();
        let accepted = self.accepted.lock().await.clone();
        let completed = self.completed.lock().await.clone();
        let failed_times = self.failed_number_of_times.lock().await.clone();
        // let failures = *self.failures.lock().await;
        let total_time = self.total_task_time.lock().await.clone();
    
        // Calculate the average task completion time if there are any completed tasks
        let avg_completion_time = if completed > 0 {
            total_time / completed as u32
        } else {
            Duration::from_secs(0)
        };

        let table_stats: String = self.collections.lock().await.clone()
            .iter()
            .map(|(name, entries)| format!("{}: {} entries", name, entries.len()))
            .collect::<Vec<_>>()
            .join(", ");


        // Create a human-readable stats report
        let stats_report = format!(
            "Server Stats:\n\
            Requests Recieved: {}\n\
            Accepted Requests: {}\n\
            Completed Tasks: {}\n\
            Server Fails: {}\n\
            Total Task Time: {:.2?}\n\
            Avg Completion Time: {:.2?}\n
            DB Data: {}\n",
            requests,
            accepted,
            completed,
            failed_times,
            // failures,
            total_time,
            avg_completion_time,
            table_stats,
        );
    
        // Send the stats report back to the client
        send_with_retry(&socket, stats_report.as_bytes(), addr, 5).await?;
        println!("done handling connection for: {}", addr);
        Ok(())
    }
    

    /// Retrieves the registered server nodes
    pub fn get_nodes(&self) -> HashMap<String, NodeInfo> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }

    async fn process_img(&self, data: Vec<u8>) -> Vec<u8> {
        let img_path = "files/to_encrypt.jpg";
        let output_path = "files/encrypted_output.png";
    
        // Step 1: Write data bytes to a file (e.g., 'to_encrypt.png')
        // TODO only do this if file does not exist
        if !Path::new(img_path).exists() {
            let mut file = File::create(img_path).await.unwrap();
            file.write_all(&data).await.unwrap();
        
            // Step 2: Call `server_encrypt_img` to perform encryption on the file
            server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;
        }

        // Step 3: Read the encrypted output file as bytes
        let mut encrypted_file = File::open(output_path).await.unwrap();
        let mut encrypted_data = Vec::new();
        encrypted_file.read_to_end(&mut encrypted_data).await.unwrap();
    
        encrypted_data
    }

    // Add a table
    async fn db_add_table(&self, addr: SocketAddr) -> Result<Option<String>> { // change return type to option?

        // send ok
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for AddCollection to {}", addr);
        
        // receive data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    // Successfully received data from the correct client
                    let table_name: &str = &String::from_utf8_lossy(&packet);

                    let mut collections = self.collections.lock().await;
                    if collections.contains_key(table_name) {
                        // reply to sender
                        let response = format!("Table '{}' already exists.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(Some(response));
                    } else {
                        collections.insert(table_name.to_string(), Vec::new());
                        // update data version with any change in DB
                        *self.db_data_version.lock().await += 1;
                        // reply to sender
                        let response = format!("Table '{}' created.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(Some(response));
                    }
                },
                Ok((_, _, recv_addr)) => {
                    eprintln!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("Receive operation timed out");
                    return Ok(None);
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    return Ok(None);
                }
            };
        }
        Ok(None)

    }
    // Add an entry to a specific table
    async fn db_add_entry(&self, received_msg: &str,  addr: SocketAddr) -> Result<String> { // change to option so that ? delegates errors to above function
        // Process input var
        let table_name = &extract_variable(&received_msg)?;

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        // send ok
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("{:?} Sent 'OK' message for AddEntry to {}", socket.local_addr(), addr);

        // recieve data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    println!("done recieving");
                    let packet = packet[..size].to_vec();
                    let data: String = String::from_utf8_lossy(&packet).trim().to_string();
                    
                    let mut entry: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value, // Only proceed if it's an object
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {:?}", e);

                            // reply to sender
                            let response = format!("Error:Failed to parse JSON: {:?}", e);
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Err(Error::new(ErrorKind::Other, "Failed to parse JSON"));
                        }
                    };
                    
                    // Add random uuid
                    let mut uuid = Uuid::new_v4().to_string();
                    if let Value::Object(ref mut obj) = entry {
                        // if UUID does not exist in obj
                        if !obj.contains_key("UUID") {
                            obj.insert("UUID".to_string(), Value::String(uuid.clone()));
                        }
                        else {
                            uuid = obj["UUID"].as_str().unwrap().to_string();
                        }
                    }

                    let mut collections = self.collections.lock().await;
                    if let Some(table) = collections.get_mut(table_name) {
                        table.push(entry);
                        // update data version with any change in DB
                        *self.db_data_version.lock().await += 1;
                        println!("Added Doc: {}", data);

                        // reply to sender
                        let response = "Document added successfully".to_string();
                        println!("Sending Reply to {:?}", addr);
                        send_reliable(&socket, uuid.as_bytes(), addr).await?;
                        return Ok(response);
                    } else {
                        // reply to sender
                        let response = format!("Table '{}' does not exist.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(response);
                    }

                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    eprintln!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("Receive operation timed out");
                    return Err(Error::new(ErrorKind::Other, "Receive operation timed out"));
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    return Err(Error::new(ErrorKind::Other, "No variable supplied"));
                }
            };
        }
        return Err(Error::new(ErrorKind::Other, "Client did not communicate"));
    }

    // Update an entry in a specific table
    async fn db_update_entry(&self, received_msg: &str, addr: SocketAddr) -> Result<String> {
        // Process input variable
        let table_name = &extract_variable(&received_msg)?;

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        // Send OK response
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("{:?} Sent 'OK' message for UpdateEntry to {}", socket.local_addr(), addr);

        // Receive data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    println!("Done receiving");
                    let packet = packet[..size].to_vec();
                    let data: String = String::from_utf8_lossy(&packet).trim().to_string();
                    
                    let mut entry_to_update: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value,
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {:?}", e);

                            // Reply to sender
                            let response = format!("Error:Failed to parse JSON: {:?}", e);
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Err(Error::new(ErrorKind::Other, "Failed to parse JSON"));
                        }
                    };

                    // Extract ID for matching
                    let id: String = if let Value::Object(ref obj) = entry_to_update {
                        if let Some(Value::String(id)) = obj.get("UUID") {
                            id.clone()
                        } else {
                            // Reply to sender
                            let response = "Error: No valid 'UUID' field found in JSON".to_string();
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Err(Error::new(ErrorKind::Other, "No valid 'UUID' field found in JSON"));
                        }
                    } else {
                        // Reply to sender
                        let response = "Error: JSON must be an object".to_string();
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Err(Error::new(ErrorKind::Other, "JSON must be an object"));
                    };

                    let mut collections = self.collections.lock().await;
                    if let Some(table) = collections.get_mut(table_name) {
                        // Find the entry to update
                        if let Some(existing_entry) = table.iter_mut().find(|doc| {
                            if let Value::Object(ref obj) = doc {
                                obj.get("UUID") == Some(&Value::String(id.clone()))
                            } else {
                                false
                            }
                        }) {
                            *existing_entry = entry_to_update; // Replace the entry
                            // Update data version
                            *self.db_data_version.lock().await += 1;
                            println!("Updated Doc: {}", data);

                            // Reply to sender
                            let response = "Document updated successfully".to_string();
                            println!("Sending Reply to {:?}", addr);
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Ok(response);
                        } else { // match not found, add to collection anyway
                            // Add random uuid
                            let mut uuid = Uuid::new_v4().to_string();
                            if let Value::Object(ref mut obj) = entry_to_update {
                                // if UUID does not exist in obj
                                if !obj.contains_key("UUID") {
                                    obj.insert("UUID".to_string(), Value::String(uuid.clone()));
                                }
                                else {
                                    uuid = obj["UUID"].as_str().unwrap().to_string();
                                }
                            }
                            table.push(entry_to_update);
                            // update data version with any change in DB
                            *self.db_data_version.lock().await += 1;
                            println!("Added Doc: {}", data);

                            // reply to sender
                            let response = "Document added successfully".to_string();
                            println!("Sending Reply to {:?}", addr);
                            send_reliable(&socket, uuid.as_bytes(), addr).await?;
                            return Ok(response);

                        }
                    } else {
                        // Reply to sender
                        let response = format!("Table '{}' does not exist.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(response);
                    }

                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    eprintln!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("Receive operation timed out");
                    return Err(Error::new(ErrorKind::Other, "Receive operation timed out"));
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    return Err(Error::new(ErrorKind::Other, "No variable supplied"));
                }
            };
        }
        return Err(Error::new(ErrorKind::Other, "Client did not communicate"));
    }
    
    // Add an entry to a specific table
    async fn db_delete_entry(&self, received_msg: &str,  addr: SocketAddr) -> Result<String> { // change to option so that ? delegates errors to above function
        // Process input var
        let table_name = &extract_variable(&received_msg)?;

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port

        // send ok
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for AddEntry to {}", addr);

        // recieve data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    let packet = packet[..size].to_vec();
                    
                    let entry: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value, // Only proceed if it's an object
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {:?}", e);

                            // reply to sender
                            let response = format!("Error:Failed to parse JSON: {:?}", e);
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Err(Error::new(ErrorKind::Other, "Failed to parse JSON"));
                        }
                    };

                    // Ensure `entry` is a JSON object for matching
                    let entry_object = match entry.as_object() {
                        Some(obj) => obj,
                        None => {
                            let response = format!("Error:Entry is not a JSON object {:?}", entry);
                            send_reliable(&socket, response.as_bytes(), addr).await?;
                            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Entry must be a JSON object"));
                        }
                    };
                    
                    let mut collections = self.collections.lock().await;
                    if let Some(table) = collections.get_mut(table_name) {
                        // entry represents dict on fields to match on 
                        // example: { "provider" : "abc" }

                        // Retain only entries that do NOT match all the fields in `entry_object`
                        table.retain(|existing_entry| {
                            !existing_entry.as_object().map_or(false, |existing_fields| {
                                entry_object.iter().all(|(key, value)| {
                                    existing_fields.get(key) == Some(value)
                                })
                            })
                        });

                        // update data version with any change in DB
                        *self.db_data_version.lock().await += 1;
                        println!("Deleted Docs matching: {}", entry);

                        // reply to sender
                        let response = "Docs deleted successfully.".to_string();
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(response);
                    } else {
                        // reply to sender
                        let response = format!("Table '{}' does not exist.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(response);
                    }

                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    eprintln!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("Receive operation timed out");
                    return Err(Error::new(ErrorKind::Other, "Receive operation timed out"));
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    return Err(Error::new(ErrorKind::Other, "No variable supplied"));
                }
            };
        }
        return Err(Error::new(ErrorKind::Other, "Client did not communicate"));
    }
    

    // Read a table and return it as a JSON array
    async fn db_read_table(&self, received_msg: &str, addr: SocketAddr) -> Result<String> {
        // Extract table name from the received packet as a string
        let table_name = &extract_variable(&received_msg)?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        // send ok
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for AddEntry to {}", addr);

        // if there is filtering info
        match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
            Ok((packet, size, recv_addr)) if recv_addr == addr => {
                let packet = packet[..size].to_vec();
                
                let entry: Value = match serde_json::from_slice(&packet) {
                    Ok(value) => value, // Only proceed if it's an object
                    Err(e) => {
                        eprintln!("Failed to parse JSON: {:?}", e);

                        // reply to sender
                        let response = format!("Error:Failed to parse JSON: {:?}", e);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Err(Error::new(ErrorKind::Other, "Failed to parse JSON"));
                    }
                };

                // Ensure `entry` is a JSON object for matching
                let entry_object = match entry.as_object() {
                    Some(obj) => obj,
                    None => {
                        let response = format!("Error:Entry is not a JSON object {:?}", entry);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Entry must be a JSON object"));
                    }
                };

                let mut collections = self.collections.lock().await;
                if let Some(table) = collections.get_mut(table_name) {
                    // entry represents dict on fields to match on 
                    // example: { "provider" : "abc" }

                    // get only entries that do NOT match all the fields in `entry_object`
                    let matched_entries: Vec<Value> = table.iter().filter(|existing_entry| {
                        // Check if the existing entry is a JSON object
                        existing_entry.as_object().map_or(false, |existing_fields| {
                            // Ensure all fields in `entry_object` match the corresponding fields in `existing_fields`
                            entry_object.iter().all(|(key, value)| {
                                existing_fields.get(key) == Some(value)
                            })
                        })
                    }).cloned().collect();

                    // reply to sender
                    let response = Value::Array(matched_entries).to_string();
                    send_reliable(&socket, response.as_bytes(), addr).await?;
                    return Ok(response);
                }
                else {
                    // reply to sender
                    let response = format!("Table '{}' does not exist.", table_name);
                    send_reliable(&socket, response.as_bytes(), addr).await?;
                    return Ok(response);
                }
            },
            Ok((_, _, recv_addr)) => {
                // eprintln!("Received data from unexpected address: {:?}", recv_addr);
                // // Ignore and continue to wait for correct address
                (0, 0, recv_addr)
            },
            Err(_) => {
                (0, 0, "0.0.0.0:0".parse().unwrap())
            }
        };
        
        // fetch all data
        // Access the collections and attempt to fetch the requested table
        let collections = self.collections.lock().await.clone();
        if let Some(table) = collections.get(table_name) {
            // Convert the Vec<Value> to a JSON array and return it
            // reply to sender
            let response = Value::Array(table.clone()).to_string();
            send_reliable(&socket, response.as_bytes(), addr).await?;
            Ok(format!("Table '{}' read by {}", table_name, addr))
        } else {
            // Return an error if the table doesn't exist
            let response = format!("Table '{}' not found.", table_name);
            send_reliable(&socket, response.as_bytes(), addr).await?;
            Err(Error::new(ErrorKind::NotFound, response))
        }
    }

}
