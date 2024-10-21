use distri::client::Client;
use std::net::SocketAddr;

#[test]
fn test_client_register_node() {
    let client = Client::default();
    let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
    client.register_node("Node1".to_string(), addr);
    
    // Check if the node was correctly registered (using a public API function)
    let nodes = client.get_nodes();
    assert_eq!(nodes.get("Node1"), Some(&addr));
}

#[test]
fn test_client_send_data() {
    let client = Client::default();
    let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
    client.register_node("Node1".to_string(), addr);

    let data = b"Hello, world!";
    let result = client.send_data(data);

    // Sending data should succeed (result is Ok)
    assert!(result.is_ok());
}

#[test]
fn test_chunking_logic() {
    // You can test public functions, but here chunking is internal
    // So this would require a unit test unless you exposed it publicly
    let client = Client::new(None, 10);
    let data = b"Some large data to test chunking!";
    let chunks = client.send_data(data);

    // Check if sending large data chunking succeeds
    assert!(chunks.is_ok());
}