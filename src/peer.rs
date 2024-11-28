// implement peer class here, keeping the client purely for communication with the cloud

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use std::path::{Path, PathBuf};

use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::fs;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncReadExt};

use serde_json::{json, Value};
use crate::utils::{recv_reliable, recv_with_timeout, send_reliable, send_with_retry, server_decrypt_img, server_encrypt_img, CHUNK_SIZE, DEFAULT_TIMEOUT, MAX_RETRIES};

use crate::client::Client;

pub struct Peer {
    public_socket: Arc<UdpSocket>,            // Socket for communication
    collections: Arc<Mutex<HashMap<String, Vec<Value>>>>, // Local data storage
    client:Client, //we should use a client object here for as the cloud communication middleware.
    
    // lists of mem operations 
    pub pending_approval: Arc<Mutex<Vec<Value>>>,
    pub inbox_queue: Arc<Mutex<Vec<Value>>>,
    pub available_resources: Arc<Mutex<Vec<Value>>>,
}


// Functions to be implemented in peer:

/// done:
// publish_info() : checks contents of resources folder, publishes a document of my own address and the list of resources (filenames) + maybe some file metadata to the cloud.
// fetch_catalog() : fetches the 'catalog' collection from the cloud, returns the json.
// request_resource(peer_addr, resource_name, num_views) : request resource from peer for a certain number of views.
// grant_resource(peer_addr, resource_name, num_views) : grants and sends the resource to the other peer.
// change_permission(ip, img_name) // update directory of service with new permission

// TODO
// access_resource(resource_name, provider_addr) // if provider_addr can be this peer's address for local resources
            // it checks the directory of service first for this resource
            // it decrypts the image, and either returns the raw image data to be supplied to a viewer or pops up the viewer
            // updates the directory of service after viewing
            // if remaining views == 0, delete entry from directory of service. like in grant resource

impl Peer {

    /// Create a new peer instance
    pub async fn new(address: SocketAddr, cloud_nodes: Option<HashMap<String, SocketAddr>>) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // Bind to the specified address
        let socket = Arc::new(UdpSocket::bind(address).await?);
        
        // Initialize the client for cloud interaction
        let mut client = Client::new(cloud_nodes, None);

        // Initialize the peer instance
        let peer = Arc::new(Peer {
            public_socket: socket,
            collections: Arc::new(Mutex::new(HashMap::new())),  // Start with an empty collection
            // client:Arc::new(client),
            client,

            pending_approval: Arc::new(Mutex::new(Vec::new())),
            inbox_queue: Arc::new(Mutex::new(Vec::new())),
            available_resources: Arc::new(Mutex::new(Vec::new())),
        });

        Ok(peer)
    }
    /// Registers a server node with the client
    pub fn register_node(&mut self, name: String, address: SocketAddr) {
        self.client.register_node(name, address);
    }


    pub async fn start(self: &Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        self.publish_info().await?;

        // TODO write a function to fetch from directory of service 'permissions' collection to update 'inbox_queue' and 'available_resources' (if some requests were missed (leava a note concept))
        //  consider using the filter in ReadCollection, like in line 393 but with ReadCollection to get only 'grant' or only 'request' documents
        // call it here

        // Init socket
        println!("Peer available on {:?}", self.public_socket.local_addr());

        // init server thread
        let serve_self = self.clone();
        tokio::spawn(async move {
            loop {
                println!("looping1");
                let mut buffer: Vec<u8> = vec![0u8; 65535]; // Buffer to hold incoming UDP packets
                let (size, addr) = match recv_with_timeout(&serve_self.public_socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                    Ok((size, addr)) => (size, addr), // Successfully received data
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        continue; // Early exit or handle the error in some other way
                    },
                    Err(e) => {
                        eprintln!("Failed to receive data: {:?}", e);
                        continue; // Early exit or handle the error in some other way
                    }
                };
                // Clone buffer data to process it in a separate task
                let packet = buffer[..size].to_vec();
                let received_msg: String = String::from_utf8_lossy(&packet).into_owned();

                let json_obj: Value = match serde_json::from_slice(&packet) {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("Failed to parse JSON msg: {:?} {:?}", received_msg, e);
                        continue // Handle the error appropriately, e.g., skip processing this data
                    }
                };

                if json_obj["type"] == "request" {
                    // Add to inbox list
                    let mut data = json_obj.clone();
                    data["requester"] = Value::String(addr.to_string());
                    let mut inbox = serve_self.inbox_queue.lock().await;
                    inbox.push(json_obj);
                }
                else if json_obj["type"] == "grant" {
                    let mut pending = serve_self.pending_approval.lock().await;

                    // Collect the items that match the condition into a separate vector
                    let r_name = json_obj["resource"].clone();
                    let matched_items: Vec<Value> = pending.iter()
                        .filter(|item| {
                            addr.to_string().as_str() == item["provider"].as_str().unwrap_or("") 
                            && item["resource"] == r_name
                        })
                        .cloned()
                        .collect();

                    // If `matched_items` is empty, no items were filtered out
                    if matched_items.is_empty() {
                        // did not request this resource, ignore it
                        continue;
                    }
                    if json_obj["num_views"].as_u64().unwrap() > 0 { // if 0, then resource grant is denied
                        // TODO complete receiving img, based on flow of grant_resource
                        // send Ok

                        // do recev_reliable to regieve img data, and save it in resources/borrowed as 'og_filename.encrp' to have uniforme extention 
                        
                        // Update local resources list
                        let mut resources = serve_self.available_resources.lock().await;
                        resources.push(json_obj);
                    }
                    // Pop from pending
                    pending.retain(|item| {
                        !(addr.to_string().as_str() == item["provider"].as_str().unwrap_or("") && item["resource"].as_str() == r_name.as_str())
                    });
                }
            }
        });
        Ok(())
    }

    pub async fn encrypt_img(&self, file_name:&str, num_views:u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file_path: PathBuf = Path::new("resources/owned").join(file_name);

        // Read the file data to be sent
        let mut file = File::open(file_path).await.unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).await.unwrap();

        // Encode access info
        let encoded = format!("{:?}.{}", self.public_socket.local_addr(), num_views);
        // Pad to the maximum length, accomodating for possibly ipv6 addresses
        let padded = format!("{:<width$}", encoded, width=62);
        // Add padded to data at the end
        data.extend_from_slice(padded.as_bytes());

        let result = self.client.send_data(data, "Encrypt").await;
        
        match result {
            Ok(data) => {
                // Step 2: Write to file
                println!("Image Encrypted successfully: {}", file_name);
                return Ok(data);
            }
            Err(e) => {
                println!("Image Encryption failed {:?}", e);
                return Err(Box::new(e));
            }
        }
        

    }
    
    // fetch_catalog() : fetches the 'catalog' collection from the cloud, returns the json.
    /// Fetch a collection from a server and store it locally
    pub async fn fetch_catalog(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        // Define the service name and any parameters if needed
        let service_name = "ReadCollection";
        let params = vec!["catalog"]; // In case you want to specify table name as a parameter
    
        // Use `send_data_with_params` to send the request and receive the response
        match self.client.send_data_with_params(Vec::new(), service_name, params).await {
            Ok(response_data) => {
                let response_message = String::from_utf8_lossy(&response_data);
                println!("Received response: {}", response_message);
    
                // Parse the JSON response
                let json_data: Vec<Value> = serde_json::from_str(&response_message)?;
                println!("Parsed collection data:\n{}", serde_json::to_string_pretty(&json_data)?);
    
                // Save the collection locally
                let mut collections = self.collections.lock().await;
                collections.insert("catalog".to_string(), json_data.clone());
                println!("Saved collection 'catalog' locally.");
    
                Ok(json_data)
            }
            Err(e) => {
                eprintln!("Error during fetch_catalog: {}", e);
                Err(Box::new(e))
            }
        }
    }
 

    // publish_info() : checks contents of resources folder, publishes a document of my own address and the list of resources (filenames) + maybe some file metadata to the cloud.
    pub async fn publish_info(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Define the folder to scan
        let folder_path = "resources/owned"; // Add folder path
        let peer_addr = self.public_socket.local_addr();
    
        // Read the folder contents asynchronously
        let mut resources_info = Vec::new();
        let mut entries = fs::read_dir(folder_path).await?;
    
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name().into_string().unwrap_or_default();
    
            // Collect metadata
            if let Ok(metadata) = entry.metadata().await {
                let file_size = metadata.len();
    
                resources_info.push(json!({
                    "filename": file_name,
                    "size": file_size,
                    "modified_time": format!("{:?}", metadata.modified()),
                }));
            }
        }
    
        // Construct the JSON payload
        let payload = json!({
            "UUID": format!("{:?}", peer_addr), // used for updating prev entry
            "peer_addr": format!("{:?}", peer_addr),
            "resources": resources_info,
        });
    
        // Serialize the JSON payload
        let payload_str = serde_json::to_string(&payload)?;
        println!("Prepared payload: {}", payload_str);
    
        let params = vec!["catalog"];
        match self.client.send_data_with_params(payload_str.as_bytes().to_vec(), "UpdateDocument", params.clone())
        .await{
            Ok(response_data) => {
                let response_message = String::from_utf8_lossy(&response_data);
                if response_message != "OK" {
                    return Err(format!("Server returned error: {}", response_message).into());
                }
                println!("Server acknowledged publish_info request.");
                Ok(())
            }
            Err(e) => {
                eprintln!("Error during publish_info: {}", e);
                Err(Box::new(e))
            }
        }
    }
    
    // request_resource(peer_addr, resource_name, num_views) : request resource from peer for a certain number of views.
    pub async fn request_resource(
        &self,
        peer_addr: SocketAddr,
        resource_name: &str,
        num_views: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the request message in JSON format
        let request_message = serde_json::json!({
            "type": "request",
            "user" : format!("{:?}", self.public_socket.local_addr()),
            "provider" : format!("{:?}", peer_addr),
            "resource_name": resource_name,
            "num_views": num_views,
            "UUID": format!("req:{:?}|{:?}|{}", peer_addr, self.public_socket.local_addr(), resource_name), // provider, requester, resource name as an ID for the 'permissions' entries
        }).to_string();

        let params = vec!["permissions"];
        let _ =  self.client.send_data_with_params(request_message.as_bytes().to_vec(), "UpdateDocument", params.clone()).await.unwrap();
    
        // Send the request to the peer
        send_with_retry(
            &self.public_socket,
            request_message.to_string().as_bytes(),
            peer_addr,
            MAX_RETRIES,
        )
        .await?;
        println!(
            "Requested resource '{}' with {} views from peer at {}",
            resource_name, num_views, peer_addr
        );
    
        Ok(())
    }
    

    /// grant_resource(peer_addr, resource_name, num_views) : grants and sends the resource to the other peer.
    pub async fn grant_resource(
        self: &Arc<Self>,
        peer_addr: SocketAddr,
        resource_name: &str,
        num_views: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        
        let resource_path = format!("./resources/{}", resource_name);
    
        // Check if the resource exists
        if !tokio::fs::metadata(&resource_path).await.is_ok() {
            eprintln!("Resource '{}' not found in the 'resources' folder.", resource_name);
            return Err("Resource not found".into());
        }
        let myself = self.clone();
        let resource_n = resource_name.to_string();

        tokio::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();

            // update directory of service with this permission grant
            let entry = json!({
                "type": "grant",
                "resource": resource_n,
                "provider": format!("{:?}",myself.public_socket.local_addr()),
                "user": format!("{:?}", peer_addr),
                "num_views": num_views,
                "remaining": num_views,
                "UUID": format!("grant:{:?}|{:?}|{}", myself.public_socket.local_addr(), peer_addr, resource_n), // provider, requester, resource name as an ID for the 'permissions' entries
            }).to_string();
            let params = vec!["permissions"];
            let _ =  myself.client.send_data_with_params(entry.as_bytes().to_vec(), "UpdateDocument", params.clone()).await.unwrap();

            // send grant message back to peer to exchange data 
            let _ = match send_with_retry(&socket, entry.as_bytes(), peer_addr, MAX_RETRIES).await {
                Ok(()) => (), // Successfully received data
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // unreachable peer
                },
                Err(e) => {
                    eprintln!("Failed to send to peer: {:?}", e);
                }
                
            };

            // await OK
            let mut buffer = [0u8; 1024];
            // Now, listen for the first response that comes back from any node
            let (size, addr) = match recv_with_timeout(&socket, &mut buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                Ok ((size, addr)) => (size, addr),
                Err(_) => {
                    return;
                }
            };

            let response = String::from_utf8_lossy(&buffer[..size]);
            if response != "OK" {
                return;
            }

            let encrypted_data =  match myself.encrypt_img(&resource_path, num_views).await {
                Ok(encrypted_data) => encrypted_data,
                Err(_) => {
                    return;
                }
            };
        
            // Send the encrypted image to the peer
            send_reliable(&socket, &encrypted_data, addr).await.expect("Failed to send resource to peer");
            println!(
                "- Granted resource '{}' with {} views to peer at {}",
                resource_n, num_views, peer_addr
            );

            // Only if everything is successful:
            // delete original request from DOS directory of services
            let filter = json!({
                "UUID": format!("req:{:?}|{:?}|{}", myself.public_socket.local_addr(), peer_addr, resource_n), // provider, requester, resource name as an ID for the 'permissions' entries
            }).to_string();
            let _ =  myself.client.send_data_with_params(filter.as_bytes().to_vec(), "DeleteDocument", params.clone()).await.unwrap();
            
            // pop from local inbox, based on user and resource_name
            let mut inbox = myself.inbox_queue.lock().await;
            inbox.retain(|item| {
                !(addr.to_string().as_str() == item["user"].as_str().unwrap_or("") && item["resource"].as_str() == Some(&resource_n))
            });
        });
    
        Ok(())
    }
    

    // receive_resource(encrypted_img_path, output_dir) : receives an encrypted image from a peer and extract the hidden resource
    // pub async fn receive_resource(
    //     &self,
    //     encrypted_img_path: &str, // Path to save the received encrypted image
    //     output_dir: &str,         // Directory to store decrypted resources
    // ) -> Result<(), Box<dyn std::error::Error>> {
    //     // Receive the encrypted image as bytes
    //     let mut buffer = [0u8; CHUNK_SIZE];
    //     let (received_len, sender_addr) = recv_with_timeout(
    //         &self.public_socket,
    //         &mut buffer,
    //         Duration::from_secs(DEFAULT_TIMEOUT),
    //     )
    //     .await?;
    //     let encrypted_data = &buffer[..received_len];
    //     println!("Received encrypted resource from {}.", sender_addr);
    
    //     // Save the encrypted image to the specified path
    //     let mut encrypted_file = tokio::fs::File::create(encrypted_img_path).await?;
    //     encrypted_file.write_all(encrypted_data).await?;
    //     println!("Saved encrypted image to '{}'.", encrypted_img_path);
    
    //     // Decrypt the image to extract the hidden data
    //     let extracted_data_path = format!("{}/extracted_data.txt", output_dir); // Temporary storage for extracted data
    //     server_decrypt_img(encrypted_img_path, &extracted_data_path).await?;
    
    //     // Read and parse the extracted data
    //     let mut extracted_file = tokio::fs::File::open(&extracted_data_path).await?;
    //     let mut extracted_content = String::new();
    //     extracted_file.read_to_string(&mut extracted_content).await?;
    
    //     // Split the extracted data into metadata and resource content
    //     let parts: Vec<&str> = extracted_content.splitn(2, '|').collect();
    //     if parts.len() != 2 {
    //         eprintln!("Malformed extracted data: {}", extracted_content);
    //         return Err("Malformed extracted data".into());
    //     }
    //     let metadata_str = parts[0];
    //     let resource_content_base64 = parts[1];
    
    //     // Parse metadata
    //     let metadata: serde_json::Value = serde_json::from_str(metadata_str)?;
    //     let provider = metadata["provider"].as_str()
    //         .ok_or("Invalid 'resource_name' value")?;
    //     let num_views = metadata["num_views"].as_u64().ok_or("Invalid 'num_views' value")?;
    //     let resource_name = metadata["resource_name"]
    //         .as_str()
    //         .ok_or("Invalid 'resource_name' value")?;
    //     println!(
    //         "Extracted metadata - Resource: '{}', Allowed Views: {}",
    //         resource_name, num_views
    //     );
    
    //     // Decode the resource content from Base64
    //     let resource_content = base64::decode(resource_content_base64)?;
    
    //     // Save the resource content to a file
    //     let resource_path = format!("{}/{}", output_dir, resource_name);
    //     let mut resource_file = tokio::fs::File::create(&resource_path).await?;
    //     resource_file.write_all(&resource_content).await?;
    //     println!("Saved resource '{}' to '{}'.", resource_name, resource_path);
    
    //     Ok(())
    // }
    

}
