// // tests/integration_test.rs
// extern crate clap;

// use clap::Arg;
// use clap::Command;

// use distri::client::Client;
// use distri::cloud::CloudNode;
// use distri::utils::{END_OF_TRANSMISSION, server_decrypt_img, server_encrypt_img};

// use tokio::fs::File;
// use tokio::io::{AsyncReadExt, AsyncWriteExt};
// use std::sync::Arc;
// use std::collections::HashMap;
// use std::net::SocketAddr;
// use std::time::{Duration, Instant};
// use tokio::time::sleep;
// use std::fs;
// use rand::seq::SliceRandom;
// use rand::rngs::StdRng;
// use rand::Rng; // Add this import for rand traits
// use rand::prelude::*;




// async fn mock_encrypt_callback(data: Vec<u8>) -> Vec<u8> {
//     let img_path = "files/to_encrypt.jpg";
//     let output_path = "files/encrypted_output.jpg";

//     let mut file = File::create(img_path).await.unwrap();
//     file.write_all(&data).await.unwrap();
//     server_encrypt_img("files/placeholder.jpg", img_path, output_path);

//     let mut encrypted_file = File::open(output_path).await.unwrap();
//     let mut encrypted_data = Vec::new();
//     encrypted_file.read_to_end(&mut encrypted_data).await.unwrap();

//     encrypted_data
// }

// async fn setup_local_mode(num_servers: usize, num_clients: usize, chunk_size: usize, duration: u64) {
//     let ip_addr = "127.0.0.1"; // Use localhost for local mode
//     let server_start_port = 8081;

//     // Call setup_servers_mode to initialize servers
//     setup_servers_mode(num_servers, ip_addr, 3000, chunk_size, duration).await;

//     // Call setup_clients_mode to initialize clients and set them to connect to the servers
//     setup_clients_mode(num_clients, ip_addr, 3000, chunk_size, duration, num_servers).await;
// }

// async fn setup_clients_mode(num_clients: usize, ip_addr: &str, start_port: u16, chunk_size: usize, test_duration_secs: u64, num_servers:usize) {
//     let mut node_map = HashMap::new();
//     let server_addr: SocketAddr = format!("{}:{}", ip_addr, start_port).parse().unwrap();

//     for i in 0..num_servers {
//         let port = start_port + i as u16; // Increment port for each server
//         let server_addr: SocketAddr = format!("{}:{}", ip_addr, port)
//             .parse()
//             .expect("Failed to parse server address");
        
//         // Use a unique identifier for each server, e.g., "Server1", "Server2", etc.
//         let server_name = format!("Server{}", i + 1);
//         node_map.insert(server_name, server_addr);
//     }



//     // Read all image files in the `./files/` directory once
//     let image_files: Vec<String> = fs::read_dir("./files")
//         .unwrap()
//         .filter_map(|entry| {
//             let entry = entry.unwrap();
//             let path = entry.path();
//             if path.is_file() && path.extension().map_or(false, |ext| ext == "jpg" || ext == "png") {
//                 Some(path.to_string_lossy().into_owned())
//             } else {
//                 None
//             }
//         })
//         .collect();

//     println!("files: {:?}", image_files);

//     // Define a test duration to run the load testing for a specific amount of time
//     let test_duration = Duration::from_secs(test_duration_secs);

//     // Spawn multiple clients, each sending random images continuously within the test duration
//     for i in 0..num_clients {
//         let mut client = Client::new(Some(node_map.clone()), Some(chunk_size));
//         for (server_name, server_addr) in &node_map {
//             client.register_node(server_name.clone(), *server_addr); // Register each server node
//         }
    
//         let image_files = image_files.clone();

//         tokio::spawn(async move {
//             let start = tokio::time::Instant::now();
//             let mut rng = StdRng::from_entropy();

//             while start.elapsed() < test_duration*1000 {
                
//                 // Choose a random image from the list
//                 if let Some(image_path) = image_files.choose(&mut rng) {
//                     // Send the selected image file
                    
//                     match client.send_data(image_path, "Encrypt").await {
//                         Ok(_) => println!("Sent image: {}", image_path),
//                         Err(e) => eprintln!("Failed to send image {}: {:?}", image_path, e),
//                     }
//                     println!("here");
//                 }
//                 // Optionally, add a slight delay between requests to avoid instantaneous bombardment
//                 sleep(Duration::from_millis(50)).await;
//             }
//             println!("Client finished sending images.");
//             // gather server stats by doing client.send_data(image_path, "Stats")
//             if i == num_clients - 1 {
//                 client.collect_stats();
//             }
//         });
//     }
// }


// async fn setup_servers_mode(num_servers: usize, ip_addr: &str, start_port: u16, chunk_size: usize, duration: u64) {
//     let mut servers = Vec::new();

//     for i in 0..num_servers {
//         let port = start_port + i as u16;
//         // println!("created server at: {}:{}", ip_addr, port);
//         let addr: SocketAddr = format!("{}:{}", ip_addr, port).parse().unwrap();
//         let elected = i == 0;
//         let server = CloudNode::new(addr, None, elected).await.unwrap();
//         servers.push(Arc::new(server));
//     }

//     for server in servers {
//         let server_arc = server.clone();
//         tokio::spawn(async move {
//             server_arc.serve().await.unwrap();
//         });
//     }
// }
// #[tokio::main]
// async fn main() {
//     let matches = Command::new("Distributed Test")
//         .arg(Arg::new("mode")
//             .long("mode")
//             .help("Mode to run the program in (local, clients, or servers)")
//             .required(true)
//         )
//         .arg(Arg::new("num_servers")
//             .long("num_servers")
//             .help("Number of servers (for 'local' or 'servers' mode)")
//             .default_value("2") // provide a default value for clarity
//         )
//         .arg(Arg::new("num_clients")
//             .long("num_clients")
//             .help("Number of clients (for 'local' or 'clients' mode)")
//             .default_value("2") // provide a default value for clarity
//         )
//         .arg(Arg::new("ip_addr")
//             .long("ip_addr")
//             .help("IP address of the hosting machine (for 'clients' or 'servers' mode)")
//             .required_if_eq("mode", "clients") // make it required only in clients mode
//         )
//         .arg(Arg::new("chunk_size")
//             .long("chunk_size")
//             .help("Size of each data chunk")
//             .default_value("1024")
//         )
//         .arg(Arg::new("duration")
//             .long("duration")
//             .help("Duration for the test run in seconds")
//             .default_value("10")
//         )
//         .get_matches();

//     let mode = matches.get_one::<String>("mode").unwrap();
//     let chunk_size: usize = matches.get_one::<String>("chunk_size").unwrap().parse().unwrap(); // Parse the value from String to usize

//     match mode.as_str() { // Use as_str() to convert &String to &str
//         "local" => {
//             let num_servers: usize = matches.get_one::<String>("num_servers").unwrap().parse().unwrap(); // Parse to usize
//             let num_clients: usize = matches.get_one::<String>("num_clients").unwrap().parse().unwrap(); // Parse to usize
//             let duration: u64 = matches.get_one::<String>("duration").unwrap().parse().unwrap(); // Parse to u64
//             setup_local_mode(num_servers, num_clients, chunk_size, duration).await;
//         },
//         "clients" => {
//             let num_clients: usize = matches.get_one::<String>("num_clients").unwrap().parse().unwrap(); // Parse to usize
//             let num_servers: usize = matches.get_one::<String>("num_servers").unwrap().parse().unwrap(); // Parse to usize
//             let ip_addr = matches.get_one::<String>("ip_addr").expect("IP address required for clients mode");
//             let duration: u64 = matches.get_one::<String>("duration").unwrap().parse().unwrap(); // Parse to u64
//             setup_clients_mode(num_clients, ip_addr, 3000, chunk_size, duration, num_servers).await;
//         },
//         "servers" => {
//             let num_servers: usize = matches.get_one::<String>("num_servers").unwrap().parse().unwrap(); // Parse to usize
//             let ip_addr = matches.get_one::<String>("ip_addr").expect("IP address required for servers mode");
//             let duration: u64 = matches.get_one::<String>("duration").unwrap().parse().unwrap(); // Parse to u64
//             setup_servers_mode(num_servers, ip_addr, 3000, chunk_size, duration).await;
//         },
//         _ => {
//             eprintln!("Invalid mode specified. Use 'local', 'clients', or 'servers'.");
//         }
//     }
// }


// tests/integration_test.rs
use distri::client::Client;
use distri::cloud::CloudNode;
use distri::utils::{END_OF_TRANSMISSION, server_decrypt_img, server_encrypt_img};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use std::sync::{Arc};
use std::net::SocketAddr;
use std::collections::HashMap;
use tokio::runtime::Handle;
 




#[tokio::main]
async fn main() {
    // Test Setup:
    // Create two server nodes: one elected, one not elected.

    let server_addr1: SocketAddr = "127.0.0.1:8081".parse().unwrap();
    let server_addr2: SocketAddr = "127.0.0.1:8082".parse().unwrap();

    let node_map: HashMap<String, SocketAddr> = vec![
        ("Server1".to_string(), server_addr1),
        ("Server2".to_string(), server_addr2),
    ].into_iter().collect();

    let chunk_size:usize = 1024;

    // Server 1 (elected = true)
    let server1 = CloudNode::new(server_addr1, None, true).await.unwrap();
    let server1_arc = Arc::new(server1);

    // Server 2 (elected = false)
    let server2 = CloudNode::new(server_addr2, None, false).await.unwrap();
    let server2_arc = Arc::new(server2);

    // Spawn the server tasks
    let server1_task = tokio::spawn(async move {
        server1_arc.serve().await.unwrap();
    });

    let server2_task = tokio::spawn(async move {
        server2_arc.serve().await.unwrap();
    });

    // Client setup: Create a client and register the two servers
    let mut client = Client::new(Some(node_map), Some(chunk_size));

    // Test file creation (simulate sending `test.png`)
    let file_path = "files/img.jpg";

    // Register the server nodes in the client
    client.register_node("Server1".to_string(), server_addr1);
    client.register_node("Server2".to_string(), server_addr2);


    // Simulate sending `test.png` to the servers
    client.send_data(file_path, "Encrypt").await.unwrap();

    // Ensure the servers handled the connection properly (let them process)
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // // Cleanup: Remove test file
    // tokio::fs::remove_file(file_path).await.unwrap();

    // Ensure the servers complete their tasks
    // server1_task.await.unwrap();
    // server2_task.await.unwrap();

}