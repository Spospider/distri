use tokio::net::{UdpSocket, TcpListener};
use tokio::sync::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use std::time::{Duration, Instant};
use crate::utils::{DEFAULT_TIMEOUT, END_OF_TRANSMISSION, server_encrypt_img, send_with_retry, recv_with_ack};
use rand::Rng;

#[derive(Clone)]
pub struct NodeInfo {
    pub mem: u16,
    pub id: u16,
    pub addr: SocketAddr,
}

pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  
    public_socket: Arc<UdpSocket>,
    chunk_size: Arc<usize>,
    elected: Arc<Mutex<bool>>,   
    failed: Arc<Mutex<bool>>,
    mem: Arc<Mutex<u16>>,
    id: Arc<Mutex<u16>>, 

    // Stats
    accepted: Arc<Mutex<u32>>,
    completed: Arc<Mutex<u32>>,
    failed_number_of_times: Arc<Mutex<u32>>,
    failures: Arc<Mutex<u32>>,
    total_task_time: Arc<Mutex<Duration>>,
}

impl CloudNode {
    pub async fn new(
        address: SocketAddr,
        nodes: Option<HashMap<String, SocketAddr>>,
        chunk_size: usize,
        elected: bool,
    ) -> Result<Arc<Self>> {
        let initial_nodes: HashMap<String, NodeInfo> = nodes
            .unwrap_or_else(HashMap::new)
            .into_iter()
            .map(|(name, addr)| (
                name,
                NodeInfo {
                    mem: 0,
                    id: addr.port(),
                    addr,
                },
            ))
            .collect();

        let socket = UdpSocket::bind(address).await?;
        let failed = false;

        Ok(Arc::new(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size: Arc::new(chunk_size),
            elected: Arc::new(Mutex::new(elected)),
            failed: Arc::new(Mutex::new(failed)),
            mem: Arc::new(Mutex::new(0)),
            id: Arc::new(Mutex::new(address.port())),
            accepted: Arc::new(Mutex::new(0)),
            completed: Arc::new(Mutex::new(0)),
            failed_number_of_times: Arc::new(Mutex::new(0)),
            failures: Arc::new(Mutex::new(0)),
            total_task_time: Arc::new(Mutex::new(Duration::default())),
        }))
    }

    pub async fn serve(self: &Arc<Self>) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.public_socket.local_addr()?).await?;
        println!("Listening for incoming info requests on {:?}", listener.local_addr());

        self.elect_leader().await;

        let mut buffer = vec![0u8; 65535];
        let mut processed_ids = HashSet::new();

        loop {
            println!("looping0");
            // let (size, addr) = self.public_socket.recv_from(&mut buffer).await?;
            let (size, addr, _packet_id) = match recv_with_ack(&self.public_socket, &mut buffer, &mut processed_ids).await {
                Ok((size, addr, packet_id)) => (size, addr, packet_id),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    continue;
                },
                Err(e) => {
                    eprintln!("Failed to receive data: {:?}", e);
                    continue;
                }
            };
            println!("looping1");
             // Clone buffer data to process it in a separate task
            let packet = buffer[..size].to_vec();
            

            let random_value = rand::thread_rng().gen_range(0..=10);
            println!("looping2");
            // If the random value is 0, do something
            if random_value == 0 && !*self.failed.lock().await  {  // start failure election
                println!("looping2.1");
                let mut failed = self.failed.lock().await;
                *failed = false; // Reset election state initially
                println!("looping2.11");
                self.get_info(); // Retrieve updated info from all nodes
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
            self.elect_leader();
            println!("looping5");
            //  to check different services
            let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&packet[..size]);
            println!("looping6");
            if received_msg == "Request: Stats" || received_msg == "Request: Encrypt"  {

                if *self.elected.lock().await || received_msg == "Request: Stats" { // accept stats request always
                    println!("looping7");
                    if received_msg == "Request: Encrypt" {
                        *self.accepted.lock().await += 1;
                    }

                    // Spawn a task to handle the connection and data processing
                    println!("looping8");
                    // let node_clone = Arc::clone(&node);  // an alternative to using self, to not cause ownership errors
                    let node = self.clone();
                    tokio::spawn(async move {
                        let start_time = Instant::now();
                        if let Err(e) = node.handle_connection(packet, size, addr).await {
                            eprintln!("Error handling connection: {:?}", e);
                            *node.failures.lock().await += 1; 
                        }
                        else{
                            let elapsed: Duration = start_time.elapsed();
                            // Accumulate the elapsed time into total_task_time
                            *node.total_task_time.lock().await += elapsed;
                        }
                    });
                }
            }
            else if received_msg == "Request: UpdateInfo" {
                println!("received UpdateInfo");
                if let Err(e) = self.handle_info_request(addr).await {
                    eprintln!("Error handling info connection: {:?}", e);
                }
                
                // tokio::spawn(async move {
                //     if let Err(e) = node.handle_info_request(addr).await {
                //         eprintln!("Error handling info connection: {:?}", e);
                //     }
                //     println!("responded to getinfo")
                    
                // });
            }
        }
    }

    async fn get_info(self: &Arc<Self>) {
        let node_addresses: Vec<(String, SocketAddr)> = {
            let nodes = self.nodes.lock().await;
            nodes.iter().map(|(id, info)| (id.clone(), info.addr)).collect()
        };

        let socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind UDP socket");

        for (node_id, addr) in node_addresses {
            if addr == self.public_socket.local_addr().unwrap() {
                continue;
            }
            let request_msg = "Request: UpdateInfo";
            println!("sending getinfo to {}", addr);

            send_with_retry(&socket, request_msg.as_bytes(), addr, 10, 5).await.unwrap();
            println!("sent UpdateInfo");

            let mut buffer = [0u8; 1024];
            
            match recv_with_ack(&socket, &mut buffer, &mut HashSet::new()).await {
                Ok((size, _, _packet_id)) => {
                    let updated_mem: u16 = buffer[..size].get(0).copied().unwrap_or(0).into();
                    let mut nodes = self.nodes.lock().await;
                    if let Some(node_info) = nodes.get_mut(&node_id) {
                        node_info.mem = updated_mem;
                        println!("Updated info for node {}: mem = {}", node_info.id, node_info.mem);
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

    async fn handle_info_request(self: &Arc<Self>, addr: SocketAddr) -> Result<()> {
        println!("Received info request from {}", addr);

        let response = format!("{} {}", self.mem.lock().await, self.id.lock().await);
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        send_with_retry(&socket, response.as_bytes(), addr, 5, 5).await?;
        println!("Sent info response to {}: {}", addr, response);

        Ok(())
    }

    async fn elect_leader(self: &Arc<Self>) {
        // let clone = self.clone();
        
        // *elected = false; // Reset election state initially
        self.get_info().await; // Retrieve updated info from all nodes
        let mut elected = self.elected.lock().await;
        *elected = self.election_alg().await; // Elect a leader based on mem and id values
        println!("elected value: {}", elected);
    }

    async fn election_alg(self: &Arc<Self>) -> bool {
        println!("election1");
        let nodes = self.nodes.lock().await;
        let mut lowest_mem = *self.mem.lock().await;
        let mut elected_node = *self.id.lock().await;
        println!("election2");

        for (_, node_info) in nodes.iter() {
            if node_info.mem < lowest_mem || (node_info.mem == lowest_mem && node_info.id < elected_node) {
                lowest_mem = node_info.mem;
                elected_node = node_info.id;
            }
        }
        println!("election3");

        // let mut elected = self.elected.lock().await;
        // *elected = self.id == elected_node;
        let elected = *self.id.lock().await == elected_node;
        println!("elected node: {}", elected_node);
        elected
    }

    async fn handle_connection(self: &Arc<Self>, data: Vec<u8>, size: usize, addr: SocketAddr) -> Result<()> {
        let received_msg = String::from_utf8_lossy(&data[..size]);

        println!("Got request from {}: {}", addr, received_msg);

        if received_msg == "Request: Encrypt" {
            println!("Processing request from client: {}", addr);
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            send_with_retry(&socket, b"OK", addr, 5, 5).await?;

            let mut buffer = [0u8; 1024];
            let mut aggregated_data = Vec::new();

            loop {
                let (size, _, _packet_id) = recv_with_ack(&socket, &mut buffer, &mut HashSet::new()).await?;
                
                let received_data = String::from_utf8_lossy(&buffer[..size]);
                if received_data == END_OF_TRANSMISSION {
                    println!("End of transmission from client {}", addr);
                    break;
                }
                aggregated_data.extend_from_slice(&buffer[..size]);
                println!("Received chunk from {}: {} bytes", addr, size);
            }

            let processed_data = self.process(aggregated_data).await;

            for chunk in processed_data.chunks(1024) {
                send_with_retry(&socket, chunk, addr, 5, 5).await?;
                println!("Sent chunk of {} bytes back to {}", chunk.len(), addr);
            }
            send_with_retry(&socket, END_OF_TRANSMISSION.as_bytes(), addr, 5, 5).await?;
            println!("Task for client done: {}", addr);
            *self.completed.lock().await += 1;
        }
        else if received_msg == "Request: Stats" {
            println!("Processing stats request from client: {}", addr);
            let socket = UdpSocket::bind("0.0.0.0:0").await?;

            let accepted = *self.accepted.lock().await;
            let completed = *self.completed.lock().await;
            let failed_times = *self.failed_number_of_times.lock().await;
            let failures = *self.failures.lock().await;
            let total_time = *self.total_task_time.lock().await;

            let avg_completion_time = if completed > 0 {
                total_time / completed as u32
            } else {
                Duration::from_secs(0)
            };

            let stats_report = format!(
                "Server Stats:\n\
                Accepted Requests: {}\n\
                Completed Tasks: {}\n\
                Failed Attempts: {}\n\
                Failures: {}\n\
                Total Task Time: {:.2?}\n\
                Avg Completion Time: {:.2?}\n",
                accepted,
                completed,
                failed_times,
                failures,
                total_time,
                avg_completion_time,
            );
        
            // Send the stats report back to the client
            // socket.send_to(stats_report.as_bytes(), &addr).await?;
            send_with_retry(&socket, stats_report.as_bytes(), addr, 5, 5).await?;
        }
        println!("done handling connection for: {}", addr);
        Ok(())
    }

    pub async fn process(&self, data: Vec<u8>) -> Vec<u8> {
        let img_path = "files/to_encrypt.jpg";
        let output_path = "files/encrypted_output.png";
    
        let mut file = File::create(img_path).await.unwrap();
        file.write_all(&data).await.unwrap();
    
        server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;
    
        let mut encrypted_file = File::open(output_path).await.unwrap();
        let mut encrypted_data = Vec::new();
        encrypted_file.read_to_end(&mut encrypted_data).await.unwrap();
    
        encrypted_data
    }
}