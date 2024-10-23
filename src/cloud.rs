use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use crate::utils::{deserialize_data, DataPacket};
use base64::decode;

pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    chunk_size: u32,
    callback: Arc<Mutex<dyn Fn(Vec<u8>) + Send + 'static>>,
}

impl CloudNode {
    /// Creates a new CloudNode
    pub fn new(
        callback: Arc<Mutex<dyn Fn(Vec<u8>) + Send + 'static>>,  // The callback for handling received data
        address: SocketAddr, 
        nodes: Option<HashMap<String, SocketAddr>>, 
        chunk_size: u32,
    ) -> Result<Self> {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let socket = UdpSocket::bind(address)?; // Bind to the specified address

        Ok(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size,
            callback,
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

            // Spawn a task to handle the connection and data processing
            tokio::spawn(async move {
                if let Err(e) = node.handle_connection(packet, addr).await {
                    eprintln!("Error handling connection: {:?}", e);
                }
            });
        }
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_connection(self: Arc<Self>, data: Vec<u8>, addr: SocketAddr) -> Result<()> {
        // Convert incoming bytes to a string to parse JSON
        let json_data = String::from_utf8(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Deserialize the JSON data into the DataPacket structure
        match deserialize_data(&json_data) {
            Ok(data_packet) => {
                println!("Received message: {}", data_packet.message);

                // If a file is included, decode and handle it
                if let Some(file_data) = data_packet.file {
                    println!("Received file: {}", file_data.file_name);
                    let decoded_file = decode(&file_data.file_content).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                    // Execute the callback with the file contents
                    let callback = self.callback.lock().await;
                    (callback)(decoded_file);  // Call the callback function with the file data
                }

                // Send a response back to the client
                let response_message = format!("Acknowledged: {}", data_packet.message);
                self.public_socket.send_to(response_message.as_bytes(), addr).await?;
            }
            Err(e) => eprintln!("Failed to deserialize data: {:?}", e),
        }

        Ok(())
    }

    /// Retrieves the registered server nodes
    pub fn get_nodes(&self) -> HashMap<String, SocketAddr> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }
}