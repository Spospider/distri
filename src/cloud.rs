use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use crate::utils::{END_OF_TRANSMISSION};
use base64::decode;


pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    chunk_size: usize,
    callback: Arc<Mutex<dyn Fn(Vec<u8>) -> Vec<u8> + Send + 'static>>,
    elected:bool
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        callback: Fn(Vec<u8>) -> Vec<u8>,  // The callback for handling received data
        address: SocketAddr, 
        nodes: Option<HashMap<String, SocketAddr>>, 
        chunk_size: usize,
        elected:bool
    ) -> Result<Self> {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let socket = UdpSocket::bind(address).await?; // Bind to the specified address

        Ok(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size,
            callback,
            elected,
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

            if self.elected {
                // Spawn a task to handle the connection and data processing
                tokio::spawn(async move {
                    if let Err(e) = node.handle_connection(packet, size, addr).await {
                        eprintln!("Error handling connection: {:?}", e);
                    }
                });
            }
        }
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_connection(self: Arc<Self>, data: Vec<u8>, size:usize, addr: SocketAddr) -> Result<()> {
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

            // Process the aggregated data using the callback
            let cb = self.callback.clone();
            let callback_fn = cb.lock().await; // Lock the callback to execute it

            let processed_data: Vec<u8> = (callback_fn)(aggregated_data); // Call the callback with the aggregated data

            // Send the processed data back to the client in chunks
            let chunk_size = 1024; // Define chunk size for sending the response
            for chunk in processed_data.chunks(chunk_size) {
                socket.send_to(chunk, &addr).await?;
                println!("Sent chunk of {} bytes back to {}", chunk.len(), addr);
            }
            socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
            println!("Task for client done: {}", addr);
        }

        Ok(())
    }

    /// Retrieves the registered server nodes
    pub fn get_nodes(&self) -> HashMap<String, SocketAddr> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }
}