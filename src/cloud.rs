use std::net::{UdpSocket, SocketAddr};
use std::time::Duration;
use sha2::{Sha256, Digest}; // For checksum verification
use std::str;
use std::io::Result;

use crate::utils::reconstruct_data;  // Import the helper function from utils.rs
use crate::utils::chunk_data;

struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, SocketAddr>>>,  // Keeps track of other cloud nodes
    public_socket: UdpSocket,
    chunk_size: u32,
    callback: Arc<Mutex<dyn Fn(Vec<u8>) + Send + 'static>>,
}

impl CloudNode {
    pub fn new(
        callback: Arc<Mutex<dyn Fn(Vec<u8>) + Send + 'static>>,  // The callback for handling received data
        address: SocketAddr, 
        nodes: Option<HashMap<String, SocketAddr>>, 
        chunk_size:u32
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let socket = UdpSocket::bind(address)?; // Bind to port 12345
        let final_chunk_size = chunk_size.unwrap_or(1024);

        Ok(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size: final_chunk_size,
            callback,
        })
    }

    /// Starts the server in a separate thread and listens for incoming data
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let socket = self.public_socket.clone(); // Clone the socket for the new thread
        let chunk_size = self.chunk_size;
        let callback = self.callback.clone(); // Clone the callback to pass to the thread

        // Spawn a new thread to run the server in the background
        thread::spawn(move || {
            let mut buffer = vec![0u8; chunk_size as usize];
            let mut assembled_data: HashMap<u32, Vec<Vec<u8>>> = HashMap::new();
            let mut last_seq_num: u32 = 0;
            let mut current_group: u32 = 0;

            loop {
                match socket.recv_from(&mut buffer) {
                    Ok((amt, src)) => {
                        println!("Received {} bytes from {}", amt, src);

                        let sequence_number = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                        let group_number = sequence_number / (u32::MAX / chunk_size);

                        // Detect wrap-around of sequence numbers
                        if sequence_number < last_seq_num {
                            current_group += 1;
                        }
                        last_seq_num = sequence_number;

                        // Store chunks by group number
                        assembled_data.entry(group_number)
                            .or_insert_with(Vec::new)
                            .push(buffer[..amt].to_vec());

                        // End of transmission detection logic
                        if amt < chunk_size as usize {
                            break;
                        }
                    },
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Timeout or non-blocking read didn't receive data
                        continue;
                    },
                    Err(e) => {
                        println!("Error receiving data: {}", e);
                        break;
                    }
                }
            }

            // After receiving, reconstruct the data
            let reassembled_data = match reconstruct_data(assembled_data, self.chunk_size) {
                Ok(data) => data,
                Err(e) => {
                    println!("Error reconstructing data: {}", e);
                    return;
                }
            };

            // Invoke the callback with the reconstructed data
            let callback = callback.lock().unwrap();
            callback(reassembled_data);
        });

        Ok(())
    }
    
    pub fn get_nodes(&self) -> HashMap<String, SocketAddr> {
        let copy = self.nodes.lock().unwrap().clone();
        return copy;
    }
}