use std::net::{UdpSocket, SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sha2::{Sha256, Digest};  // For checksum (hashing)
use std::time::Duration;

use crate::utils::reconstruct_data;  // Import the helper function from utils.rs
use crate::utils::chunk_data;


pub struct Client {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of server nodes
    chunk_size: u32,
}
// Implementing Default for Client
impl Default for Client {
    fn default() -> Self {
        Client {
            nodes: HashMap::new(),  // Default to an empty HashMap
            chunk_size: 1024,       // Default chunk size to 1024
        }
    }
}

impl Client {
    /// Creates a new client instance
    pub fn new(nodes: Option<HashMap<String, SocketAddr>>, chunk_size: Option<u32>) -> Self {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let final_chunk_size = chunk_size.unwrap_or(1024);

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

    /// Sends data to all registered server nodes (multicast using UDP)
    pub fn send_bytes(&self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let nodes = self.nodes.lock().unwrap();
        let socket = UdpSocket::bind("0.0.0.0:0")?; // Bind to any available port

        // Chunk the data if it's too large
        let chunked_data = chunk_data(data, 1024); // Assuming max chunk size of 1024 bytes
        for chunk in chunked_data {
            // Send each chunk to each server node
            for addr in nodes.values() {
                socket.send_to(&chunk, addr)?; 
            }
        }

        Ok(())
    }

    /// Receives data from server nodes and reassembles chunks
    pub fn receive_bytes(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind("0.0.0.0:12345")?; // Bind to a specific port
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;

        let mut buffer = [0; 1024];
        let mut received_chunks  = Vec::new();
        
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((amt, _src)) => {
                    received_chunks.push(buffer[..amt].to_vec());

                    // End of transmission logic (assuming chunk size check)
                    if amt < 1024 {  // If data received is smaller than the chunk size, assume it's the last packet
                        break;
                    }
                },
                // Handle timeout error: WouldBlock means the operation timed out
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    println!("Timed out waiting for response.");
                    break;
                },
                Err(e) => return Err(Box::new(e)),  // Return other errors
            }
        }

        // Reassemble the chunks and verify integrity
        let data = reconstruct_data(received_chunks, 1024)?;
        Ok(data)
    }

    /// Sends a string message to all registered server nodes (multicast using UDP)
    pub fn send_message(&self, data: String) -> Result<(), Box<dyn std::error::Error>> {
        // Convert the string to bytes
        let bytes = data.into_bytes();
        self.send_bytes(&bytes)  // Reuse the send_bytes function
    }    
    /// Receives and reassembles a string message from server nodes
    pub fn receive_message(&self) -> Result<String, Box<dyn std::error::Error>> {
        let bytes = self.receive_bytes()?;  // Reuse the receive_bytes function

        // Convert the received bytes back to a string
        let message = String::from_utf8(bytes)?;
        Ok(message)
    }

    pub fn get_nodes(&self) -> HashMap<String, SocketAddr> {
        let copy = self.nodes.lock().unwrap().clone();
        return copy;
    }
}
