use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::error::Error;
use tokio::fs::File;
use crate::utils::{END_OF_TRANSMISSION, DEFAULT_TIMEOUT, server_decrypt_img, send_with_retry, recv_with_timeout, recv_reliable, send_reliable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Duration;



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

    pub async fn send_data(&self, file_path: &str, service:&str) -> Result<(), Box<dyn Error>> {
        
        // Create a UDP socket for sending and receiving messages
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
        
        let request_message = format!("Request: {}", service); // The message to be sent

        // Multicast the message to all nodes
        for node_addr in self.nodes.values() {
            let node_addr = node_addr.clone(); // Clone the address
            println!("sending request to  {}", node_addr);

            // match socket.send_to(request_message.as_bytes(), &node_addr).await {
            match send_with_retry(&socket, request_message.as_bytes(), node_addr, 5).await {
                Ok(_) => {
                    println!("Request message sent to {}", node_addr);
                }
                Err(e) => {
                    // this doesnt
                    eprintln!("Failed to send message to {}: {:?}", node_addr, e);
                }
            }
        }

        // Buffer for receiving data
        let mut buffer = [0u8; 1024];

        // Now, listen for the first response that comes back from any node
        // let (size, addr) = socket.recv_from(&mut buffer).await?;
        let (size, addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await?;

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

            // start receiving result
            self.await_result(socket, addr).await;
        }
        

        Ok(())
    }


    pub async fn collect_stats(&self) -> Result<(), Box<dyn Error>> {
        // Create a UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
        
        let request_message = "Request: Stats"; // Message to send for collecting stats

        // Send the request message to all nodes
        for node_addr in self.nodes.values() {
            let node_addr = node_addr.clone(); // Clone the address
            // match socket.send_to(request_message.as_bytes(), &node_addr).await {
            match send_with_retry(&socket, request_message.as_bytes(), node_addr, 5).await {
                Ok(_) => {
                    println!("Stats request sent to {}", node_addr);
                }
                Err(e) => {
                    eprintln!("Failed to send stats request to {}: {:?}", node_addr, e);
                }
            }
        }

        // Set up a buffer to receive responses
        let mut buffer = [0u8; 65535];

        // Loop to collect and print responses from each server
        for _ in 0..self.nodes.len() {
            // let (size, addr) = socket.recv_from(&mut buffer).await?;

            let (size, addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await?;
            let response = String::from_utf8_lossy(&buffer[..size]);
            println!("Received response from {}: {}", addr, response);
        }

        Ok(())
    }


    async fn await_result(&self, socket:UdpSocket, sender: SocketAddr) {
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

        // Write the received image with hidden data to a file
        // let mut output_image_file = File::create("files/received_with_hidden.png").await.unwrap();
        // output_image_file.write_all(&output_image_data).await.unwrap();
        // println!("Received image with hidden data saved as 'files/received_with_hidden.png'.");
        match File::create("files/received_with_hidden.png").await {
            Ok(mut output_image_file) => {
                if let Err(e) = output_image_file.write_all(&output_image_data).await {
                    eprintln!("Failed to write received image with hidden data to file: {}", e);
                } else {
                    println!("Received image with hidden data saved as 'files/received_with_hidden.png'.");
                }
            }
            Err(e) => {
                eprintln!("Failed to create file for received image with hidden data: {}", e);
            }
        }


        // Extract the hidden image from the received image
        // server_decrypt_img("files/received_with_hidden.png", "files/extracted_hidden_image.jpg").await;
        // println!("Hidden image extracted and saved as 'files/extracted_hidden_image.jpg'.");
        match server_decrypt_img("files/received_with_hidden.png", "files/extracted_hidden_image.jpg").await {
            Ok(_) => {
                println!("Hidden image extracted and saved as 'files/extracted_hidden_image.jpg'.");
            }
            Err(e) => {
                eprintln!("Failed to extract hidden image: {}", e);
            }
        }
    }
}
