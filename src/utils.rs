use tokio::{fs::File, io::AsyncReadExt, io::AsyncWriteExt};
use steganography::encoder::*;
use steganography::decoder::*;
use steganography::util::*;
use base64;
use std::error::Error;
use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration};
use std::net::SocketAddr;


pub const END_OF_TRANSMISSION: &str = "END_OF_TRANSMISSION";


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
pub async fn send_with_retry(socket: &UdpSocket, message: &[u8], addr: SocketAddr, max_retries: u8) -> Result<(), std::io::Error> {
    let mut attempts = 0;
    while attempts < max_retries {
        attempts += 1;
        
        match socket.send_to(message, addr).await {
            Ok(_) => {
                // Successfully sent the message
                // println!("Successfully sent message to {} on attempt {}", addr, attempts);
                return Ok(());
            },
            Err(e) => {
                eprintln!("Failed to send message to {}: {:?} (attempt {}/{})", addr, e, attempts, max_retries);
                if attempts < max_retries {
                    // Wait a bit before retrying
                    sleep(Duration::from_millis(100)).await;
                } else {
                    // Exhausted retries, return the error
                    return Err(e);
                }
            },
        }
    }
    // If all retries are exhausted, return an error (this should be unreachable)
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send message after retries"))
}