use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::sleep;
use std::collections::{self, HashMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use std::net::SocketAddr;
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use rand::Rng; 
use serde_json::{json, to_string, Value};
use colored::*; // Import the trait for coloring
use uuid::Uuid;
use tracing_subscriber::filter::LevelFilter;
use tracing::{ instrument, info, warn, error, debug};
use tracing_error::{InstrumentResult, TracedError};



use crate::utils::{
    extract_args, 
    generate_response,
    DBOp, 
    DistriError,
    NodeInfo
};
use crate::networking::{
    DEFAULT_TIMEOUT,
    MAX_RETRIES,
    recv_reliable, 
    recv_with_timeout, 
    send_reliable, 
    send_with_retry 
};
use crate::db::{
    DB,
    Metadata
};
use crate::service::Service;




pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    // chunk_size: Arc<usize>,
    elected: Arc<Mutex<bool>>,   // Elected state wrapped in Mutex for safe mutable access
    failed: Arc<Mutex<bool>>,
    load: Arc<Mutex<i32>>,
    id: Arc<Mutex<u16>>, // is the port of the addr, server ports have to be unique
    num_workers: Arc<Mutex<u32>>,
    
    // For stats
    requests:Arc<Mutex<u32>>,
    accepted:Arc<Mutex<u32>>,
    completed:Arc<Mutex<u32>>,
    failed_number_of_times:Arc<Mutex<u32>>,
    failures:Arc<Mutex<u32>>,
    total_task_time:Arc<Mutex<Duration>>,

    // Distributed DB 
    db: DB,
    
    // Services
    services: Vec<Arc<dyn Service + 'static>>,
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        services: Vec<Box<dyn Service + 'static>>,
        num_workers:u32,
        address: SocketAddr,
        nodes: Option<HashMap<String, SocketAddr>>,
        _chunk_size: usize,
        collection_names: Option<Vec<&str>>,
    ) -> Result<Arc<Self>, DistriError> {
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
        let mut collections_metadata = HashMap::new();
        for collection_name in collection_names.unwrap_or_else(Vec::new) {
            collections.insert(collection_name.to_string(), HashMap::new());
            collections_metadata.insert(collection_name.to_string(), HashMap::new());
        }

        let services: Vec<Arc<dyn Service + 'static>> = services.into_iter().map(Arc::from).collect();

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
            db: DB::new(collections, collections_metadata),

            // collections_metadata: Arc::new(Mutex::new(collections.clone())),
            // collections: Arc::new(Mutex::new(collections)),
            // db.data_version: Arc::new(Mutex::new(0)),
            // db.time_to_update: Arc::new(Mutex::new(true)),

            services,
        }))
    }

    /// Starts the server to listen for incoming requests, elect a leader, and process data
    pub async fn serve(self: &Arc<Self>) -> Result<(), DistriError> {
        info!("Listening for incoming info requests on {:?}", self.public_socket.local_addr());

        // initialize request queue, multi-sender, multi receiver.
        // let (tx, mut rx) = mpsc::channel(1000);
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let internal_queue = Arc::new(Mutex::new(VecDeque::new()));
        let num_workers = self.num_workers.lock().await.clone();

        // main receiving layer thread
        let recv_self = self.clone();
        let recv_queue = queue.clone();
        let internal_queue1 = internal_queue.clone();
        tokio::spawn(async move {
            loop {
                debug!("looping1");
                let mut buffer: Vec<u8> = vec![0u8; 65535]; // Buffer to hold incoming UDP packets
                let (size, addr) = match recv_with_timeout(&recv_self.public_socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                    Ok((size, addr)) => (size, addr), // Successfully received data
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        continue; // Early exit or handle the error in some other way
                    },
                    Err(e) => {
                        error!("Failed to receive data: {:?}", e);
                        continue; // Early exit or handle the error in some other way
                    }
                };

                // Failure mechanism
                let random_value = rand::thread_rng().gen_range(1..=10);
                debug!("looping2");
                // If the random value is 0, do something
                if random_value == 0 && !*recv_self.failed.lock().await  {  // start failure election
                    debug!("looping2.1");
                    let mut failed = recv_self.failed.lock().await;
                    *failed = false; // Reset election state initially
                    debug!("looping2.11");
                    let _ = recv_self.get_info(None); // Retrieve updated info from all nodes
                    debug!("looping2.12");
                    *failed = recv_self.election_alg(None).await; // Elect a node to fail
                    debug!("looping2.13");
                    if *failed {
                        *recv_self.failed_number_of_times.lock().await += 1;
                        debug!("Node {} with is now failed.", recv_self.public_socket.local_addr().unwrap());
                    }
                    debug!("looping2.2");
                }
                debug!("looping3");
                
                // while failed do nothing at all
                if *recv_self.failed.lock().await {
                    debug!("looping3.1");
                    let random_value = rand::thread_rng().gen_range(0..=10);
                    if random_value == 0 {
                        debug!("looping3.2");
                        debug!("Node {} is back up from failure.", recv_self.public_socket.local_addr().unwrap());
                        let mut failed = recv_self.failed.lock().await;
                        *failed = false;
                        debug!("looping3.3");
                    }
                    else {
                        // stay failed
                        debug!("Node is dead");
                        continue;
                    }
                }
                debug!("looping4");
                // Clone buffer data to process it in a separate task
                let packet = buffer[..size].to_vec();
                let received_msg: String = String::from_utf8_lossy(&packet).into_owned();
                debug!("received: {}", received_msg);

                // Send to queue
                if received_msg == "ReqInternal: UpdateInfo" || received_msg == "ReqInternal: Stats" {
                    debug!("pushed into internal");
                    internal_queue1.lock().await.push_back((received_msg, addr, Instant::now()));
                }
                else {
                    recv_queue.lock().await.push_back((received_msg, addr, Instant::now()));
                }
                
                // if tx.send((received_msg, addr)).await.is_err() {
                //     eprintln!("Receiver task: failed to send to processing queue.");
                // }
                debug!("looping5");
            }
        });

        // thread for cloud internal communication
        let proc_self = self.clone();
        let int_queue: Arc<Mutex<VecDeque<(String, SocketAddr, Instant)>>> = internal_queue.clone();

        tokio::spawn(async move {
            loop {
                // Pop from queue
                let (received_msg, addr, recv_time) = match int_queue.lock().await.pop_back() {
                    Some((received_msg, addr, recv_time)) => (received_msg, addr, recv_time),
                    None => {
                        continue;
                    },
                };

                // Drop old requests, focus on ones the clients are still waiting on
                if recv_time.elapsed() > Duration::from_secs(DEFAULT_TIMEOUT) {
                    continue;
                }                

                // Stats msgs and updateInfo msgs pass directly
                let node = proc_self.clone();
                if received_msg == "ReqInternal: Stats"  {
                    // Spawn a task to handle the connection and data processing
                    debug!("doing stats");
                    tokio::spawn(async move {
                        // let start_time = Instant::now(); // Record start time
                        if let Err(e) = node.handle_stats(addr).await {
                            error!("Error handling Stats: {:?}", e);
                        }
                        else{
                            info!("Stats Done for {}", addr);
                        }

                    });
                    
                }
                else if received_msg == "ReqInternal: UpdateInfo" {
                    info!("received UpdateInfo");
                    if let Err(e) = proc_self.handle_info_request(addr).await {
                        // error!("Error handling UpdateInfo: {:?}", e);   
                    }
                }
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

                    let service_name = received_msg.split_whitespace().nth(1).unwrap_or("");

                    // count new request for service
                    *proc_self.requests.lock().await += 1;
                    
                    // If its a DB operation, sync data with other nodes.
                    if received_msg.starts_with("ReqMem: CreateCollection") || received_msg.starts_with("ReqMem: AddDocument") || received_msg.starts_with("ReqMem: DeleteDocument") || received_msg.starts_with("ReqMem: ReadCollection") {
                    // election slows things down a lot
                        let node = proc_self.clone();
                        // save prev state, and set false immediately to avoid race conditions on the elected var.
                        let prev_elected = proc_self.elected.lock().await.clone();
                        *proc_self.elected.lock().await = false;
                        tokio::spawn(async move {
                            node.elect_leader(Some(true), Some(prev_elected)).await;
                        });
                    }

                    // Only if elected
                    if proc_self.elected.lock().await.clone() {
                        println!("Handling {}", received_msg);
                        let node = proc_self.clone();
                        let service_option = proc_self.services.iter().find(|s| s.name() == service_name);
                        let _args = extract_args(received_msg.as_str());
                        if received_msg.split_whitespace().nth(0).unwrap_or("") == "Request:" && service_option.is_some() && _args.is_ok() {
                            let service = service_option.unwrap().clone();
                            let args = _args.unwrap();
                            // Spawn a task to handle the connection and data processing
                            println!("looping7");
                            tokio::spawn(async move {
                                let start_time = Instant::now(); // Record start time                    
                                if let Err(e) = node.handle_service(&service, args, addr).await {
                                    *node.failures.lock().await += 1; 
                                }
                                else{
                                    let elapsed: Duration = start_time.elapsed();
                                    *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
                                    info!("Service Done for {}", addr);
                                }
                            });
                        }
                        
                        
                        // Distributed DB stuff
                        else if received_msg == "ReqMem: CreateCollection" {
                            if let Err(e) = proc_self.db_add_table(addr).await {
                            } else {
                                *proc_self.completed.lock().await += 1;
                                info!("AddCollection Done for {}", addr);
                            }
                        }
                        else if received_msg.starts_with("ReqMem: AddDocument") {  // only check the first part of "ReqMem: AddDocument<tablename>"
                            let start_time = Instant::now();
                            if let Err(e) = proc_self.db_add_entry(_args.unwrap(), addr).await {
                                
                            } else {
                                info!("AddDocument Done for {}", addr);
                                let elapsed: Duration = start_time.elapsed();
                                *proc_self.completed.lock().await += 1;
                                *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
                            }
                        }
                        else if received_msg.starts_with("ReqMem: UpdateDocument") {  // only check the first part of "ReqMem: AddDocument<tablename>"
                            let start_time = Instant::now();
                            if let Err(e) = proc_self.db_update_entry(_args.unwrap(), addr).await {
                                
                            } else {
                                info!("UpdateDocument Done for {}", addr);
                                let elapsed: Duration = start_time.elapsed();
                                *proc_self.completed.lock().await += 1;
                                *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time

                            }
                        }
                        else if received_msg.starts_with("ReqMem: DeleteDocument") {  // only check the first part of "ReqMem: AddDocument<tablename>"
                            let start_time = Instant::now();
                            if let Err(e) = proc_self.db_delete_entry(_args.unwrap(), addr).await {
                                
                            } else {
                                info!("DeleteDocument Done for {}", addr);
                                let elapsed: Duration = start_time.elapsed();
                                *proc_self.completed.lock().await += 1;
                                *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
                            }
                        }
                        else if received_msg.starts_with("ReqMem: ReadCollection") { // only check the first part of "ReqMem: ReadCollection<tablename>"
                            tokio::spawn(async move {
                                let start_time = Instant::now(); // Record start time
                                if let Err(e) = node.db_read_table(_args.unwrap(), addr).await {
                                    
                                }
                                else{
                                    info!("ReadCollection Done for {}", addr);
                                    let elapsed: Duration = start_time.elapsed();
                                    *node.total_task_time.lock().await += elapsed; // Accumulate the elapsed time into total_task_time
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
              
    /// DB Sync mechanism
    /// base concept: send msg for Info Exchange including hash, and version number
                    /// on the other end, version num = max(incoming_version_num, self.version_num)
                    /// new data = current data union incoming data, growing set.
            
            /// Flow: Assuming fully connected nodes.
            /// - db_announce: (TODO) announce new changes to all other nodes, publish, subscribe without responding. 
            /// - handle_db_announce:
            ///     - DB merges new info (meta + data)
            /// - db_exchange: Periocially or with every db operation, exchange version num and hash with every other node, 
            /// - handle_db_exchange: a node will check if it missed something and request it.

    #[instrument(name = "DB Exchange", skip(self, from))]
    async fn db_exchange(self: &Arc<Self>, from: Option<SocketAddr>) {
        
        let node_addresses: Vec<(String, SocketAddr)> = {
            let nodes = self.nodes.lock().await;
            nodes.iter().map(|(id, info)| (id.clone(), info.addr)).collect()
        };
    
        // Create a UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind UDP socket");

        let my_db_metadata = match self.db.get_metadata_json().await {
            Ok(str) => str,
            Err(err) => {
                error!(err);
                return;
            }
        };
        // Make a hash from it, to compare with others
        let db_hash = {
            let mut hasher = Sha256::new();
            hasher.update(my_db_metadata.as_bytes());
            format!("{:x}", hasher.finalize()) // Convert hash to a hex string
        };
    
        for (_, addr) in node_addresses.clone() {
            if addr == self.public_socket.local_addr().unwrap() {
                continue;
            }
            let request_msg = "ReqInternal: UpdateInfo |".to_owned() + &db_hash + "|"+ &self.db.data_version.lock().await.to_string();
            // Send the request to the node
            send_with_retry(&socket, request_msg.as_bytes(), addr, 2).await.unwrap();
        }
    
        for (_, addr) in node_addresses {
            // Receive possible response from the node
            let _ = match recv_reliable(&socket, Some(Duration::from_secs(DEFAULT_TIMEOUT / 2))).await {
                Ok((packet, size, _)) => {
                    // should i check if responser is in node_addresses?
                    
                    // Convert received packet into a JSON string
                    let packet_str = String::from_utf8_lossy(&packet[..size]);

                    // Deserialize JSON: Expecting { "collection_name": { "uuid": { metadata_json } } }
                    let remote_data: HashMap<String, HashMap<Uuid, Metadata>> = match serde_json::from_str(&packet_str) {
                        Ok(data) => data,
                        Err(e) => {
                            error!("Failed to parse received metadata JSON: {:?}", e);
                            continue;
                        }
                    };

                    // Figure out the  needed entries
                    let mut needed_entries: HashMap<String, Vec<Uuid>> = HashMap::new();

                    // Merge metadata for each collection and gather UUIDs of needed entries
                    for (collection_name, remote_metadata) in remote_data.iter() {
                        let required_uuids = self.db.merge_metadata(collection_name, remote_metadata.clone()).await;
                        needed_entries.insert(collection_name.clone(), required_uuids);
                    }

                    // Construct response JSON: { "collection_name": { "uuid": data } }
                    let mut response_data: HashMap<String, HashMap<Uuid, Value>> = HashMap::new();
                    for (collection_name, uuids) in needed_entries.iter() {
                        response_data.insert(collection_name.clone(), self.db.get_entries(collection_name, uuids.to_vec()).await);
                    }
                    
                    // Serialize the response data to JSON string
                    let response_json = match serde_json::to_string(&response_data) {
                        Ok(json) => json,
                        Err(e) => {
                            error!("Failed to serialize response JSON: {:?}", e);
                            "{}".to_string()
                        }
                    };

                    // Send response back
                    match send_reliable(&socket, response_json.as_bytes(), addr).await {
                        Ok(()) => {},
                        Err(e)=> {
                            error!("{}", e)
                        }
                    };
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    error!("Request to {} timed out", addr);
                },
                Err(e) => {
                    error!("Failed to receive response from {}: {:?}", addr, e);
                }
            };
        }
    }


 // election stuff

 /// Elects the leader node based on the lowest load value, breaking ties with the lowest id
    async fn elect_leader(self: &Arc<Self>, for_db:Option<bool>, prev_elected:Option<bool>) {
        info!("{}", "elect_leader1".yellow());
        let mut elected = self.elected.lock().await;
        if !self.db.time_to_update.lock().await.clone() && self.db.data_version.lock().await.clone() > 0 {
            info!("{} {}", "skipped election".yellow(), elected);
            if let Some(elect_val) = prev_elected {
                *elected = elect_val;
            }
            return;
        }
        *self.db.time_to_update.lock().await = false;
        info!("elected locked");
        
        // elected = true if there are no known neighbors
        if self.nodes.lock().await.is_empty() {
            *elected = true;
            println!("{} {}","No neighbors, elected:".yellow(), elected);
        }
        
        // println!("{}", "elect_leader2".yellow());
        if *self.requests.lock().await > 1 {
            self.get_info(None).await; // Retrieve updated info from all nodes
        }
        // println!("{}", "elect_leader3".yellow());
        
        *elected = self.election_alg(for_db).await; // Elect a leader based on load and id values
        info!("{} {}","elected value:".yellow(), elected);
        // *self.electing.lock().await = false;
        let newself = self.clone();
        tokio::spawn(
            async move {
            // Sleep for n seconds
            sleep(Duration::from_secs(1)).await;
            *newself.db.time_to_update.lock().await = true;
        });
    }

    async fn election_alg(self: &Arc<Self>, for_db:Option<bool>) -> bool {
        // println!("{}", "election1".yellow());
        let nodes = self.nodes.lock().await;
        let mut lowest_load = self.load.lock().await.clone();
        let mut elected_node = self.id.lock().await.clone();
        let mut highest_db_version = self.db.data_version.lock().await.clone();
        let my_db_version = highest_db_version.clone();

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
            if my_db_version >= highest_db_version {
                return true;
            }
            else {
                return false;
            }
        }
        return elected;        
    }

    async fn DB_sync_publish(self: &Arc<Self>, op: DBOp, data:Value, db_version:u32) {
        if let Some(packet) = json!({
                "data" : data,
                "db_version" : db_version,
                "op" : op.to_string()
            }).as_str() {

            let node_addresses: Vec<(String, SocketAddr)> = {
                let nodes = self.nodes.lock().await;
                nodes.iter().map(|(id, info)| (id.clone(), info.addr)).collect()
            };

            let socket: UdpSocket = match UdpSocket::bind("0.0.0.0:0").await {
                Ok(socket) => socket,
                Err(e) =>{
                    error!("Couldn't allocate socket in DB_sync_publish: {}", e);
                    return;
                }
            };
            let mut reqmsg = String::from("RegInternal: DBSync |");
            reqmsg.push_str(packet);
            for (_, addr) in node_addresses {
                if addr == self.public_socket.local_addr().unwrap() {
                    continue;
                }
                send_with_retry(&socket, reqmsg.as_bytes(), addr, MAX_RETRIES).await.unwrap_or_default();
            }
        }
    }

    async fn DB_sync_listener(self: &Arc<Self>, msg:&str) {
        let parts: Vec<&str> = msg.splitn(2, '|').collect();
        
        if parts.len() < 2 {
            // If the message doesn't contain at least two parts, log or handle the error.
            eprintln!("Invalid message format: {}", msg);
            return;
        }
        let packet: Value = serde_json::from_str(parts[1]).unwrap_or_else(|e| {
            // If deserialization fails, log the error and return.
            eprintln!("Failed to deserialize message: {}", e);
            Value::Null
        });

        let op = DBOp::from_string(packet["op"].as_str().unwrap_or_default());
        if op.is_none() {
            return;
        }

        match op.unwrap() {
            DBOp::ADD => {

            },
            DBOp::UPDATE => {
                // check if "UUID" entry is in packet["data"]
            },
            DBOp::DELETE => {
                // check if "UUID" entry is in packet["data"]
            },
            DBOp::CREATECOLLECTION => {
                // check if "name" entry is in packet["data"]
                //
            },
            DBOp::DELETECOLLECTION => {
                // check if "name" entry is in packet["data"]
            },
            _ =>{}
        }
    }
 // end election stuff

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_service(
        self: &Arc<Self>, 
        service: &Arc<dyn Service>, 
        args: HashMap<String, String>, 
        addr: SocketAddr
    ) -> Result<(), DistriError> {
        println!("Processing request from client: {}", addr);

        // Establish a connection to the client for sending responses
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        println!("Established connection with client on {}", socket.local_addr()?);

        // Send "OK" message to the client to indicate we are ready to receive data
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        println!("Sent 'OK' message for Encrypt to {}", addr);

        let (aggregated_data, _, _) = recv_reliable(&socket, Some(Duration::from_secs(5))).await?;
        // Increment accepted
        *self.accepted.lock().await += 1;

        // Process the aggregated data
        let deserialized_data = if !aggregated_data.is_empty() {
            Some(service.deserialize_request(aggregated_data)?)
        } else {
            None
        };
    

        // Process the request
        let response_data = service.process(
            args, // Convert args to JSON value
            deserialized_data,
        ).await?;
        // Serialize the response data
        let serialized_response = service.serialize_response(response_data)?;

        // Send response
        send_reliable(&socket, &serialized_response, addr).await?;

        *self.completed.lock().await += 1;
        Ok(())
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_stats(self: &Arc<Self>, addr: SocketAddr) -> Result<(), DistriError> {
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

        let table_stats: String = self.db.get_stats().await.join(", ");


        // Create a human-readable stats report
        let stats_report = format!(
            "Server Stats:\n\
            Requests Recieved: {}\n\
            Accepted Requests: {}\n\
            Completed Tasks: {}\n\
            Server Fails: {}\n\
            Total Task Time: {:.2?}\n\
            Avg Completion Time: {:.2?}\n
            DB Data: {}\n
            DB Version: {}\n",
            
            requests,
            accepted,
            completed,
            failed_times,
            // failures,
            total_time,
            avg_completion_time,
            table_stats,
            self.db.data_version.lock().await.clone(),
        );
    
        // Send the stats report back to the client
        send_with_retry(&socket, stats_report.as_bytes(), addr, MAX_RETRIES).await?;
        println!("done handling connection for: {}", addr);
        Ok(())
    }
    

    /// Retrieves the registered server nodes
    pub fn get_nodes(&self) -> HashMap<String, NodeInfo> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }


    // Cloud DB request handlers
    #[instrument(name = "db_add_table", skip(self, addr))]
    async fn db_add_table(&self, addr: SocketAddr) -> Result<Option<String>, DistriError> { // change return type to option?
        // send ok
        let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        debug!("Sent 'OK' message for AddCollection to {}", addr);
        
        // receive data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    // Successfully received data from the correct client
                    let mut my_data_version = self.db.data_version.lock().await;
                    let collection_name: &str = &String::from_utf8_lossy(&packet);

                    let mut collections = self.collections.lock().await;
                    if collections.contains_key(collection_name) {
                        // reply to sender
                        let response = format!("collection '{}' already exists.", collection_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(Some(response));
                    } else {
                        collections.insert(collection_name.to_string(), Vec::new());
                        // update data version with any change in DB
                        *my_data_version += 1;
                        // reply to sender
                        let response = format!("collection '{}' created.", collection_name);
                        send_reliable(&socket, response.as_bytes(), addr).await?;
                        return Ok(Some(response));
                    }
                },
                Ok((_, _, recv_addr)) => {
                    error!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    error!("Receive operation timed out");
                    return Ok(None);
                },
                Err(e) => {
                    error!("Failed to receive data: {:?}", e);
                    return Ok(None);
                }
            };
        }
        Ok(None)

    }
    // Add an entry to a specific table
    #[instrument(name = "db_add_entry", skip(self, args, addr))]
    async fn db_add_entry(&self, args: HashMap<String, String>,  addr: SocketAddr) -> Result<String, DistriError> { // change to option so that ? delegates errors to above function
        // Process input var
        debug!("IN db_add_entry");
        let collection_name = &args["table"];

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        // send ok
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        debug!("{:?} Sent 'OK' message for AddEntry to {}", socket.local_addr(), addr);

        // recieve data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let _ = match recv_reliable(&socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    let mut my_data_version = self.db.data_version.lock().await;
                    debug!("done recieving");
                    let packet = packet[..size].to_vec();
                    
                    let mut entry: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value, // Only proceed if it's an object
                        Err(e) => {
                            // reply to sender
                            let response = format!("Failed to parse JSON: {:?}", e);
                            send_reliable(&socket, DistriError::ValidationError(response.clone()).to_json().as_bytes(), addr).await?;
                            return Err(DistriError::ValidationError(response));
                        }
                    };

                    let op_result = self.add_doc(collection_name, entry).await.map_err(|e| DistriError::OperationalError(e.to_string()));
                    if op_result.is_ok() {
                        *my_data_version += 1;
                    }
                    // send response back to client
                    send_reliable(&socket, generate_response(op_result).as_bytes(), addr).await?;
                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    error!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    ()
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    error!("Receive operation timed out");
                    return Err(DistriError::NetworkError("Receive operation timed out".to_string()));
                },
                Err(e) => {
                    error!("Failed to receive data: {:?}", e);
                    return Err(DistriError::OperationalError(format!(
                        "Failed to receive data: {:?}",
                        e
                    )));
                }
            };
        }
        error!("Client did not communicate");
        return Err(DistriError::OrchestrationError("Client did not Communicate".to_string()));
    }

    // Update an entry in a specific table
    #[instrument(name = "db_update_entry", skip(self, args, addr))]
    async fn db_update_entry(&self, args: HashMap<String, String>, addr: SocketAddr) -> Result<String, DistriError> {
        // Process input variable
        let collection_name = &args["table"];

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        // Send OK response
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        debug!("{:?} Sent 'OK' message for UpdateEntry to {}", socket.local_addr(), addr);

        // Receive data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let (_, _, _) = match recv_reliable(&socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    debug!("Done receiving");
                    let mut my_data_version = self.db.data_version.lock().await;
                    let packet = packet[..size].to_vec();
                    // let data: String = String::from_utf8_lossy(&packet).trim().to_string();
                    
                    let mut entry_to_update: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value,
                        Err(e) => {
                            error!("Failed to parse JSON: {:?}", e);

                            // Reply to sender
                            let err = DistriError::ValidationError(format!("Error:Failed to parse JSON: {:?}", e));
                            send_reliable(&socket, err.to_json().as_bytes(), addr).await?;
                            return Err(err);
                        }
                    };

                    let op_result= self.update_doc(collection_name, entry_to_update).await.map_err(|e| DistriError::OperationalError(e.to_string()));
                    if op_result.is_ok() {
                        *my_data_version += 1;
                    }
                    send_reliable(&socket, generate_response(op_result.clone()).as_bytes(), addr).await?;
                    
                    return op_result;

                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    error!("Received data from unexpected address: {:?}", recv_addr);
                    // Ignore and continue to wait for correct address
                    (0, 0, recv_addr)
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    error!("Receive operation timed out");
                    return Err(DistriError::NetworkError("Receive operation timed out".to_string()));
                },
                Err(e) => {
                    error!("Failed to receive data: {:?}", e);
                    return Err(DistriError::ValidationError("No variable supplied".to_string()));
                }
            };
        }
        error!("Client did not communicate");
        return Err(DistriError::OrchestrationError("Client did not communicate".to_string()));
    }
    
    // Add an entry to a specific table
    #[instrument(name = "db_delete_entry", skip(self, args, addr))]
    async fn db_delete_entry(&self, args: HashMap<String, String>,  addr: SocketAddr) -> Result<String, DistriError> { // change to option so that ? delegates errors to above function
        // Process input var
        let collection_name = &args["table"];

        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port

        // send ok
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        debug!("Sent 'OK' message for AddEntry to {}", addr);

        // recieve data
        // Loop to ensure we get data from the correct client
        for _ in 0..5 {
            let _ = match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
                Ok((packet, size, recv_addr)) if recv_addr == addr => {
                    let mut my_data_version = self.db.data_version.lock().await;
                    let packet = packet[..size].to_vec();
                    
                    let entry: Value = match serde_json::from_slice(&packet) {
                        Ok(value) => value, // Only proceed if it's an object
                        Err(e) => {
                            error!("Failed to parse JSON: {:?}", e);

                            // reply to sender
                            let response = format!("Failed to parse JSON: {:?}", e);
                            send_reliable(&socket, DistriError::ValidationError(response.clone()).to_json().as_bytes(), addr).await?;
                            return Err(DistriError::ValidationError(response));
                        }
                    };

                    let op_result = self.delete_doc(collection_name, entry).await.map_err(|e| DistriError::OperationalError(e.to_string()));
                    if op_result.is_ok() {
                        *my_data_version += 1;
                    }
                    send_reliable(&socket, generate_response(op_result).as_bytes(), addr).await?;
                }, // Successfully received data
                Ok((_, _, recv_addr)) => {
                    // Ignore and continue to wait for correct address
                    ()
                },
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(DistriError::NetworkError("Receive operation timed out".to_string()));
                },
                Err(e) => {
                    return Err(e.into());
                }
            };
        }
        error!("Client did not communicate");
        return Err(DistriError::OrchestrationError("Client did not communicate".to_string()));
    }
    

    // Read a table and return it as a JSON array
    #[instrument(name = "db_read_table", skip(self, args, addr))]
    async fn db_read_table(&self, args: HashMap<String, String>, addr: SocketAddr) -> Result<String, DistriError> {
        // Extract table name from the received packet as a string
        let collection_name = &args["table"];
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        // send ok
        send_with_retry(&socket, b"OK", addr, MAX_RETRIES).await?;
        debug!("Sent 'OK' message for AddEntry to {}", addr);

        // if there is filtering info
        match recv_reliable(&socket, Some(Duration::from_secs(1))).await {
            Ok((packet, size, recv_addr)) if recv_addr == addr => {
                let packet = packet[..size].to_vec();
                
                let entry: Value = match serde_json::from_slice(&packet) {
                    Ok(value) => value, // Only proceed if it's an object
                    Err(e) => {
                        error!("Failed to parse JSON: {:?}", e);

                        // reply to sender
                        let response = format!("Failed to parse JSON: {:?}", e);
                        send_reliable(&socket, DistriError::ValidationError(response.clone()).to_json().as_bytes(), addr).await?;
                        return Err(DistriError::ValidationError(response));
                    }
                };
                let _my_db_version = self.db.data_version.lock().await;
                let op_result = self.db.get_entries(collection_name, entries)
                let op_result: Result<_, DistriError> = self.read_docs(collection_name, entry).await.map_err(|e| DistriError::OperationalError(e.to_string()));
                send_reliable(&socket, generate_response(op_result).as_bytes(), addr).await?;
            },
            Ok((_, _, recv_addr)) => {
                warn!("Received data from unexpected address: {:?}", recv_addr);
                // // Ignore and continue to wait for correct address
                ()
            },
            Err(_) => {
                ()
            }
        };
        error!("Client did not communicate");
        Err(DistriError::OrchestrationError("Client did not communicate".to_string()))
    }


}
