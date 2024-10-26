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
 


async fn mock_encrypt_callback(data: Vec<u8>) -> Vec<u8> {
    let img_path = "files/to_encrypt.jpg";
    let output_path = "files/encrypted_output.jpg";

    // Step 1: Write data bytes to a file (e.g., 'to_encrypt.jpg')
    let mut file = File::create(img_path).await.unwrap(); // Use await for the async File::create
    file.write_all(&data).await.unwrap(); // Use await for async write_all

    // Step 2: Call `server_encrypt_img` to perform encryption on the file
    server_encrypt_img("files/placeholder.jpg", img_path, output_path);

    // Step 3: Read the encrypted output file as bytes
    let mut encrypted_file = File::open(output_path).await.unwrap(); // Use await for async File::open
    let mut encrypted_data = Vec::new();
    encrypted_file.read_to_end(&mut encrypted_data).await.unwrap(); // Use await for async read_to_end

    // Return the encrypted data as the output
    encrypted_data
}


#[tokio::main]
async fn main() {
    // Test Setup:
    // Create two server nodes: one elected, one not elected.
    // let callback = mock_encrypt_callback;

    let server_addr1: SocketAddr = "127.0.0.1:8081".parse().unwrap();
    let server_addr2: SocketAddr = "127.0.0.1:8082".parse().unwrap();

    let node_map: HashMap<String, SocketAddr> = vec![
        ("Server1".to_string(), server_addr1),
        ("Server2".to_string(), server_addr2),
    ].into_iter().collect();

    let chunk_size:usize = 1024;

    // Server 1 (elected = true)
    let server1 = CloudNode::new(server_addr1, None, chunk_size, true).await.unwrap();
    let server1_arc = Arc::new(server1);

    // Server 2 (elected = false)
    let server2 = CloudNode::new(server_addr2, None, chunk_size, false).await.unwrap();
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
    client.send_data(file_path).await.unwrap();

    // Ensure the servers handled the connection properly (let them process)
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // // Cleanup: Remove test file
    // tokio::fs::remove_file(file_path).await.unwrap();

    // Ensure the servers complete their tasks
    // server1_task.await.unwrap();
    // server2_task.await.unwrap();

}