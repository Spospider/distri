use clap::Parser; 
use distri::client::Client;
use distri::cloud::CloudNode;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use std::collections::HashSet;
use serde_json::json;
use serde_json::to_vec;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

mod utils;
use utils::{decrypt_image, write_to_file};



#[derive(Parser, Debug)]
struct Arguments {
    #[arg(short, long)]
    mode: String,
    #[arg(long)]
    identifier: Option<u32>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    report: bool,
    #[arg(long, value_delimiter = ',')]
    ips: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();
    
    let mode = args.mode;
    let other_ips: Vec<SocketAddr> = args.ips
        .iter() // Create an iterator over the Vec<String>
        .cloned() // Clone each String (to avoid borrowing issues)
        .collect::<HashSet<String>>() // Collect into a HashSet to remove duplicates
        .into_iter() // Convert back into an iterator
        .map(|ip| ip.parse().expect("Failed to parse an IP address")) // Parse each unique String to SocketAddr
        .collect(); // Collect the results into a Vec<SocketAddr>

    let own_addr:SocketAddr = args.ips[0].parse().expect("REASON");

    match mode.as_str() {
        "server" => {
            // Server Mode
            let identifier = args.identifier.unwrap_or(10);
            let is_elected = identifier == 10; // Use "10" as a unique identifier for an elected server

            // Initialize server and other nodes in the network
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (i, addr) in other_ips.iter().enumerate() {
                node_map.insert(format!("Server{}", i + 1), *addr);
            }

            // define DB tables for directory of service
            let table_names = Some(vec!["catalog"]);

            // Create and start the server
            let server = CloudNode::new(own_addr, Some(node_map), 1024, is_elected, table_names).await.unwrap();
            let server_arc = Arc::new(server);
            tokio::spawn(async move {
                server_arc.serve().await.unwrap();
            });

            println!("Server {} started at {:?}", identifier, own_addr);
            // Wait for a termination signal (Ctrl+C)
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C signal handler");
            println!("Shutting down server...");
        }
        "client" => {
            // Client Mode
            let report = args.report;
            let chunk_size: usize = 1024;
            let file_path = "files/img.jpg";

            // Create and configure the client with the servers' addresses
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (i, addr) in other_ips.iter().enumerate() {
                node_map.insert(format!("Server{}", i + 1), *addr);
            }
            let mut client = Client::new(Some(node_map.clone()), Some(chunk_size));

            // Register all servers in the client node map
            for (name, addr) in &node_map {
                client.register_node(name.clone(), *addr);
            }
            let mut failures = 0;

            // Load test: Send 10_000 requests
            for _ in 0..10 {
                // Read the file data to be sent
                let mut file = File::open(file_path).await.unwrap();
                let mut data = Vec::new();
                file.read_to_end(&mut data).await.unwrap();

                let result = client.send_data(data, "Encrypt").await;
                println!("Sent image: {}", file_path);
                match result {
                    Ok(data) => {
                        // Step 2: Write to file
                        if let Err(e) = write_to_file("files/received_with_hidden.png", &data).await {
                            eprintln!("Writing received file failed: {}", e);
                        }
                        // Step 3: Decrypt Image
                        if let Err(e) = decrypt_image("files/received_with_hidden.png", "files/extracted_hidden_image.jpg").await {
                            eprintln!("Error decrypting hidden image from {} to {}: {}", "files/received_with_hidden.png", "files/extracted_hidden_image.jpg", e);
                        }
                    }
                    Err(e) => {
                        println!("Receiving file failed due to {:?}", e);
                        failures += 1;
                    }
                }
                sleep(Duration::from_millis(50)).await; // Optional delay between requests
            }

            let params = vec!["catalog"];
            let client_catalogue =json!({
                "images" : {
                    "img1" : {
                        "size" : "value",
                        "dimentions" : "value",
                        "thumbnail" : "value"
                    },
                    "img2" : {
                        "size" : "value",
                        "dimentions" : "value",
                        "thumbnail" : "value"
                    }
                }
            }); 
            // converted to an array of bytes
            let client_catalogue_bytes = to_vec(&client_catalogue).expect("Failed to serialize JSON");

            println!("pushing to DB...");

            let result = client.send_data_with_params(client_catalogue_bytes, "AddDocument", params).await.unwrap();
            println!("{}", String::from_utf8_lossy(&result));

            println!("Reading directory of service...");
            // Read directory of service
            let params = vec!["catalog"];
            let result = client.send_data_with_params(Vec::new(), "ReadTable", params).await.unwrap();
            println!("{}", String::from_utf8_lossy(&result));


            // Optionally gather stats if `report` flag is set
            if report {
                println!("doing report");
                client.collect_stats().await;
                println!("Total failed Tasks: {}", failures);
            }
        }
        _ => {
            eprintln!("Invalid mode specified. Use 'server' or 'client'.");
        }
    }
}

// cargo run -- --mode server --ips 127.0.0.1:3000,127.0.0.1:3001

// cargo run -- --mode server --ips 127.0.0.1:3001,127.0.0.1:3000

// cargo run -- --mode client --report --ips 127.0.0.1:3000,127.0.0.1:3001


