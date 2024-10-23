use tokio::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::Path;
use std::net::SocketAddr;
use std::error::Error;
use crate::utils::serialize_data;

pub struct Client {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of server nodes
    chunk_size: u32,
}

impl Default for Client {
    fn default() -> Self {
        Client {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            chunk_size: 1024,
        }
    }
}

impl Client {
    /// Creates a new client instance
    pub fn new(nodes: Option<HashMap<String, SocketAddr>>, chunk_size: Option<u32>) -> Self {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let final_chunk_size: u32 = chunk_size.unwrap_or(1024);

        Client {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            chunk_size: final_chunk_size,
        }
    }

    /// Registers a server node with its name and address
    pub fn register_node(&self, name: String, address: SocketAddr) {
        let mut nodes = self.nodes.lock().unwrap();
        nodes.insert(name, address);
    }

    /// Sends data (message and optional file) to a registered server node via UDP
    /// and waits for the server's response.
    pub async fn send_data(&self, message: &str, file_path: Option<&Path>, server_name: &str) -> Result<(), Box<dyn Error>> {
        // Get the server address from the nodes list
        let nodes = self.nodes.lock().unwrap();
        let server_addr = match nodes.get(server_name) {
            Some(addr) => addr,
            None => return Err("Server not found".into()),
        };

        // Serialize the message and file into a JSON string
        let serialized_data = serialize_data(message, file_path)?;

        // Create the UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
        socket.send_to(serialized_data.as_bytes(), server_addr).await?;

        // Wait for the server's response
        let mut buf = [0u8; 1024];
        let (size, _) = socket.recv_from(&mut buf).await?;
        let response = String::from_utf8(buf[..size].to_vec())?;

        println!("Received response from server: {}", response);
        Ok(())
    }
}