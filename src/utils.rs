use tokio::{fs::File, io::AsyncReadExt, io::AsyncWriteExt};
use steganography::encoder::*;
use steganography::decoder::*;
use steganography::util::*;
use base64;
use std::error::Error;
use tokio::net::UdpSocket;
use tokio::time::{sleep, timeout};
use std::net::SocketAddr;
use std::io;
use std::collections::HashSet;
use std::time::Duration;



pub const END_OF_TRANSMISSION: &str = "END_OF_TRANSMISSION";
pub const DEFAULT_TIMEOUT: u64 = 10; // in seconds
pub const RETRY_INTERVAL: u64 = 1; // in seconds
pub const ACK: &[u8] = b"ACK";


// Server encrypt: hides second image inside the first image
pub async fn server_encrypt_img(base_img_path: &str, img_to_hide_path: &str, output_path: &str) {
    // Read the second image (the one to hide) as bytes
    let mut img_to_hide_file = File::open(img_to_hide_path).await.expect("Failed to open image to hide");
    let mut img_to_hide_bytes = Vec::new();
    img_to_hide_file.read_to_end(&mut img_to_hide_bytes).await.expect("Failed to read image");

    // Encode the image bytes in base64
    let encoded_img_to_hide = base64::encode(&img_to_hide_bytes);

    // Hide the base64-encoded image inside the base image
    write_to_img(&encoded_img_to_hide, base_img_path, output_path).await;
}

// Server decrypt: extracts hidden image and saves it
pub async fn server_decrypt_img(base_img_path: &str, output_hidden_img_path: &str) -> Result<(), Box<dyn Error + 'static>> {
    // Extract the hidden message (base64-encoded image)
    let encoded_message = read_from_img(base_img_path).await;
    // print!("encoded_message: {}", encoded_message);
    // Decode the base64-encoded message back to image bytes
    let decoded_img_bytes = match base64::decode(&encoded_message) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to decode base64: {}", e);
            return Err(Box::new(e));  // use a simple error here
        }
    };
    // Write the decoded image bytes to a new image file
    let mut output_file = match File::create(output_hidden_img_path).await {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to create output image file: {}", e);
            return Err(Box::new(e));  // Propagate the error
        }
    };
    output_file.write_all(&decoded_img_bytes).await.expect("Failed to write decoded image");
    Ok(())
}

async fn write_to_img(message: &str, img_path: &str, output_path: &str) {
    let binding = message.to_string();
    let payload = str_to_bytes(&binding);
    let destination_image = file_as_dynamic_image(img_path.to_string());
    let encoder = Encoder::new(payload, destination_image);
    let result = encoder.encode_alpha();
    save_image_buffer(result, output_path.to_string());
}

async fn read_from_img(img_path: &str) -> String {
    let encoded_image = file_as_image_buffer(img_path.to_string());
    let decoder = Decoder::new(encoded_image);
    let out_buffer = decoder.decode_alpha();
    let clean_buffer: Vec<u8> = out_buffer.into_iter()
                                    .filter(|b| {
                                        *b != 0xff_u8
                                    })
                                    .collect();
    let message = bytes_to_str(clean_buffer.as_slice());
    // println!("{:?}", message);
    message.to_string()
}


/// Sends a message to a specified address with a specified number of retry attempts.
/// 
/// # Arguments
/// * `socket` - The UDP socket to use for sending the message.
/// * `message` - The message to send as a byte slice.
/// * `addr` - The destination address.
/// * `max_retries` - Maximum number of retries on failure.
/// 
/// # Returns
/// * `Result<(), std::io::Error>` - Returns `Ok(())` on success, or an error after all retries fail.
pub async fn send_with_retry(
    socket: &UdpSocket,
    message: &[u8],
    addr: SocketAddr,
    packet_id: u64,
    max_retries: u8,
) -> Result<(), io::Error> {
    let mut attempts = 0;
    let mut ack_buffer = [0u8; 16]; // Buffer for receiving ACKs

    while attempts < max_retries {
        attempts += 1;

        // Prepare the packet with the packet ID
        let mut packet = packet_id.to_be_bytes().to_vec();
        packet.extend_from_slice(message);

        match socket.send_to(&packet, addr).await {
            Ok(_) => {
                // Wait for an acknowledgment within a timeout
                match timeout(Duration::from_millis(500), socket.recv_from(&mut ack_buffer)).await {
                    Ok(Ok((size, _))) if &ack_buffer[..size] == ACK => {
                        // ACK received, exit function successfully
                        return Ok(());
                    },
                    _ => {
                        // No ACK received within timeout
                        eprintln!("Attempt {}/{}: ACK not received, retrying...", attempts, max_retries);
                    }
                }
            },
            Err(e) => {
                eprintln!("Failed to send message to {}: {:?} (attempt {}/{})", addr, e, attempts, max_retries);
            },
        }

        if attempts < max_retries {
            // Wait a bit before retrying
            sleep(Duration::from_millis(100)).await;
        } else {
            // Exhausted retries, return an error
            return Err(io::Error::new(io::ErrorKind::Other, "Failed to send message after retries"));
        }
    }
    Err(io::Error::new(io::ErrorKind::Other, "Failed to send message after retries"))
}



pub async fn recv_with_ack(
    socket: &UdpSocket,
    buffer: &mut [u8],
    processed_ids: &mut HashSet<u64>, // Track processed packet IDs to avoid duplicates
) -> Result<(usize, SocketAddr, u64), io::Error> {
    // Receive the packet with a timeout
    let (size, addr) = socket.recv_from(buffer).await?;
    if size < 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Packet too small"));
    }

    // Extract packet ID from the packet
    let packet_id = u64::from_be_bytes(buffer[..8].try_into().unwrap());

    // Check if the packet ID has already been processed
    if processed_ids.contains(&packet_id) {
        // If duplicate, still send an ACK but skip processing
        socket.send_to(ACK, addr).await?;
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Duplicate packet"));
    }

    // Mark packet ID as processed and send an ACK
    processed_ids.insert(packet_id);
    socket.send_to(ACK, addr).await?;

    // Return the packet data for processing
    Ok((size - 8, addr, packet_id))
}