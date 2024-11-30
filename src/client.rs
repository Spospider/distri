use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::error::Error;
use crate::utils::{
    DEFAULT_TIMEOUT, 
    send_with_retry, 
    recv_with_timeout, 
    recv_reliable, 
    send_reliable
};
use tokio::time::Duration;



pub struct Client {
    nodes: HashMap<String, SocketAddr>,  // Keeps track of server nodes
    // chunk_size: usize,
}

impl Default for Client {
    fn default() -> Self {
        Client {
            nodes: HashMap::new(),
            // chunk_size: 1024,
        }
    }
}

impl Client {
    /// Creates a new client instance
    pub fn new(nodes: Option<HashMap<String, SocketAddr>>, _chunk_size: Option<usize>) -> Self {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        // let final_chunk_size: usize = chunk_size.unwrap_or(1024);

        Client {
            nodes: initial_nodes,
            // chunk_size: final_chunk_size,
        }
    }

    /// Registers a server node with its name and address
    pub fn register_node(&mut self, name: String, address: SocketAddr) {
        self.nodes.insert(name, address);
    }

    pub async fn send_data(&self, data: Vec<u8>, service:&str) -> Result<Vec<u8>, std::io::Error> {
        // Create a UDP socket for sending and receiving messages
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
        let request_message = format!("Request: {}", service); // The message to be sent

        // Multicast the message to all nodes
        for node_addr in self.nodes.values() {
            let node_addr = node_addr.clone(); // Clone the address

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
        let (size, addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await?;

        let response = String::from_utf8_lossy(&buffer[..size]);
        if !data.is_empty(){
            if response == "OK" {
                println!("request for service accepted from {}", addr);

                send_reliable(&socket, &data, addr).await?;

                // start receiving result
                let result = self.await_result(socket, addr).await;
                return Ok(result?)
            }
            
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Send request not accepted."))
        } else {
            // return response directy
            return Ok(buffer[..size].to_vec());
        }
    }

    pub async fn send_data_with_params(
        &self, 
        data: Vec<u8>, 
        service: &str, 
        params: Vec<&str>
    ) -> Result<Vec<u8>, std::io::Error> {
        // Create a UDP socket for sending and receiving messages
        let socket = UdpSocket::bind("0.0.0.0:0").await?;  // Bind to any available port
    
        // Build the parameterized request message
        let param_string = params.join(","); // Combine parameters into a comma-separated string
        let request_message = format!("Request: {}<{}>", service, param_string); // Format request message
    
        // Multicast the message to all nodes
        for node_addr in self.nodes.values() {
            let node_addr = node_addr.clone(); // Clone the address
            // println!("Sending request to {}", node_addr);
    
            // Send the request message with retries
            match send_with_retry(&socket, request_message.as_bytes(), node_addr, 5).await {
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
    
        // Listen for the first response from any node
        let (size, addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await?;
    
        let response = String::from_utf8_lossy(&buffer[..size]);
        
        // if there is data to send, expect Ok message first
        if response == "OK" {
            println!("Request for service accepted from {}", addr);
        
            if !data.is_empty(){
                // Send the file data reliably
                send_reliable(&socket, &data, addr).await?;
            }
    
            // Start receiving the result
            let result = self.await_result(socket, addr).await;
            return Ok(result?);
        }
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Send request not accepted."))    
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
                    continue;
                }
            }
            let mut buffer = [0u8; 65535];
            // Await response
            let _ = match recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                Ok((size, addr)) => {
                    let response = String::from_utf8_lossy(&buffer[..size]);
                    println!("Received response from {}: {}", addr, response);
                }
                Err(e) => {
                    eprintln!("No Stats response from {}: {:?}", node_addr, e);
                    continue;
                }
            };
            
        }

        // Set up a buffer to receive responses

        // Loop to collect and print responses from each server
        // for _ in 0..self.nodes.len() {
        //     // let (size, addr) = socket.recv_from(&mut buffer).await?;

        //     let (size, addr) = recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await?;
        //     let response = String::from_utf8_lossy(&buffer[..size]);
        //     println!("Received response from {}: {}", addr, response);
        // }

        Ok(())
    }


    async fn await_result(&self, socket:UdpSocket, sender: SocketAddr) -> Result<Vec<u8>, std::io::Error>  {
        // println!("receiving on {:?}", socket.local_addr().unwrap());
        let (data, _, addr) = recv_reliable(&socket, None).await.unwrap(); // Adjust for proper error handling if necessary.
        if addr != sender {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Received data from unexpected sender",
            ));
        }
        Ok(data)
    }
    
}
