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
use tokio::sync::Barrier;
use std::process::{Command, Child};


use show_image::*;

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

    #[arg(long, default_value_t = 1)]
    n: usize, 

    #[arg(long, default_value_t = 1)]
    n_servers: usize, 
}

#[show_image::main]
#[tokio::main]
async fn main() {
    let base_ip = "127.0.0.1".to_string();
    let start_port = 3000;
    let args = Arguments::parse();
    
    let mode = args.mode;
    let other_ips: Vec<SocketAddr> = args.ips
        .iter() // Create an iterator over the Vec<String>
        .cloned() // Clone each String (to avoid borrowing issues)
        .collect::<HashSet<String>>() // Collect into a HashSet to remove duplicates
        .into_iter() // Convert back into an iterator
        .map(|ip| ip.parse().expect("Failed to parse an IP address")) // Parse each unique String to SocketAddr
        .collect(); // Collect the results into a Vec<SocketAddr>

    match mode.as_str() {
        "server" => {
            // Server Mode
            // let identifier = args.identifier.unwrap_or(10);

            // Initialize server and other nodes in the network
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (_, addr) in other_ips.iter().enumerate() {
                node_map.insert(addr.port().to_string(), *addr);
            }
            let own_addr:SocketAddr = args.ips[0].parse().expect("Failed to parse an Socket address");

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
            let own_addr:SocketAddr = args.ips[0].parse().expect("Failed to parse an Socket address");

            // Initialize server and other nodes in the network
            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for (i, addr) in other_ips.iter().enumerate() {
                node_map.insert(addr.port().to_string(), *addr);
            }

            // Create and start the server
            let peer = Peer::new(&identifier.as_str(), own_addr, Some(node_map)).await.unwrap();
            run_program(&peer).await;
            
        }
        "test-client-encrypt" => {
            // test-client-encrypt mode
            let barrier = Arc::new(Barrier::new(args.n));
            let mut tasks = vec![];

            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for j in 0..args.n_servers {
                let port = start_port + j;
                let server_port = start_port + j;
                let server_addr = format!("{}:{}", base_ip, server_port);
                let server_socket: SocketAddr = server_addr.parse().unwrap();
                node_map.insert(format!("Server{}", j), server_socket);
            }
            let file_path = "files/img.jpg";
            let mut file = File::open(file_path).await.unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).await.unwrap();

            for i in 0..args.n {
                // Create and configure the client with the servers' addresses
                
                let client = Client::new(Some(node_map.clone()), Some(1024));
                let barrier = barrier.clone();
                let _data = data.clone();
                tasks.push(tokio::spawn(async move {
                    let mut failures = 0;
                    let start_time = Instant::now();
                    for _ in 0..10 {
                        let result = client.send_data(_data.clone(), "Encrypt").await;
                        match result {
                            Ok(_) => {}
                            Err(e) => {
                                failures += 1;
                                // eprintln!("{}", e);
                            },
                        }
                    }

                    if i == args.n -1 {
                        let _ = client.collect_stats().await;
                    }
                    let elapsed = start_time.elapsed();
                    barrier.wait().await;
                    (failures, elapsed)
                }));
            }

            let mut total_failures = 0;
            let mut total_time = Duration::new(0, 0);

            for task in tasks {
                let (failures, elapsed) = task.await.unwrap();
                total_failures += failures;
                total_time += elapsed;
            }

            println!("Test completed: Total failed tasks: {}", total_failures);
            println!("Avg response time: {}", total_time.as_secs_f64() / args.n as f64 / 10.0 as f64);
        }

        "test-client-db" => {
            // test-client-db mode
            let barrier = Arc::new(Barrier::new(args.n));
            let mut tasks = vec![];

            let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
            for j in 0..args.n_servers {
                let port = start_port + j;
                let server_port = start_port + j;
                let server_addr = format!("{}:{}", base_ip, server_port);
                let server_socket: SocketAddr = server_addr.parse().unwrap();
                node_map.insert(format!("Server{}", j), server_socket);
            }

            for i in 0..args.n {
                let client = Client::new(Some(node_map.clone()), Some(1024));
                let barrier = barrier.clone();
                tasks.push(tokio::spawn(async move {
                    let mut failures = 0;
                    let start_time = Instant::now();
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
                    let result = client.send_data_with_params(to_vec(&client_catalogue).unwrap(), "AddDocument", params.clone()).await;
                    match result {
                        Ok(_) => {}
                        Err(_) => failures += 1,
                    }
                    let result = client.send_data_with_params(Vec::new(), "ReadCollection", params.clone()).await;
                    match result {
                        Ok(_) => {}
                        Err(_) => failures += 1,
                    }
                    client_catalogue["New thing"] = Value::String("some txt".to_string());
                    let result = client.send_data_with_params(to_vec(&client_catalogue).unwrap(), "UpdateDocument", params.clone()).await;
                    match result {
                        Ok(_) => {}
                        Err(_) => failures += 1,
                    }
                    let result = client.send_data_with_params(Vec::new(), "ReadCollection", params.clone()).await;
                    match result {
                        Ok(_) => {}
                        Err(_) => failures += 1,
                    }

                    if i == args.n -1 {
                        let _ = client.collect_stats().await;
                    }

                    let elapsed = start_time.elapsed();
                    barrier.wait().await;
                    (failures, elapsed)
                }));
            }

            let mut total_failures = 0;
            let mut total_time = Duration::new(0, 0);

            for task in tasks {
                let (failures, elapsed) = task.await.unwrap();
                total_failures += failures;
                total_time += elapsed;
            }

            println!("Test completed: Total failed tasks: {}", total_failures);
            println!("Avg response time: {}", total_time.as_secs_f64() / args.n as f64);
        }

        "test-servers" => {
            // test-servers mode - automatically generate IPs starting from 3000
            let barrier = Arc::new(Barrier::new(args.n + 1));
            // let mut tasks = vec![];

            for i in 0..args.n {
                let port = start_port + i;
                let server_addr = format!("{}:{}", base_ip, port);
                // let own_addr: SocketAddr = server_addr.parse().unwrap();
                
                // Generate the node map for this server
                let mut node_map: HashMap<String, SocketAddr> = HashMap::new();
                let mut ips: Vec<String> = vec![server_addr.clone()];
                for j in 0..args.n {
                    if i != j {
                        let server_port = start_port + j;
                        let server_addr = format!("{}:{}", base_ip, server_port);
                        ips.push(server_addr);
                        // let server_socket: SocketAddr = server_addr.parse().unwrap();
                        // node_map.insert(server_port.to_string(), server_socket);
                    }
                }
                // let table_names = Some(vec!["catalog", "permissions", "users"]);


                // tasks.push(tokio::spawn(async move {
                //     let server = CloudNode::new(4, own_addr, Some(node_map), 1024, table_names).await.unwrap();
                //     let server_arc = Arc::new(server);
                //     server_arc.serve().await.unwrap();
                // }));
                let ips_arg = ips.join(",");
                let child = Command::new("./target/release/example")
                    // .arg("./target/release/example") // Or `--release` for optimized builds
                    .arg("--mode")
                    .arg("server") // Replace with the actual binary name for your server
                    .arg("--ips").arg(ips_arg) // Pass arguments to the server process
                    .spawn()
                    .expect("Failed to spawn process");

            }

            // for task in tasks {
            //     task.await.unwrap();
            // }
        }

        _ => {
            eprintln!("Invalid mode specified. Use 'server', 'client', 'test-client-encrypt', 'test-client-db' or 'test-servers'.");
        }
    
    }
}

// cargo run -- --mode server --ips 127.0.0.1:3000,127.0.0.1:3001

// cargo run -- --mode server --ips 127.0.0.1:3001,127.0.0.1:3000

// cargo run -- --mode client --report --ips 127.0.0.1:3000,127.0.0.1:3001

// cargo run -- --mode peer --identifier ali --ips 127.0.0.1:2000,127.0.0.1:3000,127.0.0.1:3001
// the first address is the peer's public address

// for tests: ./target/release/example --mode test-servers --n 3
// for tests: ./target/release/example --mode test-client-encrypt --n 100 --n-servers 3
// for tests: ./target/release/example --mode test-client-db --n 100 --n-servers 3