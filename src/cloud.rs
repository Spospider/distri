use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, Error, ErrorKind};

use std::collections::HashMap;
// use std::os::unix::net::SocketAddr;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use std::time::{Duration, Instant};
use crate::utils::{DEFAULT_TIMEOUT, END_OF_TRANSMISSION, server_encrypt_img, send_with_retry, recv_with_timeout, recv_reliable, send_reliable, extract_variable};
use rand::Rng; 
use serde_json::{value, Value};




#[derive(Clone)]
pub struct NodeInfo {
    pub load: u16,
    pub id: u16,
    pub addr: SocketAddr,
}


pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    chunk_size: Arc<usize>,
    elected: Arc<Mutex<bool>>,   // Elected state wrapped in Mutex for safe mutable access
    failed: Arc<Mutex<bool>>,
    load: Arc<Mutex<u16>>,
    id: Arc<Mutex<u16>>, // is the port of the addr, server ports have to be unique
    // internal_socket: Arc<UdpSocket>,
    
    // For stats
    accepted:Arc<Mutex<u32>>,
    completed:Arc<Mutex<u32>>,
    failed_number_of_times:Arc<Mutex<u32>>,
    failures:Arc<Mutex<u32>>,
    total_task_time:Arc<Mutex<Duration>>,

    // Distributed DB 
    tables: Arc<Mutex<HashMap<String, Vec<Value>>>>,

    
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        address: SocketAddr,
        nodes: Option<HashMap<String, SocketAddr>>,
        chunk_size: usize,
        elected: bool,
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
                    },
                )
            })
            .collect();
        // initialize initial_nodes as Arc<Mutex<HashMap<String, NodeInfo>>> from  nodes: Option<HashMap<String, SocketAddr>> 
        
        let socket = UdpSocket::bind(address).await?;
        let failed = false;
      
        // (address: SocketAddr)use address ip only as str here
        // let internal_socket: SocketAddr = format!("{}:{}", address.ip().to_string(), 4444).parse().unwrap();
        
        // initialize any table names that should exist
        let mut tables = HashMap::new();
        for table_name in table_names.unwrap_or_else(Vec::new) {
            tables.insert(table_name.to_string(), Vec::new());
        }

        Ok(Arc::new(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size:Arc::new(chunk_size),
            elected: Arc::new(Mutex::new(elected)),
            failed: Arc::new(Mutex::new(failed)),
            load: Arc::new(Mutex::new(0)),
            id: Arc::new(Mutex::new(address.port())),
          
            // Stats init
            accepted: Arc::new(Mutex::new(0)),
            completed: Arc::new(Mutex::new(0)),
            failed_number_of_times: Arc::new(Mutex::new(0)),
            failures: Arc::new(Mutex::new(0)),
            total_task_time: Arc::new(Mutex::new(Duration::default())),

            // Distributed DB
            tables: Arc::new(Mutex::new(tables)),
        }))
    }

    /// Starts the server to listen for incoming requests, elect a leader, and process data
    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        println!("Listening for incoming info requests on {:?}", self.public_socket.local_addr());

        // Initial leader election and info retrieval
        let _ = self.elect_leader();

        let mut buffer = vec![0u8; 65535]; // Buffer to hold incoming UDP packets
        loop {
            println!("looping0");
            // let (size, addr) = self.public_socket.recv_from(&mut buffer).await?;
            let (size, addr) = match recv_with_timeout(&self.public_socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                Ok((size, addr)) => (size, addr), // Successfully received data
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    continue;
                    // eprintln!("Receive operation timed out");
                    // continue; // Early exit or perform specific logic on timeout
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    continue; // Early exit or handle the error in some other way
                }
            };
            println!("looping1");
             // Clone buffer data to process it in a separate task
            let packet = buffer[..size].to_vec();
            

            let random_value = rand::thread_rng().gen_range(1..=10);
            println!("looping2");
            // If the random value is 0, do something
            if random_value == 0 && !*self.failed.lock().await  {  // start failure election
                println!("looping2.1");
                let mut failed = self.failed.lock().await;
                *failed = false; // Reset election state initially
                println!("looping2.11");
                let _ = self.get_info(); // Retrieve updated info from all nodes
                println!("looping2.12");
                *failed = self.election_alg().await; // Elect a node to fail
                println!("looping2.13");
                if *failed {
                    *self.failed_number_of_times.lock().await += 1;
                    println!("Node {} with is now failed.", self.public_socket.local_addr()?);
                }
                println!("looping2.2");
            }
            println!("looping3");
            
            // while failed do nothing at all
            if *self.failed.lock().await {
                println!("looping3.1");
                let random_value = rand::thread_rng().gen_range(0..=10);
                if random_value == 0 {
                    println!("looping3.2");
                    println!("Node {} is back up from failure.", self.public_socket.local_addr()?);
                    let mut failed = self.failed.lock().await;
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
            // perform election
            let _ = self.elect_leader();
            println!("looping5");
            //  to check different services
            let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&packet[..size]);
            println!("looping6");
            if received_msg == "Request: Stats"  || *self.elected.lock().await { // only if elected, or its a stats request
                if received_msg == "Request: Encrypt"  {
                        println!("looping7");
                        *self.accepted.lock().await += 1;

                        // Spawn a task to handle the connection and data processing
                        println!("looping8");
                        let node = self.clone();
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
                    println!("looping9");
                    // Spawn a task to handle the connection and data processing
                    let node = self.clone();
                    tokio::spawn(async move {
                        let start_time = Instant::now(); // Record start time
                        if let Err(e) = node.handle_stats(addr).await {
                            eprintln!("Error handling Stats: {:?}", e);
                            *node.failures.lock().await += 1; 
                        }
                        else{
                            let elapsed: Duration = start_time.elapsed();
                            *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
                            println!("Stats Done for {}", addr);
                        }

                    });
                    
                }
                else if received_msg == "Request: UpdateInfo" {
                    println!("received UpdateInfo");
                    if let Err(e) = self.handle_info_request(addr).await {
                        eprintln!("Error handling UpdateInfo: {:?}", e);
                    }
                }

                // Distributed DB stuff
                else if received_msg == "Request: AddTable" {
                    if let Err(e) = self.db_add_table(addr).await {
                        eprintln!("Error handling add table service: {:?}", e);
                    } else {
                        println!("AddTable Done for {}", addr);
                    }
                }
                else if received_msg.starts_with("Request: AddDocument") {  // only check the first part of "Request: AddDocument<tablename>"
                    if let Err(e) = self.db_add_entry(&received_msg.clone(), addr).await {
                        eprintln!("Error handling add doc service: {:?}", e);
                    } else {
                        println!("AddDocument Done for {}", addr);
                    }
                }
                else if received_msg.starts_with("Request: ReadTable") { // only check the first part of "Request: ReadTable<tablename>"
                    if let Err(e) = self.db_read_table(&received_msg.clone(), addr).await {
                        eprintln!("Error handling read table service: {:?}", e);
                    } else {
                        println!("ReadTable Done for {}", addr);
                    }
                }
            }
        }
        // Ok(())
    }
              

 // election stuff
    /// Retrieves updated information from all nodes using TCP messages
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
            println!("sending getinfo to {}", addr);
    
            // Send the request to the node
            // TODO implement timeout and retrying, should we create our own send_to ?
            send_with_retry(&socket, request_msg.as_bytes(), addr, 10).await.unwrap();
            // socket.send_to(request_msg.as_bytes(), addr).await.expect("Failed to send request");
            println!("sent UpdateInfo");
    
            // Buffer to hold the response
            let mut buffer = [0u8; 1024];
            
            // Receive the response from the node
            // match socket.recv_from(&mut buffer).await {
            match recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                Ok((size, _)) => {
                    let updated_mem:u16 = buffer[..size].get(0).copied().unwrap_or(0).into();
    
                    let mut nodes = self.nodes.lock().await;
                    if let Some(node_info) = nodes.get_mut(&node_id) {
                        node_info.load = updated_mem;
                        println!("Updated info for node {}: load = {}", node_info.id, node_info.load);
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    eprintln!("getinfo operation timed out");
                },
                Err(e) => {
                    eprintln!("Failed to receive response from {}: {:?}", addr, e);
                }
            }
        }
    }

    /// Handles incoming TCP requests for node information
    async fn handle_info_request(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        println!("Received info request from {}", addr);

        let load = self.completed.lock().await.clone() - self.accepted.lock().await.clone(); // order is reversedd as the min value is selected
        let response = format!("{} {}", load, self.id.lock().await); // self.load.lock().await
        let socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind UDP socket");

        // Send the response back to the requesting node
        // socket.send_to(response.as_bytes(), addr).await?;
        send_with_retry(&socket, response.as_bytes(), addr, 5).await?;
        println!("Sent info response to {}: {}", addr, response);

        Ok(())
    }

    /// Elects the leader node based on the lowest load value, breaking ties with the lowest id
     
    async fn elect_leader(self: &Arc<Self>) {
        // let clone = self.clone();
        
        // *elected = false; // Reset election state initially
        self.get_info().await; // Retrieve updated info from all nodes
        let mut elected = self.elected.lock().await;
        *elected = self.election_alg().await; // Elect a leader based on load and id values
        println!("elected value: {}", elected);
    }

    async fn election_alg(self: &Arc<Self>) -> bool {
        println!("election1");
        let nodes = self.nodes.lock().await;
        let mut lowest_mem = *self.load.lock().await;
        let mut elected_node = *self.id.lock().await;
        println!("election2");

        for (_, node_info) in nodes.iter() {
            if node_info.load < lowest_mem || (node_info.load == lowest_mem && node_info.id < elected_node) {
                lowest_mem = node_info.load;
                elected_node = node_info.id;
            }
        }
        println!("election3");

        // let mut elected = self.elected.lock().await;
        // *elected = self.id == elected_node;
        let elected = *self.id.lock().await == elected_node;
        println!("elected node: {}", elected_node);
        // println!("Node {} is elected as leader: {} with load={}", self.id, *elected, self.load);
        return elected;
        
    }
  
 // end election stuff


    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_encryption(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        // Convert incoming bytes to a string to parse JSON
        // let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&data[..size]);

        // if received_msg == "Request: Encrypt" {
        println!("Processing request from client: {}", addr);

        // Establish a connection to the client for sending responses
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        println!("Established connection with client on {}", socket.local_addr()?);

        // Send "OK" message to the client to indicate we are ready to receive data
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for Encrypt to {}", addr);

        let (aggregated_data, _, _) = recv_reliable(&socket).await?;

        // Process the aggregated data
        let processed_data = self.process_img(aggregated_data).await;

        send_reliable(&socket, &processed_data, addr).await?;

        socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
        println!("Task for client done: {}", addr);
        *self.completed.lock().await += 1;
        // }
        println!("done handling connection for: {}", addr);
        Ok(())
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_stats(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        // Convert incoming bytes to a string to parse JSON
        // let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&data[..size]);

        // if received_msg == "Request: Stats" {
        println!("Processing stats request from client: {}", addr);

        // Establish a connection to the client for sending responses
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
    
        // Lock and retrieve values from the shared stats variables
        let accepted = *self.accepted.lock().await;
        let completed = *self.completed.lock().await;
        let failed_times = *self.failed_number_of_times.lock().await;
        // let failures = *self.failures.lock().await;
        let total_time = *self.total_task_time.lock().await;
    
        // Calculate the average task completion time if there are any completed tasks
        let avg_completion_time = if completed > 0 {
            total_time / completed as u32
        } else {
            Duration::from_secs(0)
        };
    
        // Create a human-readable stats report
        let stats_report = format!(
            "Server Stats:\n\
            Accepted Requests: {}\n\
            Completed Tasks: {}\n\
            Failed Attempts: {}\n\
            Total Task Time: {:.2?}\n\
            Avg Completion Time: {:.2?}\n",
            accepted,
            completed,
            failed_times,
            // failures,
            total_time,
            avg_completion_time,
        );
    
        // Send the stats report back to the client
        // socket.send_to(stats_report.as_bytes(), &addr).await?;
        send_with_retry(&socket, stats_report.as_bytes(), addr, 5).await?;
        // }
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
        let mut file = File::create(img_path).await.unwrap();
        file.write_all(&data).await.unwrap();
    
        // Step 2: Call `server_encrypt_img` to perform encryption on the file
        server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;
    
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
        println!("Sent 'OK' message for AddTable to {}", addr);
        
        // receive data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (packet, _, _) = match recv_reliable(&socket).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    // Successfully received data from the correct client
                    let table_name: &str = &String::from_utf8_lossy(&packet);

                    let mut tables = self.tables.lock().await;
                    if tables.contains_key(table_name) {
                        // reply to sender
                        let response = format!("Table '{}' already exists.", table_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(Some(response));
                    } else {
                        tables.insert(table_name.to_string(), Vec::new());
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

        // Check if the table_name exists in self.table_names
        // let tables = self.tables.lock().await;
        // if !tables.contains_key(table_name) {
        //     // Table does not exist, send error to client
        //     send_with_retry(&socket, b"Error: Table Does not Exist", addr, 5).await?;
        //     return Err(Error::new(ErrorKind::Other, "Add entry: table does not exist"));
        // }


        // send ok
        send_with_retry(&socket, b"OK", addr, 5).await?;
        println!("Sent 'OK' message for AddEntry to {}", addr);


        // recieve data
        // let mut buffer = [0u8; 1024];
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (packet, _, _) = match recv_reliable(&socket).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    // let packet: [u8] = buffer[..size].to_vec();
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
                    
                    // Add the "provider" field with the client's address
                    if let Value::Object(ref mut obj) = entry {
                        obj.insert("provider".to_string(), Value::String(addr.to_string()));
                    }

                    let mut tables = self.tables.lock().await;
                    if let Some(table) = tables.get_mut(table_name) {
                        table.push(entry);
                        println!("Added Doc: {}", data);

                        // reply to sender
                        let response = "Entry added successfully.".to_string();
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

        // Access the tables and attempt to fetch the requested table
        let tables = self.tables.lock().await;
        if let Some(table) = tables.get(table_name) {
            // Convert the Vec<Value> to a JSON array and return it
            // reply to sender
            let response = Value::Array(table.clone()).to_string();
            send_with_retry(&socket, response.as_bytes(), addr, 5).await?;
            Ok(format!("Table '{}' read by {}", table_name, addr))
        } else {
            // Return an error if the table doesn't exist
            let response = format!("Table '{}' not found.", table_name);
            send_reliable(&socket, response.as_bytes(), addr).await?;
            Err(Error::new(ErrorKind::NotFound, response))
        }
    }

}
