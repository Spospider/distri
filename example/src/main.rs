use clap::Parser; 
use distri::client::Client;
use distri::cloud::CloudNode;
use distri::peer::Peer;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::path::Path;
use std::time::{Duration, Instant};
// use tokio::time::sleep;
use std::collections::HashSet;
use serde_json::{to_vec, Value, json};

use tokio::fs::File;
use tokio::io::AsyncReadExt;

mod utils;
use utils::{decrypt_image, write_to_file};

mod app;
use app::run_program;


#[derive(Parser, Debug)]
struct Arguments {
    #[arg(short, long)]
    mode: String,
    #[arg(long)]
    identifier: Option<String>,
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

    let own_addr:SocketAddr = args.ips[0].parse().expect("Failed to parse an Socket address");

    match mode.as_str() {
        "server" => {
            // Server Mode
            // let identifier = args.identifier.unwrap_or(10);

            // Initialize server and other nodes in the network
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (_, addr) in other_ips.iter().enumerate() {
                node_map.insert(addr.port().to_string(), *addr);
            }

            // define DB tables for directory of service
            let table_names = Some(vec!["catalog", "permissions", "users"]);

            // Create and start the server
            let server = CloudNode::new(4, own_addr, Some(node_map), 1024, table_names).await.unwrap();
            let server_arc = Arc::new(server);
            server_arc.serve().await.unwrap();
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
            let  client = Client::new(Some(node_map.clone()), Some(chunk_size));

            // Register all servers in the client node map
            // for (name, addr) in &node_map {
            //     client.register_node(name.clone(), *addr);
            // }
            let mut failures = 0;

            // Load test: Send 10_000 requests
            let start_time = Instant::now();
            for _ in 0..10 {
                // Read the file data to be sent
                let mut file = File::open(file_path).await.unwrap();
                let mut data = Vec::new();
                file.read_to_end(&mut data).await.unwrap();

                let result = client.send_data(data, "Encrypt").await;
                
                match result {
                    Ok(data) => {
                        // Step 2: Write to file
                        println!("Sent image: {}", file_path);
                        if !Path::new("files/received_with_hidden.png").exists() {
                            if let Err(e) = write_to_file("files/received_with_hidden.png", &data).await {
                                eprintln!("Writing received file failed: {}", e);
                            }
                            // Step 3: Decrypt Image
                            if let Err(e) = decrypt_image("files/received_with_hidden.png", "files/extracted_hidden_image.jpg").await {
                                eprintln!("Error decrypting hidden image from {} to {}: {}", "files/received_with_hidden.png", "files/extracted_hidden_image.jpg", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Receiving file failed due to {:?}", e);
                        failures += 1;
                    }
                }
                // sleep(tokio::time::Duration::from_millis(50)).await; // Optional delay between requests
            }

            let params = vec!["catalog"];
            let mut client_catalogue =json!({
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
                },
                "doc" : "1",
                "provider" : "127.0.0.1:1234"
            }); 
            // converted to an array of bytes
            let client_catalogue_bytes = to_vec(&client_catalogue).expect("Failed to serialize JSON");
            println!("pushing to DB...");
            let result = client.send_data_with_params(client_catalogue_bytes, "AddDocument", params.clone()).await.unwrap();
            println!("Add Doc Result: {}", String::from_utf8_lossy(&result));

            // converted to an array of bytes
            client_catalogue["doc"] = Value::String("2".to_string());
            let client_catalogue_bytes = to_vec(&client_catalogue).expect("Failed to serialize JSON");
            println!("pushing to DB...");
            let result = client.send_data_with_params(client_catalogue_bytes, "AddDocument", params.clone()).await.unwrap();
            println!("Add Doc Result: {}", String::from_utf8_lossy(&result));

            client_catalogue["UUID"] = Value::String(String::from_utf8_lossy(&result).to_string());
            client_catalogue["New thing"] = Value::String("some txt".to_string());

            let client_catalogue_bytes = to_vec(&client_catalogue).expect("Failed to serialize JSON");

            let result = client.send_data_with_params(client_catalogue_bytes, "UpdateDocument", params.clone()).await.unwrap();
            println!("Update Doc Result: {}", String::from_utf8_lossy(&result));

            
            println!("Reading directory of service...");
            // Read directory of service filtered
            let filter =json!({
                "doc" : "1"
            }); 
            let params = vec!["catalog"];
            let result = client.send_data_with_params(to_vec(&filter).expect(""), "ReadCollection", params).await.unwrap();
            println!("Directory of Service Result:\n{}", String::from_utf8_lossy(&result));
            // Read directory of service
            let params = vec!["catalog"];
            let result = client.send_data_with_params(Vec::new(), "ReadCollection", params).await.unwrap();
            println!("Directory of Service Result:\n{}", String::from_utf8_lossy(&result));
            let elapsed: Duration = start_time.elapsed();


            // Optionally gather stats if `report` flag is set
            if report {
                println!("doing report");
                let _ = client.collect_stats().await;
                println!("Total failed Tasks: {}", failures);
                println!("Total Test Time: {}", elapsed.as_secs_f64());
            }
        }
        "peer" => {
            // Server Mode
            let identifier = args.identifier.expect("No identifier was specified, use the --identifier arg.");

            // Initialize server and other nodes in the network
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (i, addr) in other_ips.iter().enumerate() {
                node_map.insert(addr.port().to_string(), *addr);
            }

            // Create and start the server
            let peer = Peer::new(&identifier.as_str(), own_addr, Some(node_map)).await.unwrap();
            run_program(&peer).await;
            
        }
        _ => {
            eprintln!("Invalid mode specified. Use 'server' or 'client'.");
        }
    }
}

// cargo run -- --mode server --ips 127.0.0.1:3000,127.0.0.1:3001

// cargo run -- --mode server --ips 127.0.0.1:3001,127.0.0.1:3000

// cargo run -- --mode client --report --ips 127.0.0.1:3000,127.0.0.1:3001

// cargo run -- --mode peer --identifier ali --ips 127.0.0.1:2000,127.0.0.1:3000,127.0.0.1:3001
// the first address is the peer's public address

