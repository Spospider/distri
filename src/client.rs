use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::error::Error;
use tokio::fs::File;
use crate::utils::{END_OF_TRANSMISSION, DEFAULT_TIMEOUT, server_decrypt_img, send_with_retry, recv_with_timeout, recv_reliable, send_reliable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Duration;
use std::collections::HashSet;

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

    pub async fn send_data(&self, file_path: &str, service: &str) -> Result<(), Box<dyn Error>> {
        // Create a UDP socket for sending and receiving messages
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        
        let request_message = format!("Request: {}", service);

        // Send the request to each node
        for &node_addr in self.nodes.values() {
            println!("Sending request to {}", node_addr);

            if let Err(e) = send_with_retry(&socket, request_message.as_bytes(), node_addr, 5, 5).await {
                eprintln!("Failed to send message to {}: {:?}", node_addr, e);
            } else {
                println!("Request message sent to {}", node_addr);
            }
        }

        // Buffer for receiving data
        let mut buffer = [0u8; 1024];

        // Listen for the first response that comes back from any node
        let (size, addr, _packet_id) = recv_with_ack(&socket, &mut buffer, &mut HashSet::new()).await?;

        let response = String::from_utf8_lossy(&buffer[..size]);
        if response == "OK" {
            println!("request for service accepted from {}", addr);

            // let mut send_buffer= vec![0u8; self.chunk_size];

            // Send the image in chunks
            // let mut chunk_index = 0;
            let mut file = File::open(file_path).await?;
            let mut data = Vec::new();
            file.read_to_end(&mut data).await?;

            // loop {
            //     let n = file.read(&mut send_buffer).await?;
            //     if n == 0 {
            //         break; // EOF reached
            //     }
            //     // socket.send_to(&send_buffer[..n], &addr).await?;
            //     send_with_retry(&socket, &send_buffer[..n], addr, 5).await?;
            //     println!("Sent chunk {} of size {}", chunk_index, n);
            //     chunk_index += 1;
            // }
            send_reliable(&socket, &data, addr).await?;

            // Send end-of-image signal
            // socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
            // send_with_retry(&socket, END_OF_TRANSMISSION.as_bytes(), addr, 5).await?;
            // println!("End of image signal sent.");
            // Start receiving result
            self.await_result(socket, addr).await?;
        }

        Ok(())
    }

    pub async fn collect_stats(&self) -> Result<(), Box<dyn Error>> {
        // Create a UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        
        let request_message = "Request: Stats";

        // Send the request to all nodes
        for &node_addr in self.nodes.values() {
            if let Err(e) = send_with_retry(&socket, request_message.as_bytes(), node_addr, 5, 5).await {
                eprintln!("Failed to send stats request to {}: {:?}", node_addr, e);
            } else {
                println!("Stats request sent to {}", node_addr);
            }
        }

        // Set up a buffer to receive responses
        let mut buffer = [0u8; 65535];

        // Loop to collect and print responses from each server
        for _ in 0..self.nodes.len() {
            let (size, addr, _packet_id) = recv_with_ack(&socket, &mut buffer, &mut HashSet::new()).await?;
            let response = String::from_utf8_lossy(&buffer[..size]);
            println!("Received response from {}: {}", addr, response);
        }

        Ok(())
    }

    async fn await_result(&self, socket: UdpSocket, sender: SocketAddr) -> Result<(), Box<dyn Error>> {
        // Receive the steganography result (image with hidden content) from the server
        // let mut output_image_data = Vec::new();
        // let mut buffer: Vec<u8> = vec![0u8; self.chunk_size];
        let (output_image_data, n, _addr) = recv_reliable(&socket).await.unwrap(); // add sender to verify its from same

        // loop {
        //     // let (n, _addr) = socket.recv_from(&mut buffer).await.unwrap();
        //     let (n, _addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await.unwrap();
        //     if _addr != sender {
        //         // ignore packets from other senders
        //         continue;
        //     }
        //     if &buffer[..n] == END_OF_TRANSMISSION.as_bytes() {
        //         println!("End of transmission signal received.");
        //         break;
        //     }

        //     output_image_data.extend_from_slice(&buffer[..n]);
        //     println!("Received chunk of size {}", n);
        // }

      // Save the received image with hidden data to a file
        let output_path = "files/received_with_hidden.png";
        if let Err(e) = File::create(output_path).await?.write_all(&output_image_data).await {
            eprintln!("Failed to write received image with hidden data to file: {}", e);
        } else {
            println!("Received image with hidden data saved as '{}'.", output_path);
        }

        // Extract the hidden image from the received image
        match server_decrypt_img(output_path, "files/extracted_hidden_image.jpg").await {
            Ok(_) => println!("Hidden image extracted and saved as 'files/extracted_hidden_image.jpg'."),
            Err(e) => eprintln!("Failed to extract hidden image: {}", e),
        }

        Ok(())
    }
}