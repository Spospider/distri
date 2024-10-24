use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::path::Path;
use std::net::SocketAddr;
use std::error::Error;
use tokio::fs::File;
use crate::utils::{END_OF_TRANSMISSION, server_decrypt_img};
use tokio::io::{AsyncReadExt, AsyncWriteExt};


pub struct Client {
    nodes: HashMap<String, SocketAddr>,  // Keeps track of server nodes
    chunk_size: usize,
}

impl Default for Client {
    fn default() -> Self {
        Client {
            nodes: HashMap::new(),
            chunk_size: 1024,
        }
    }
}

impl Client {
    /// Creates a new client instance
    pub fn new(nodes: Option<HashMap<String, SocketAddr>>, chunk_size: Option<usize>) -> Self {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let final_chunk_size: usize = chunk_size.unwrap_or(1024);

        Client {
            nodes: initial_nodes,
            chunk_size: final_chunk_size,
        }
    }

    /// Registers a server node with its name and address
    pub fn register_node(&mut self, name: String, address: SocketAddr) {
        self.nodes.insert(name, address);
    }

    /// and waits for the server's response.
    pub async fn send_data(&self, file_path: Option<&Path>) -> Result<(), Box<dyn Error>> {
        
        // Create a UDP socket for sending and receiving messages
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
        
        let request_message = "Request: Encrypt".to_string(); // The message to be sent

        // Multicast the message to all nodes
        for node_addr in self.nodes.values() {
            let node_addr = node_addr.clone(); // Clone the address

            match socket.send_to(request_message.as_bytes(), &node_addr).await {
                Ok(_) => {
                    println!("Request message sent to {}", node_addr);
                }
                Err(e) => {
                    eprintln!("Failed to send message to {}: {:?}", node_addr, e);
                }
            }
        }

        // Buffer for receiving data
        let mut buffer = [0u8; 1024];

        // Now, listen for the first response that comes back from any node
        let (size, addr) = socket.recv_from(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..size]);
        if response == "OK" {
            println!("request for service accepted from {}", addr);

            let mut send_buffer= vec![0u8; self.chunk_size];

            // Send the image in chunks
            let mut chunk_index = 0;
            let mut file = File::open(file_path.unwrap()).await?;

            loop {
                let n = file.read(&mut send_buffer).await?;
                if n == 0 {
                    break; // EOF reached
                }
                socket.send_to(&send_buffer[..n], &addr).await?;
                println!("Sent chunk {} of size {}", chunk_index, n);
                chunk_index += 1;
            }

            // Send end-of-image signal
            socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
            println!("End of image signal sent.");

            // start receiving result
            self.await_result(socket, addr).await;
        }
        

        Ok(())
    }

    async fn await_result(&self, socket:UdpSocket, sender: SocketAddr) {
        // Receive the steganography result (image with hidden content) from the server
        let mut output_image_data = Vec::new();
        let mut buffer: Vec<u8> = vec![0u8; self.chunk_size];
        loop {
            let (n, _addr) = socket.recv_from(&mut buffer).await.unwrap();
            if _addr != sender {
                // ignore packets from other senders
                continue;
            }
            if &buffer[..n] == END_OF_TRANSMISSION.as_bytes() {
                println!("End of transmission signal received.");
                break;
            }

            output_image_data.extend_from_slice(&buffer[..n]);
            println!("Received chunk of size {}", n);
        }

        // Write the received image with hidden data to a file
        let mut output_image_file = File::create("files/received_with_hidden.png").await.unwrap();
        output_image_file.write_all(&output_image_data).await.unwrap();
        println!("Received image with hidden data saved as 'received_with_hidden.png'.");


        // Extract the hidden image from the received image
        server_decrypt_img("files/received_with_hidden.png", "files/extracted_hidden_image.jpg").await;
        println!("Hidden image extracted and saved as 'files/extracted_hidden_image.jpg'.");
    }
}