// implement peer class here, keeping the client purely for communication with the cloud

use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use std::sync::Arc;
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::Duration;
use crate::utils::{send_reliable, recv_reliable, DEFAULT_TIMEOUT};
use std::path::Path;
use std::fs;
use base64;


/// I think we need peer instead of client.Client to hold the individual peer info
pub struct Peer {
    pub peer_id: u16,
    pub public_socket: Arc<UdpSocket>,            // Socket for communication
    pub cloud_address: SocketAddr,               // Address of the cloud (central server or leader)
    pub server_addresses: Arc<Mutex<Vec<SocketAddr>>>, // List of known servers
    pub collections: Arc<Mutex<HashMap<String, Vec<Value>>>>, // Local data storage
}

impl Peer {
    /// Create a new peer instance
    pub async fn new(peer_id: u16, bind_addr: &str, cloud_addr: &str) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // Bind to the specified address
        let socket = Arc::new(UdpSocket::bind(bind_addr).await?);
        // Parse the cloud address
        let cloud_addr: SocketAddr = cloud_addr.parse()?;

        // Initialize the peer instance
        let peer = Arc::new(Self {
            peer_id,
            public_socket: socket,
            cloud_address: cloud_addr,
            server_addresses: Arc::new(Mutex::new(Vec::new())), // Start with an empty server list
            collections: Arc::new(Mutex::new(HashMap::new())),  // Start with an empty collection
        });

        Ok(peer)
    }

    /// Fetch a collection from a server and store it locally
    pub async fn fetch_collection_from_server(
        &self,
        server_addr: SocketAddr,
        tablename: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the request message
        let request_message = format!("Request: ReadCollection{}", tablename);
    
        // Send the request to the server using `send_with_retry`
        send_with_retry(&self.public_socket, request_message.as_bytes(), server_addr, MAX_RETRIES).await?;
        println!("Sent request: {}", request_message);
    
        // Wait for acknowledgment (e.g., "OK") using `recv_with_timeout`
        let mut buffer = [0; 1024];
        match recv_with_timeout(&self.public_socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
            Ok((len, _)) => {
                let ack_message = String::from_utf8_lossy(&buffer[..len]);
                if ack_message != "OK" {
                    eprintln!("Failed to receive acknowledgment: {}", ack_message);
                    return Err("Acknowledgment failed".into());
                }
                println!("Received acknowledgment: {}", ack_message);
            }
            Err(e) => {
                eprintln!("Error receiving acknowledgment: {}", e);
                return Err(Box::new(e));
            }
        }
    
        // Receive the JSON response using `recv_reliable`
        match recv_reliable(&self.public_socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
            Ok((data, _, _)) => {
                let response_message = String::from_utf8_lossy(&data);
                println!("Received response: {}", response_message);
    
                // Parse the JSON response
                let json_data: Vec<Value> = serde_json::from_str(&response_message)?;
                println!("Parsed collection data:\n{}", serde_json::to_string_pretty(&json_data)?);
    
                // Save the collection locally
                let mut collections = self.collections.lock().await;
                collections.insert(tablename.to_string(), json_data);
                println!("Saved collection '{}' locally.", tablename);
    
                Ok(())
            }
            Err(e) => {
                eprintln!("Error receiving JSON response: {}", e);
                Err(Box::new(e))
            }
        }
    }

    pub async fn send_image(
        socket: &UdpSocket,
        image_path: &Path,
        recipient_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Read the image from the given path
        let image_bytes = fs::read(image_path)?;
        let image_base64 = base64::encode(image_bytes);

        // Create the message
        let file_name = image_path.file_name()
            .ok_or("Invalid file name")?
            .to_string_lossy();
        let message = format!("SendImage:{}:{}", file_name, image_base64);

        // Send the message with retry logic
        send_with_retry(socket, message.as_bytes(), recipient_addr, MAX_RETRIES).await?;
        println!("Image '{}' sent to {}", file_name, recipient_addr);

        Ok(())
    }

    pub async fn get_image(
        socket: &UdpSocket,
        image_name: &str,
        peer_addr: SocketAddr,
        save_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create and send the request message
        let request_message = format!("RequestImage:{}", image_name);
        send_with_retry(socket, request_message.as_bytes(), peer_addr, MAX_RETRIES).await?;
        println!("Requested image '{}' from {}", image_name, peer_addr);
    
        // Receive the image data
        match recv_reliable(socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
            Ok((data, _, _)) => {
                let response = String::from_utf8_lossy(&data);
    
                // Parse the received response
                if !response.starts_with("Image:") {
                    return Err("Unexpected response format".into());
                }
                let parts: Vec<&str> = response.splitn(3, ':').collect();
                if parts.len() != 3 {
                    return Err("Malformed image response".into());
                }
                let received_name = parts[1];
                let image_base64 = parts[2];
    
                // Decode and save the image
                let image_bytes = base64::decode(image_base64)?;
                let save_path = save_dir.join(received_name);
                fs::write(&save_path, &image_bytes)?;
                println!("Image '{}' saved to {}", received_name, save_path.display());
    
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to receive image: {}", e);
                Err(Box::new(e))
            }
        }
    }
    
    pub async fn send_collection_to_server(
        socket: &UdpSocket,
        server_addr: SocketAddr,
        collection_name: &str,
        collection_data: &HashMap<String, Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Serialize the collection data to JSON
        let json_data = serde_json::to_string(collection_data)?;
        println!("Serialized collection '{}': {}", collection_name, json_data);
    
        // Create the request message
        let message = format!("SendCollection:{}:{}", collection_name, json_data);
    
        // Send the collection with retry logic
        send_with_retry(socket, message.as_bytes(), server_addr, MAX_RETRIES).await?;
        println!("Collection '{}' sent to server at {}", collection_name, server_addr);
    
        // Wait for acknowledgment
        match recv_reliable(socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
            Ok((ack_data, _, _)) => {
                let ack_message = String::from_utf8_lossy(&ack_data);
                if ack_message != "OK" {
                    return Err(format!("Server returned error: {}", ack_message).into());
                }
                println!("Server acknowledged collection '{}'", collection_name);
            }
            Err(e) => {
                eprintln!("Failed to receive acknowledgment: {}", e);
                return Err(Box::new(e));
            }
        }
    
        Ok(())
    }


    // /// Request image encryption
    // pub async fn request_image_encryption(
    //     self: &Arc<Self>,
    //     image_data: Vec<u8>,
    // ) -> Result<(), Box<dyn std::error::Error>> {
    //     let servers = self.server_addresses.lock().await.clone();
    //     if servers.is_empty() {
    //         println!("No servers available.");
    //         return Ok(());
    //     }

    //     let first_server = servers[0];
    //     let mut fragment_no = 0u8;
    //     let no_fragments = (image_data.len() / 65000) as u8 + 1;

    //     for chunk in image_data.chunks(65000) {
    //         let mut buffer = vec![1, fragment_no, no_fragments];
    //         buffer.extend_from_slice(chunk);
    //         self.public_socket.send_to(&buffer, first_server).await?;
    //         fragment_no += 1;
    //     }

    //     println!("Image data sent to server {:?}", first_server);

    //     // Receive processed image fragments
    //     let mut received_data = vec![];
    //     for _ in 0..no_fragments {
    //         let mut buffer = vec![0u8; 65535];
    //         let (size, _addr) = self.public_socket.recv_from(&mut buffer).await?;
    //         received_data.extend_from_slice(&buffer[2..size]); // Skip fragment metadata
    //     }

    //     println!("Received processed image data of size {}", received_data.len());
    //     Ok(())
    // }

    /// Start the peer instance to listen for incoming messages
    pub async fn run(self: &Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 65535];
        loop {
            let (size, addr) = self.public_socket.recv_from(&mut buffer).await?;
            let received_msg = String::from_utf8_lossy(&buffer[..size]);

            println!("Received message from {}: {}", addr, received_msg);

            // Handle specific message types if needed
            if received_msg.starts_with("Request:") {
                println!("Handling request: {}", received_msg);
                // Add custom handling for different request types here
            }
        }
    }
}
