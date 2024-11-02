use clap::{Arg, ArgGroup, Command, Parser, Subcommand}; 
use distri::client::Client;
use distri::cloud::CloudNode;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use std::collections::HashSet;



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
    // let matches = Command::new("Distributed Test")
    //     .arg(Arg::new("mode")
    //         .required(true)
    //         .help("Mode to run the program in: 'server' or 'client'")
    //     )
    //     .arg(Arg::new("identifier")
    //         .required_if_eq("mode", "server")
    //         .help("Unique identifier for the server node (only required in server mode)")
    //     )
    //     .arg(Arg::new("report")
    //         .required(false)
    //         .help("Optional flag for clients to gather and report stats at the end")
    //     )
    //     .arg(Arg::new("ips")
    //         .help("First IP address (this will be the own address)")
    //         .required(true)
    //     )
    //     .group(ArgGroup::new("ips_group")
    //         .args(&["ips"])
    //         .required(true)
    //         .multiple(true)
    //         // .help("Specify at least one other IP address")
    //     )
    //     .get_matches();
    let mut args = Arguments::parse();
    

    

    // Determine mode (server or client)
    let mode = args.mode;
    println!("mode: {}", mode);
    // assign own_addr = other_ips[0] 
    // let own_addr: SocketAddr = matches.get_one::<String>("ip_self").unwrap().parse().expect("Failed to parse own IP address");
    // let other_ips: Vec<SocketAddr> = args.ips
    //     .iter() // Create an iterator over the Vec<String>
    //     .map(|ip| ip.parse().expect("Failed to parse an IP address")) // Parse each String to SocketAddr
    //     .collect(); // Collect the results into a Vec<SocketAddr>
    // remove duplicates!!
    let other_ips: Vec<SocketAddr> = args.ips
        .iter() // Create an iterator over the Vec<String>
        .cloned() // Clone each String (to avoid borrowing issues)
        .collect::<HashSet<String>>() // Collect into a HashSet to remove duplicates
        .into_iter() // Convert back into an iterator
        .map(|ip| ip.parse().expect("Failed to parse an IP address")) // Parse each unique String to SocketAddr
        .collect(); // Collect the results into a Vec<SocketAddr>

    let own_addr:SocketAddr = args.ips[0].parse().expect("REASON");
    println!("own_addr: {}", own_addr);



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

            // Create and start the server
            let server = CloudNode::new(own_addr, Some(node_map), is_elected).await.unwrap();
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

            // Load test: Send 10_000 requests
            for _ in 0..2 {
                match client.send_data(file_path, "Encrypt").await {
                    Ok(_) => println!("Sent image: {}", file_path),
                    Err(e) => eprintln!("Failed to send image {}: {:?}", file_path, e),
                }
                sleep(Duration::from_millis(50)).await; // Optional delay between requests
            }

            // Optionally gather stats if `report` flag is set
            if report {
                println!("doing report");
                client.collect_stats().await;
            }
        }
        _ => {
            eprintln!("Invalid mode specified. Use 'server' or 'client'.");
        }
    }
}

// cargo run -- --mode server --identifier 10 --ips 127.0.0.1:3000,127.0.0.1:3001,127.0.0.1:3001

// cargo run -- --mode client --report true --ips 127.0.0.1:3000,127.0.0.1:3001,127.0.0.1:3001