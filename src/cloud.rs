use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::atomic::{AtomicU32, Ordering};

use std::collections::HashMap;
// use std::os::unix::net::SocketAddr;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use std::time::{Duration, Instant};
use crate::utils::{END_OF_TRANSMISSION, server_encrypt_img};

pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    // internal_socket: Arc<UdpSocket>,
    // Async callback that now returns a future
    // callback: Callback,
    elected: bool,
    failed: bool,

    // For stats
    accepted:Arc<Mutex<u32>>,
    completed:Arc<Mutex<u32>>,
    failed_number_of_times:Arc<Mutex<u32>>,
    failures:Arc<Mutex<u32>>,
    total_task_time:Arc<Mutex<Duration>>,
    
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        // callback: impl Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Vec<u8>> + Send>> + Send + Sync + 'static,
        address: SocketAddr,
        nodes: Option<HashMap<String, SocketAddr>>,
        elected: bool,
    ) -> Result<Self> {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let socket = UdpSocket::bind(address).await?;

        // (address: SocketAddr)use address ip only as str here
        // let internal_socket: SocketAddr = format!("{}:{}", address.ip().to_string(), 4444).parse().unwrap();

        Ok(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            // callback: Arc::new(callback),
            elected,
            // internal_socket: Arc::new(UdpSocket::bind(internal_socket).await?),
            failed:false,

            // Stats init
            accepted: Arc::new(Mutex::new(0)),
            completed: Arc::new(Mutex::new(0)),
            failed_number_of_times: Arc::new(Mutex::new(0)),
            failures: Arc::new(Mutex::new(0)),
            total_task_time: Arc::new(Mutex::new(Duration::default())),
        })
    }
    /// Starts the server, listens for new connections, and processes data
    pub async fn serve(self: Arc<Self>) -> Result<()> {
        let mut buffer = vec![0u8; 65535]; // Buffer to hold incoming UDP packets

        loop {
            // Wait for incoming data
            let (size, addr) = self.public_socket.recv_from(&mut buffer).await?;

            // Clone buffer data to process it in a separate task
            let packet = buffer[..size].to_vec();
            let node = self.clone();

            //  to check different services
            let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&packet[..size]);

            if (self.elected && !self.failed) || received_msg == "Request: Stats" { // accept stats request always
                // self.accepted += 1;
                // self.accepted.fetch_add(1, Ordering::SeqCst);
                *self.accepted.lock().await += 1;

                // Spawn a task to handle the connection and data processing
                let node_clone = Arc::clone(&node);  // an alternative to using self, to not cause ownership errors
                tokio::spawn(async move {
                    let start_time = Instant::now(); // Record start time
                    if let Err(e) = node.handle_connection(packet, size, addr).await {
                        eprintln!("Error handling connection: {:?}", e);
                        
                        *node_clone.failures.lock().await += 1; // how to make this not cause an error
                    }
                    else{
                        let elapsed = start_time.elapsed();

                        // Accumulate the elapsed time into total_task_time
                        *node_clone.total_task_time.lock().await += elapsed;
                    }

                     
                });
                
            }
        }
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_connection(self: Arc<Self>, data: Vec<u8>, size: usize, addr: SocketAddr) -> Result<()> {
        // Convert incoming bytes to a string to parse JSON
        let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&data[..size]);

        println!("Got request from {}: {}", addr, received_msg);

        if received_msg == "Request: Encrypt" {
            println!("Processing request from client: {}", addr);

            // Establish a connection to the client for sending responses
            let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
            println!("Established connection with client on {}", socket.local_addr()?);

            // Send "OK" message to the client to indicate we are ready to receive data
            socket.send_to(b"OK", &addr).await?;
            println!("Sent 'OK' message to {}", addr);

            // Buffer to receive chunks of data
            let mut buffer = [0u8; 1024];
            let mut aggregated_data = Vec::new(); // Aggregate the incoming data

            // Receive data in chunks from the client
            loop {
                let (size, _) = socket.recv_from(&mut buffer).await?;
                let received_data = String::from_utf8_lossy(&buffer[..size]);

                // Check for the end of transmission message
                if received_data == END_OF_TRANSMISSION {
                    println!("End of transmission from client {}", addr);
                    break;
                }
                // Append the received chunk to the aggregated_data buffer
                aggregated_data.extend_from_slice(&buffer[..size]);
                println!("Received chunk from {}: {} bytes", addr, size);
            }

            // Process the aggregated data using the async callback
            // let callback = self.callback.clone();
            // let callback_fn = cb.lock().await; // Lock the callback to execute it

            // Call the async callback and await the result
            let processed_data = self.process(aggregated_data).await; // Call and await the async callback

            // let processed_data: Vec<u8> = (callback_fn)(aggregated_data).await;

            // Send the processed data back to the client in chunks
            let chunk_size = 1024; // Define chunk size for sending the response
            for chunk in processed_data.chunks(chunk_size) {
                socket.send_to(chunk, &addr).await?;
                println!("Sent chunk of {} bytes back to {}", chunk.len(), addr);
            }
            socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
            println!("Task for client done: {}", addr);
            *self.completed.lock().await += 1;
        }
        else if received_msg == "Request: Stats" {
            //send a report of the stats for vars:
                // accepted:Arc<Mutex<u32>>,
                // completed:Arc<Mutex<u32>>,
                // failed_number_of_times:Arc<Mutex<u32>>,
                // failures:Arc<Mutex<u32>>,
                // total_task_time:Arc<Mutex<Duration>>,
            // back to the client in a human readable format
            println!("Processing stats request from client: {}", addr);

            // Establish a connection to the client for sending responses
            let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
        
            // Lock and retrieve values from the shared stats variables
            let accepted = *self.accepted.lock().await;
            let completed = *self.completed.lock().await;
            let failed_times = *self.failed_number_of_times.lock().await;
            let failures = *self.failures.lock().await;
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
            socket.send_to(stats_report.as_bytes(), &addr).await?;
            println!("Sent stats report to client {}", addr);
        
        }
        Ok(())
    }

    /// Retrieves the registered server nodes
    pub fn get_nodes(&self) -> HashMap<String, SocketAddr> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }

    async fn process(&self, data: Vec<u8>) -> Vec<u8> {
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
    
        // Return the encrypted data as the output
        return encrypted_data
    }
    
}