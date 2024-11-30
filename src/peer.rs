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

use serde_json::{to_vec, Value, json};
use crate::utils::{recv_reliable, recv_with_timeout, send_reliable, send_with_retry, server_decrypt_img, peer_decrypt_img, server_encrypt_img, CHUNK_SIZE, DEFAULT_TIMEOUT, MAX_RETRIES};

use crate::client::Client;

pub struct Peer {
    id: Arc<String>,
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
    pub async fn new(id:&str, address: SocketAddr, cloud_nodes: Option<HashMap<String, SocketAddr>>) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // Bind to the specified address
        let socket = Arc::new(UdpSocket::bind(address).await?);
        
        // Initialize the client for cloud interaction
        let client = Client::new(cloud_nodes, None);

        // Initialize the peer instance
        let peer = Arc::new(Peer {
            id:Arc::new(id.to_string()),
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
        // Publish my catalog
        self.publish_info().await?;
        
        // Get up to date with the cloud
        let filter = json!({
            "type" : "request",
            "provider" : *self.id.clone(),
        });
        let cloud_transactions =  self.fetch_collection("permissions", Some(filter)).await.expect("Failed to fetch from 'permissions' collection");
        // Update `inbox_queue`
        let mut inbox: tokio::sync::MutexGuard<'_, Vec<Value>> = self.inbox_queue.lock().await;
        for item in &cloud_transactions {
            // check if types is request, and i am the provider
            // if item["type"].as_str() == Some("request") && item["provider"].as_str() == Some(format!("{:?}",self.public_socket.local_addr()).as_str()) {
                if let Some(requester) = item["requester"].as_str() {
                    let mut data = item.clone();
                    data["requester"] = Value::String(requester.to_string());
                    inbox.push(data);
                }
            // }
        }

        // Get up to date with the cloud
        let filter = json!({
            "type" : "grant",
            "user" : *self.id.clone(),
        });
        let cloud_transactions =  self.fetch_collection("permissions", Some(filter)).await.expect("Failed to fetch from 'permissions' collection");
        
        // Check missed grants, and resend request for them
        for item in &cloud_transactions {
            // if its a grant transaction and i am the user
            // if item["type"].as_str() == Some("grant") && item["user"].as_str() == Some(format!("{:?}",self.public_socket.local_addr()).as_str()) {
                if let Some(peer_id) = item["provider"].as_str() {
                    if let Some(resource_name) = item["resource_name"].as_str() {
                        if let Some(num_views) = item["num_views"].as_u64() {
                            // request it from peer again
                            self.request_resource(peer_id, resource_name, num_views as u32).await.unwrap_or_default();
                            // peer should then send a grant resource
                        }
                    }
                }
            // }
        }


        // Init socket
        println!("Peer available on {:?}", self.public_socket.local_addr().unwrap());

        // init server thread
        let serve_self = self.clone();
        tokio::spawn(async move {
            loop {
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
                    serve_self.handle_request_msg(addr, json_obj).await;
                }
                else if json_obj["type"] == "grant" {
                    serve_self.handle_grant_msg(addr, json_obj).await;
                }
            }
        });
        Ok(())
    }
    async fn handle_request_msg(self: &Arc<Self>, addr:SocketAddr, json_obj:Value) {
        // Add to inbox list
        let data = json_obj.clone();
        let resource_name = data["resource_name"].as_str().unwrap_or("NULL");
        // check if request has been granted before from DOS, and grant automatically if so.
        let peer_id = json_obj["user"].as_str().expect("failed to parse userid");
        let filter = json!({
            "UUID": format!("grant:{:?}|{:?}|{}", self.id, peer_id, resource_name),
        });
        let cloud_transactions =  self.fetch_collection("permissions", Some(filter)).await.expect("Failed to fetch 'permissions' collection");
        for item in &cloud_transactions {
            // check if types is request, and i am the provider
            if item["num_views"].as_u64().unwrap_or(0) > 0  && item["remaining"].as_u64().unwrap_or(0) < item["num_views"].as_u64().unwrap_or(0) {
                // send grant msg automatically
                let num_views = (item["num_views"].as_u64().unwrap_or(0) - item["remaining"].as_u64().unwrap_or(0)) as u32;
                self.grant_resource(peer_id, resource_name, num_views).await.unwrap_or_default();

                return;
            }
        }


        // if not then add to inbox
        let mut inbox = self.inbox_queue.lock().await;
        inbox.push(data);
    }

    async fn handle_grant_msg(&self, addr:SocketAddr, json_obj:Value) {
        let mut pending = self.pending_approval.lock().await;

        // Collect the items that match the condition into a separate vector
        let r_name = json_obj["resource"].clone();
        let _tmp = json_obj["provider"].clone();
        let peer_id = _tmp.as_str().expect("failed to parse userid");
        let matched_items: Vec<Value> = pending.iter()
            .filter(|item| {
                peer_id == item["provider"].as_str().unwrap_or("") 
                && item["resource"] == r_name
            })
            .cloned()
            .collect();

        // If `matched_items` is empty, no items were filtered out
        if matched_items.is_empty() {
            // did not request this resource, ignore it
            return;
        }
        if json_obj["num_views"].as_u64().unwrap() > 0 { // if 0, then resource grant is denied
            // Send OK acknowledgment
            if let Err(e) = send_with_retry(&self.public_socket, b"OK", addr, MAX_RETRIES).await {
                eprintln!("Failed to send acknowledgment: {:?}", e);
                return;
            }
            // do recev_reliable to recieve img data, and save it in resources/borrowed as 'og_filename.encrp' to have uniform extention 
            let img_data = match recv_reliable(&self.public_socket, Some(Duration::from_secs(DEFAULT_TIMEOUT))).await {
                Ok((data, _, _)) => data,
                Err(e) => {
                    eprintln!("Failed to receive image data: {:?}", e);
                    return;
                }
            };
            let og_filename = json_obj["resource_name"].as_str().unwrap_or("unknown");
            let output_path = format!("resources/borrowed/{}.encrp", og_filename);
            if let Err(e) = async {
                let mut file = tokio::fs::File::create(&output_path).await?;
                file.write_all(&img_data).await?;
                Ok::<(), std::io::Error>(())
            }
            .await
            {
                eprintln!("Failed to save received file '{}': {:?}", output_path, e);
                return;
            }
            println!("Saved received resource '{}' as '{}'.", og_filename, output_path);

            // Update local resources list
            let mut resources = self.available_resources.lock().await;
            resources.push(json_obj);
        }
        // Pop from pending
        pending.retain(|item| {
            !(peer_id == item["provider"].as_str().unwrap_or("") && item["resource"].as_str() == r_name.as_str())
        });
    }

    async fn encrypt_img(&self, file_name:&str, num_views:u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // let file_path: PathBuf = Path::new("./resources/owned").join(file_name);
        // println!("file_path: {:?}", file_path);
        // Read the file data to be sent
        let mut file = File::open(file_name).await.unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).await.unwrap();

        // // Encode access info
        // let encoded: String = format!("{:?}.{}", self.public_socket.local_addr(), num_views);
        // // Pad to the maximum length, accomodating for possibly ipv6 addresses
        // let padded = format!("{:<width$}", encoded, width=62);

        // Add padded to data at the end
        // data.extend_from_slice(padded.as_bytes());

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
    
    // fetch_collection(): fetches any collection from the cloud, returns the json.
    pub async fn fetch_collection(
        &self,
        collection_name: &str,
        filter: Option<Value>,
    ) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let service_name = "ReadCollection";
        let params = vec![collection_name];
    
        // Use `send_data_with_params` to send the request and receive the response
        let filter_data:Vec<u8>;
        if filter.is_some() {
            filter_data = to_vec(&filter).unwrap_or(Vec::new());
        }
        else {
            filter_data = Vec::new();
        }

        match self
            .client
            .send_data_with_params(filter_data, service_name, params)
            .await
        {
            Ok(response_data) => {
                let response_message = String::from_utf8_lossy(&response_data);
                println!("Received response: {}", response_message);
    
                // Parse the JSON response
                let json_data: Vec<Value> = serde_json::from_str(&response_message)?;
                println!(
                    "Parsed collection data for '{}':\n{}",
                    collection_name,
                    serde_json::to_string_pretty(&json_data)?
                );
    
                // Save the collection locally
                let mut collections = self.collections.lock().await;
                collections.insert(collection_name.to_string(), json_data.clone());
                println!("Saved collection '{}' locally.", collection_name);
    
                Ok(json_data)
            }
            Err(e) => {
                eprintln!("Error during fetch_collection: {}", e);
                Err(Box::new(e))
            }
        }
    }

    async fn resolve_id(&self, id:&str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        let payload = json!({
            "UUID" : id,
        }).to_string();
        let params = vec!["users"];
        let result = self.client.send_data_with_params(payload.as_bytes().to_vec(), "ReadCollection", params.clone())
        .await.expect("Failed to resolve name from DOS.");
        let data:Vec<Value> = serde_json::from_slice(&result).expect("failed to parse json resolved");
        // check if data has a first entry, if so, 
        // let address = take data[0]["addr"]
        if let Some(first_entry) = data.get(0) {
            if let Some(address) = first_entry.get("addr") {
                let addr = address.as_str().expect("addr not a string");
                return Ok(addr.parse::<SocketAddr>()?);
            } else {
                eprintln!("The 'addr' field is missing in the first entry.");
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "The 'addr' field is missing in the first entry")));
            }
        } else {
            eprintln!("ID not registered.");
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Decryption failed")));
        }

    }
    

    // publish_info() : checks contents of resources folder, publishes a document of my own address and the list of resources (filenames) + maybe some file metadata to the cloud.
    pub async fn publish_info(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Perform login
        let payload = json!({
            "UUID" : self.id.as_str(),
            "addr" : self.public_socket.local_addr().unwrap(),
        }).to_string();
        let params = vec!["users"];
        self.client.send_data_with_params(payload.as_bytes().to_vec(), "UpdateDocument", params.clone())
        .await.expect("Failed publishing to catalog.");

        
        // Define the folder to scan
        let folder_path = "resources/owned"; // Add folder path
    
        // Read the folder contents asynchronously
        let mut resources_info = Vec::new();
        let mut entries = fs::read_dir(folder_path).await?;
    
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name().into_string().unwrap_or_default();

            let encrypted_data =  match self.encrypt_img(&entry.path().to_str().unwrap(), 0).await {
                Ok(encrypted_data) => encrypted_data,
                Err(_) => {
                    continue;
                }
            };
            // save encrypted_data in file as filename .encrp in resources/encrypted
            // Write the encrypted data to the file
            let output_dir = std::path::Path::new("resources/encrypted");
            let output_path = output_dir.join(format!("{}.encrp", file_name));
            
            // Ensure the directory exists
            if !output_dir.exists() {
                match fs::create_dir_all(&output_dir).await {
                    Ok(_) => println!("Created directory: {:?}", output_dir),
                    Err(e) => {
                        eprintln!("Failed to create directory {:?}: {}", output_dir, e);
                        continue; // Skip the current iteration if the directory cannot be created
                    }
                }
            }
            
            // Write the file
            match fs::write(&output_path, encrypted_data).await {
                Ok(_) => {
                    println!("Encrypted data saved to {:?}", output_path);
                }
                Err(e) => {
                    eprintln!("Failed to save encrypted data to file: {}", e);
                    continue; // Skip the current iteration if the file cannot be written
                }
            }
                            
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
            "UUID": *self.id.clone(), // used for updating prev entry
            "id": *self.id.clone(),
            "resources": resources_info,
        });
    
        // Serialize the JSON payload
        let payload_str = serde_json::to_string(&payload)?;
        println!("Prepared payload: {}", payload_str);
    
        let params = vec!["catalog"];
        self.client.send_data_with_params(payload_str.as_bytes().to_vec(), "UpdateDocument", params.clone())
        .await.expect("Failed publishing to catalog.");

        Ok(())
    }
    
    // request_resource(peer_addr, resource_name, num_views) : request resource from peer for a certain number of views.
    pub async fn request_resource(
        &self,
        peer_id: &str,
        resource_name: &str,
        num_views: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the request message in JSON format
        let request_message = serde_json::json!({
            "type": "request",
            "user" : *self.id.clone(),
            "provider" : peer_id,
            "resource_name": resource_name,
            "num_views": num_views,
            "UUID": format!("req:{:?}|{:?}|{}", peer_id, self.id, resource_name), // provider, requester, resource name as an ID for the 'permissions' entries
        });

        let params = vec!["permissions"];
        let _ =  self.client.send_data_with_params(request_message.to_string().as_bytes().to_vec(), "UpdateDocument", params.clone()).await.unwrap();
    
        // Send the request to the peer
        send_with_retry(
            &self.public_socket,
            request_message.to_string().as_bytes(),
            self.resolve_id(peer_id).await?,
            MAX_RETRIES,
        )
        .await?;

        // add to pending
        let mut pending = self.pending_approval.lock().await;
        pending.push(request_message);
        
        println!(
            "Requested resource '{}' with {} views from peer {}",
            resource_name, num_views, peer_id
        );
    
        Ok(())
    }
    

    /// grant_resource(peer_addr, resource_name, num_views) : grants and sends the resource to the other peer.
    pub async fn grant_resource(
        self: &Arc<Self>,
        peer_id: &str,
        resource_name: &str,
        num_views: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        
        let resource_path = format!("./resources/encrypted/{}.encrp", resource_name);
    
        // Check if the resource exists
        if !tokio::fs::metadata(&resource_path).await.is_ok() {
            eprintln!("Resource '{}' not found in the 'resources' folder.", resource_name);
            return Err("Resource not found".into());
        }
        let myself = self.clone();
        let resource_n = resource_name.to_string();
        let peer_id_ = peer_id.to_string();

        tokio::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();

            // update directory of service with this permission grant
            let entry = json!({
                "type": "grant",
                "resource": resource_n,
                "provider": *myself.id.clone(),
                "user": peer_id_,
                "num_views": num_views,
                "remaining": num_views,
                "UUID": format!("grant:{:?}|{:?}|{}", myself.id, peer_id_, resource_n), // provider, requester, resource name as an ID for the 'permissions' entries
            }).to_string();
            let params = vec!["permissions"];
            let _ =  myself.client.send_data_with_params(entry.as_bytes().to_vec(), "UpdateDocument", params.clone()).await.unwrap();

            // pop from local inbox, based on user and resource_name
            let mut inbox = myself.inbox_queue.lock().await;
            inbox.retain(|item| {
                !(peer_id_ == item["user"].as_str().unwrap_or("") && item["resource"].as_str() == Some(&resource_n))
            });

            // send grant message  to peer to exchange data 
            let _ = match send_with_retry(&socket, entry.as_bytes(), myself.resolve_id(peer_id_.as_str()).await.expect("Failed grant resource"), MAX_RETRIES).await {
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

            // let mut encrypted_data =  match myself.encrypt_img(&resource_path, num_views).await {
            //     Ok(encrypted_data) => encrypted_data,
            //     Err(_) => {
            //         return;
            //     }
            // };

            // Construct the file path by appending .encrp to the resource name
            let file_path = std::path::Path::new("resources/encrypted").join(format!("{}.encrp", resource_n));
            // Read the encrypted data from the file
            let mut encrypted_data: Vec<u8> = match std::fs::read(&file_path) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to read encrypted data from file {}: {}", file_path.display(), e);
                    Vec::new() // Return an empty vector on error
                }
            };

            // Encode access info
            let encoded: String = format!("{:?}.{}", myself.public_socket.local_addr().unwrap(), num_views);
            // Pad to the maximum length, accomodating for possibly ipv6 addresses
            let padded = format!("{:<width$}", encoded, width=62);

            // Add padded to data at the end
            encrypted_data.extend_from_slice(padded.as_bytes());
        
            // Send the encrypted image to the peer
            send_reliable(&socket, &encrypted_data, addr).await.expect("Failed to send resource to peer");
            println!(
                "- Granted resource '{}' with {} views to peer {}",
                resource_n, num_views, peer_id_
            );

            // Only if everything is successful:
            // delete original request from DOS directory of services
            let filter = json!({
                "UUID": format!("req:{:?}|{:?}|{}", myself.id, peer_id_, resource_n), // provider, requester, resource name as an ID for the 'permissions' entries
            }).to_string();
            let _ =  myself.client.send_data_with_params(filter.as_bytes().to_vec(), "DeleteDocument", params.clone()).await.unwrap();
            
            
        });
    
        Ok(())
    }

    // access_resource(resource_name, provider_addr)
    // it checks first if "resources/encrypted/resource_name.encrp" exists or not
    // if not, it returns an error, and if yes, it reads the encrp file
    // then it extracts the last 62 bits, and from them extracts num_of_views
    // finally, if num_of_views > 0, it decrypts the image using peer_decrypt_img, decrement remaining in the directory of service, and returns the raw image data
    // if num_of_views == 0, it deletes the entry from the fhe folder and the directory of service
    pub async fn access_resource(&self, resource_name: &str, provider_addr: SocketAddr) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Check if the encrypted resource exists
        let file_path = std::path::Path::new("resources/encrypted").join(format!("{}.encrp", resource_name));
        if !std::fs::metadata(&file_path).is_ok() {
            eprintln!("Encrypted resource '{}' not found in the 'resources/encrypted' folder.", resource_name);
            return Err("Resource not found".into());
        }

        // Read the encrypted data from the file
        let encrypted_data: Vec<u8> = match std::fs::read(&file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read encrypted data from file {}: {}", file_path.display(), e);
                return Err(e.into());
            }
        };

        // Extract the access info from the last 62 bytes
        let access_info = String::from_utf8_lossy(&encrypted_data[encrypted_data.len() - 62..]);
        let parts: Vec<&str> = access_info.split('.').collect();
        if parts.len() != 2 {
            eprintln!("Invalid access info found in the encrypted data: {}", access_info);
            return Err("Invalid access info".into());
        }

        // Extract the number of views
        let num_views = match parts[1].parse::<u32>() {
            Ok(views) => views,
            Err(e) => {
                eprintln!("Failed to parse the number of views from '{}': {}", parts[1], e);
                return Err(e.into());
            }
        };

        // Check if the number of views is greater than 0
        if num_views == 0 {
            // delete the entry from the folder and the directory of service
            let entry = json!({
                "type": "grant",
                "resource": resource_name,
                "provider": format!("{:?}", provider_addr),
                "user": format!("{:?}", self.public_socket.local_addr().unwrap()),
                "num_views": num_views,
                "remaining": num_views,
                "UUID": format!("grant:{:?}|{:?}|{}", provider_addr, self.public_socket.local_addr().unwrap(), resource_name), // provider, requester, resource name as an ID for the 'permissions' entries
            }).to_string();
            let params = vec!["permissions"];
            let _ =  self.client.send_data_with_params(entry.as_bytes().to_vec(), "DeleteDocument", params.clone()).await.unwrap();

            // Delete the encrypted resource file
            if let Err(e) = std::fs::remove_file(&file_path) {
                eprintln!("Failed to delete the encrypted resource file '{}': {}", file_path.display(), e);
            }

            // return an error
            eprintln!("No views remaining for resource '{}'.", resource_name);
            return Err("No views remaining".into());
        }

        // Decrypt the image data
        let decrypted_data = peer_decrypt_img(&encrypted_data).await?;


        // Update the remaining views in the directory of service
        let entry = json!({
            "type": "grant",
            "resource": resource_name,
            "provider": format!("{:?}", provider_addr),
            "user": format!("{:?}", self.public_socket.local_addr().unwrap()),
            "num_views": num_views - 1,
            "remaining": num_views - 1,
            "UUID": format!("grant:{:?}|{:?}|{}", provider_addr, self.public_socket.local_addr().unwrap(), resource_name), // provider, requester, resource name as an ID for the 'permissions' entries
        }).to_string(); 
        let params = vec!["permissions"];
        let _ =  self.client.send_data_with_params(entry.as_bytes().to_vec(), "UpdateDocument", params.clone()).await.unwrap();

        // Return the decrypted image data
        Ok(decrypted_data)

    }
}