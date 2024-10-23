// tests/integration_test.rs
use tokio;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use cloud::CloudNode;
use client::Client;

#[tokio::test]
async fn test_client_server_communication() -> Result<(), Box<dyn std::error::Error>> {
    // Callback function to handle server-side file handling (empty in this case)
    let callback = Arc::new(Mutex::new(|_file_data: Vec<u8>| {
        // Handle file data if needed (currently doing nothing)
    }));

    // Start the server (CloudNode) on a background task
    let server_addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let cloud_node = CloudNode::new(callback.clone(), server_addr, None, 1024)?;
    let cloud_node = Arc::new(cloud_node);

    let server = tokio::spawn({
        let cloud_node = cloud_node.clone();
        async move {
            cloud_node.serve().await.unwrap();
        }
    });

    // Set up the client
    let mut nodes = HashMap::new();
    nodes.insert("server".to_string(), server_addr);
    let client = Client::new(Some(nodes), Some(1024));

    // Send a message from the client to the server
    client.send_data("Hello, server!", None, "server").await?;

    // Await the server task to ensure it runs until completion (it won't in this case as it's a loop)
    server.abort();  // Abort server loop after test

    Ok(())
}