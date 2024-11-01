// use tokio::net::UdpSocket;
// use tokio::sync::Mutex;
// use tokio::fs::File;
// use tokio::io::{AsyncReadExt, AsyncWriteExt};

// use std::collections::HashMap;
// use std::sync::Arc;
// use std::net::SocketAddr;
// use std::io::Result;
// use crate::utils::{END_OF_TRANSMISSION, server_encrypt_img};
// use rand::random;

// #[derive(Clone)]
// pub struct NodeInfo {
//     pub mem: u8,
//     pub id: u8,
//     pub addr: SocketAddr,
// }

// pub struct CloudNode {
//     nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  // Keeps track of other cloud nodes
//     public_socket: Arc<UdpSocket>,
//     chunk_size: usize,
//     elected: Mutex<bool>,   // Elected state wrapped in Mutex for safe mutable access
//     mem: u8,
//     id: u8,
// }

// impl CloudNode {
//     /// Creates a new CloudNode
//     pub async fn new(
//         address: SocketAddr,
//         nodes: Option<HashMap<String, NodeInfo>>,
//         chunk_size: usize,
//         elected: bool,
//         mem: u8,
//         id: u8,
//     ) -> Result<Arc<Self>> {
//         let initial_nodes = nodes.unwrap_or_else(HashMap::new);
//         let socket = UdpSocket::bind(address).await?;

//         Ok(Arc::new(CloudNode {
//             nodes: Arc::new(Mutex::new(initial_nodes)),
//             public_socket: Arc::new(socket),
//             chunk_size,
//             elected: Mutex::new(elected),
//             mem,
//             id,
//         }))
//     }

//     /// Starts the server, listens for new connections, and processes data
//     pub async fn serve(self: Arc<Self>) -> Result<()> {
//         {
//             let mut elected = self.elected.lock().await;
//             *elected = false;  // Reset election state initially

//             self.get_info().await;    // Retrieve updated info from all nodes
//             self.elect_leader().await; // Elect a leader based on mem and id values
//         }

//         let mut buffer = vec![0u8; 65535]; // Buffer to hold incoming UDP packets

//         loop {
//             let (size, addr) = self.public_socket.recv_from(&mut buffer).await?;

//             // Clone buffer data to process it in a separate task
//             let packet = buffer[..size].to_vec();
//             let node = self.clone();

//             tokio::spawn(async move {
//                 let elected = *node.elected.lock().await;
//                 if elected {
//                     if let Err(e) = node.handle_connection(packet, size, addr).await {
//                         eprintln!("Error handling connection: {:?}", e);
//                     }
//                 }
//             });
//         }
//     }

//     /// Retrieves updated information from all nodes
//     async fn get_info(&self) {
//         let mut nodes = self.nodes.lock().await;
//         for (_, node_info) in nodes.iter_mut() {
//             node_info.mem = random::<u8>();  // Simulate updated mem
//             println!("Updated info for node {}: mem = {}", node_info.id, node_info.mem);
//         }
//     }

//     /// Elects the leader node based on the lowest mem value, breaking ties with the lowest id
//     async fn elect_leader(&self) {
//         let nodes = self.nodes.lock().await;
//         let mut lowest_mem = self.mem;
//         let mut elected_node = self.id;

//         for (_, node_info) in nodes.iter() {
//             if node_info.mem < lowest_mem || (node_info.mem == lowest_mem && node_info.id < elected_node) {
//                 lowest_mem = node_info.mem;
//                 elected_node = node_info.id;
//             }
//         }

//         let mut elected = self.elected.lock().await;
//         *elected = self.id == elected_node;
//         println!("Node {} is elected as leader: {}", self.id, *elected);
//     }

//     /// Handle an incoming connection, aggregate the data, process it, and send a response
//     async fn handle_connection(self: Arc<Self>, data: Vec<u8>, size: usize, addr: SocketAddr) -> Result<()> {
//         // Convert incoming bytes to a string to parse JSON
//         let received_msg: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&data[..size]);

//         println!("Got request from {}: {}", addr, received_msg);

//         if received_msg == "Request: Encrypt" {
//             println!("Processing request from client: {}", addr);

//             // Establish a connection to the client for sending responses
//             let socket = UdpSocket::bind("0.0.0.0:0").await?; // Bind to an available random port
//             println!("Established connection with client on {}", socket.local_addr()?);

//             // Send "OK" message to the client to indicate we are ready to receive data
//             socket.send_to(b"OK", &addr).await?;
//             println!("Sent 'OK' message to {}", addr);

//             // Buffer to receive chunks of data
//             let mut buffer = [0u8; 1024];
//             let mut aggregated_data = Vec::new(); // Aggregate the incoming data

//             // Receive data in chunks from the client
//             loop {
//                 let (size, _) = socket.recv_from(&mut buffer).await?;
//                 let received_data = String::from_utf8_lossy(&buffer[..size]);

//                 // Check for the end of transmission message
//                 if received_data == END_OF_TRANSMISSION {
//                     println!("End of transmission from client {}", addr);
//                     break;
//                 }
//                 // Append the received chunk to the aggregated_data buffer
//                 aggregated_data.extend_from_slice(&buffer[..size]);
//                 println!("Received chunk from {}: {} bytes", addr, size);
//             }

//             // Process the aggregated data
//             let processed_data = self.process(aggregated_data).await;

//             // Send the processed data back to the client in chunks
//             let chunk_size = 1024; // Define chunk size for sending the response
//             for chunk in processed_data.chunks(chunk_size) {
//                 socket.send_to(chunk, &addr).await?;
//                 println!("Sent chunk of {} bytes back to {}", chunk.len(), addr);
//             }
//             socket.send_to(END_OF_TRANSMISSION.as_bytes(), &addr).await?;
//             println!("Task for client done: {}", addr);
//         }

//         Ok(())
//     }

//     /// Retrieves the registered server nodes
//     pub fn get_nodes(&self) -> HashMap<String, NodeInfo> {
//         let copy = self.nodes.blocking_lock().clone();
//         return copy;
//     }

//     async fn process(&self, data: Vec<u8>) -> Vec<u8> {
//         let img_path = "files/to_encrypt.jpg";
//         let output_path = "files/encrypted_output.png";
    
//         // Step 1: Write data bytes to a file (e.g., 'to_encrypt.png')
//         let mut file = File::create(img_path).await.unwrap();
//         file.write_all(&data).await.unwrap();
    
//         // Step 2: Call `server_encrypt_img` to perform encryption on the file
//         server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;
    
//         // Step 3: Read the encrypted output file as bytes
//         let mut encrypted_file = File::open(output_path).await.unwrap();
//         let mut encrypted_data = Vec::new();
//         encrypted_file.read_to_end(&mut encrypted_data).await.unwrap();
    
//         // Return the encrypted data as the output
//         return encrypted_data
//     }
// }


use tokio::net::{UdpSocket, TcpStream, TcpListener};
use tokio::sync::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::Result;
use crate::utils::{END_OF_TRANSMISSION, server_encrypt_img};
use rand::random;

#[derive(Clone)]
pub struct NodeInfo {
    pub mem: u8,
    pub id: u8,
    pub addr: SocketAddr,
}

pub struct CloudNode {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,  // Keeps track of other cloud nodes
    public_socket: Arc<UdpSocket>,
    chunk_size: usize,
    elected: Mutex<bool>,   // Elected state wrapped in Mutex for safe mutable access
    mem: u8,
    id: u8,
}

impl CloudNode {
    /// Creates a new CloudNode
    pub async fn new(
        address: SocketAddr,
        nodes: Option<HashMap<String, NodeInfo>>,
        chunk_size: usize,
        elected: bool,
        mem: u8,
        id: u8,
    ) -> Result<Arc<Self>> {
        let initial_nodes = nodes.unwrap_or_else(HashMap::new);
        let socket = UdpSocket::bind(address).await?;

        Ok(Arc::new(CloudNode {
            nodes: Arc::new(Mutex::new(initial_nodes)),
            public_socket: Arc::new(socket),
            chunk_size,
            elected: Mutex::new(elected),
            mem,
            id,
        }))
    }

    /// Starts the server to listen for incoming requests, elect a leader, and process data
    pub async fn serve(self: Arc<Self>) -> Result<()> {
        // Set up the TCP listener to listen for incoming TCP connections
        let listener = TcpListener::bind(self.public_socket.local_addr()?).await?;
        println!("Listening for incoming info requests on {:?}", listener.local_addr());

        // Spawn a task to handle incoming TCP connections separately
        let node = self.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((socket, addr)) = listener.accept().await {
                    let node_clone = node.clone();
                    tokio::spawn(async move {
                        if let Err(e) = node_clone.handle_info_request(socket, addr).await {
                            eprintln!("Failed to handle info request from {}: {:?}", addr, e);
                        }
                    });
                }
            }
        });

        // Initial leader election and info retrieval
        {
            let mut elected = self.elected.lock().await;
            *elected = false; // Reset election state initially
            self.get_info().await; // Retrieve updated info from all nodes
            self.elect_leader().await; // Elect a leader based on mem and id values
        }

        // UDP processing loop (from the previous code) would go here...

        Ok(())
    }

    /// Retrieves updated information from all nodes using TCP messages
    async fn get_info(&self) {
        let node_addresses: Vec<(String, SocketAddr)> = {
            let nodes = self.nodes.lock().await;
            nodes.iter().map(|(id, info)| (id.clone(), info.addr)).collect()
        };

        for (node_id, addr) in node_addresses {
            if let Ok(mut stream) = TcpStream::connect(addr).await {
                if stream.write_all(b"Request: Update").await.is_ok() {
                    let mut buffer = [0u8; 1024];
                    if let Ok(size) = stream.read(&mut buffer).await {
                        let updated_mem = buffer[0];

                        let mut nodes = self.nodes.lock().await;
                        if let Some(node_info) = nodes.get_mut(&node_id) {
                            node_info.mem = updated_mem;
                            println!("Updated info for node {}: mem = {}", node_info.id, node_info.mem);
                        }
                    }
                }
            }
        }
        // let nodes = self.nodes.lock().await;

        // for (_, node_info) in nodes.iter() {
        //     let addr = node_info.addr;

        //     // Try to connect to the node via TCP
        //     if let Ok(mut stream) = TcpStream::connect(addr).await {
        //         println!("Connected to node {}: {}", node_info.id, addr);

        //         // Send a request message
        //         if let Err(e) = stream.write_all(b"Request: UpdateInfo").await {
        //             eprintln!("Failed to send update request to {}: {:?}", addr, e);
        //             continue;
        //         }

        //         // Buffer to hold the response from the node
        //         let mut response = [0u8; 1024];
        //         let size = match stream.read(&mut response).await {
        //             Ok(size) => size,
        //             Err(e) => {
        //                 eprintln!("Failed to read response from {}: {:?}", addr, e);
        //                 continue;
        //             }
        //         };

        //         // Process the response (assuming the node sends updated mem value as plain u8)
        //         let updated_mem = response[0];
        //         println!("Received updated mem from node {}: {}", node_info.id, updated_mem);

        //         // Update the node's mem value
        //         if let Some(node_info) = nodes.get_mut(&node_info.id.to_string()) {
        //             node_info.mem = updated_mem;
        //         }
        //     } else {
        //         eprintln!("Failed to connect to node {}: {}", node_info.id, addr);
        //     }
        // }
    }

    /// Handles incoming TCP requests for node information
    async fn handle_info_request(&self, mut socket: TcpStream, addr: SocketAddr) -> Result<()> {
        println!("Received info request from {}", addr);

        // Read the request message
        let mut buffer = [0u8; 1024];
        let size = socket.read(&mut buffer).await?;
        let request_msg = String::from_utf8_lossy(&buffer[..size]);

        if request_msg == "Request: UpdateInfo" {
            // Serialize the current node info (mem and id) to respond
            let response = format!("{} {}", self.mem, self.id);

            // Send the response back to the requesting node
            socket.write_all(response.as_bytes()).await?;
            println!("Sent info response to {}: {}", addr, response);
        }

        Ok(())
    }

    /// Elects the leader node based on the lowest mem value, breaking ties with the lowest id
    async fn elect_leader(&self) {
        let nodes = self.nodes.lock().await;
        let mut lowest_mem = self.mem;
        let mut elected_node = self.id;

        for (_, node_info) in nodes.iter() {
            if node_info.mem < lowest_mem || (node_info.mem == lowest_mem && node_info.id < elected_node) {
                lowest_mem = node_info.mem;
                elected_node = node_info.id;
            }
        }

        let mut elected = self.elected.lock().await;
        *elected = self.id == elected_node;
        println!("Node {} is elected as leader: {} with mem={}", self.id, *elected, self.mem);
    }

    /// Handle an incoming connection, aggregate the data, process it, and send a response
    async fn handle_connection(self: Arc<Self>, data: Vec<u8>, size: usize, addr: SocketAddr) -> Result<()> {
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

            // Process the aggregated data
            let processed_data = self.process(aggregated_data).await;

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
    pub fn get_nodes(&self) -> HashMap<String, NodeInfo> {
        let copy = self.nodes.blocking_lock().clone();
        return copy;
    }

    async fn process(&self, data: Vec<u8>) -> Vec<u8> {
        let img_path = "files/to_encrypt.jpg";
        let output_path = "files/encrypted_output.png";
    
        // Step 1: Write data bytes to a file (e.g., 'to_encrypt.png')
        let mut file = File::create(img_path).await.unwrap();
        file.write_all(&data).await.unwrap();
    
        // Step 2: Call `server_encrypt_img` to perform encryption on the file
        server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;
    
        // Step 3: Read the encrypted output file as bytes
        let mut encrypted_file = File::open(output_path).await.unwrap();
        let mut encrypted_data = Vec::new();
        encrypted_file.read_to_end(&mut encrypted_data).await.unwrap();
    
        encrypted_data
    }
}
