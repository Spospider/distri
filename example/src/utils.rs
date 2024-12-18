use tokio::{fs::File, io::AsyncWriteExt};
use base64;
use std::error::Error;
use steganography::decoder::*;
use steganography::util::*;

use image::GenericImageView;


use std::thread;
use std::time::Duration;

use show_image::*;


// Helper functions

async fn read_from_img(img_path: &str) -> Result<String, Box<dyn Error + 'static>> {
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
    Ok(message.to_string())
}


// Server decrypt: extracts hidden image and saves it
pub async fn server_decrypt_img(base_img_path: &str, output_hidden_img_path: &str) -> Result<(), Box<dyn Error + 'static>> {
    // Extract the hidden message (base64-encoded image)
    let encoded_message = read_from_img(base_img_path).await?;
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


// peer decrypt: gets raw data of an encrypted image extracts the hidden image and returns it as bytes following the logic of read_from_img but using the raw image istead of the image path
pub async fn peer_decrypt_img(encoded_img: &Vec<u8>) -> Result<Vec<u8>, Box<dyn Error + 'static>> {
    /* let decoder = Decoder::new(encoded_image);
    let out_buffer = decoder.decode_alpha();
    let clean_buffer: Vec<u8> = out_buffer.into_iter()
                                    .filter(|b| {
                                        *b != 0xff_u8
                                    })
                                    .collect();
    let message = bytes_to_str(clean_buffer.as_slice());
    // println!("{:?}", message);
    message.to_string() */
    
    // Extract the hidden message (base64-encoded image)
    let encoded_message = bytes_to_str(encoded_img.as_slice());
    // print!("encoded_message: {}", encoded_message);
    // Decode the base64-encoded message back to image bytes
    let decoded_img_bytes = match base64::decode(&encoded_message) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to decode base64: {}", e);
            return Err(Box::new(e));  // use a simple error here
        }
    };
    Ok(decoded_img_bytes)
}





pub async fn decrypt_image(input_path: &str, output_path: &str) -> Result<(), std::io::Error> {
    match server_decrypt_img(input_path, output_path).await {
        Ok(_) => {
            println!("Decrypted image saved to '{}'.", output_path);
            Ok(())
        }
        Err(e) => {
            eprintln!("Decryption failed for '{}': {}", input_path, e);
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Decryption failed"))
        }
    }
}

pub async fn write_to_file(file_path: &str, data: &[u8]) -> Result<(), std::io::Error> {
    return Ok(());
    match File::create(file_path).await {
        Ok(mut file) => {
            file.write_all(data).await?;
            println!("File saved to '{}'.", file_path);
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to create file '{}': {}", file_path, e);
            Err(e)
        }
    }
}

pub fn show_image(image_data: Vec<u8>) -> Result<(), Box<dyn Error>> {
    // Decode the image data
    let img = image::load_from_memory(&image_data)?;
    let img = img.to_rgba8(); // Convert to RGBA format
    let (width, height) = img.dimensions();

    // Convert the image to a flat vector of u8 pixel data
    let pixel_data: Vec<u8> = img.into_raw();

    // Create an ImageView with the loaded image data
    let image = ImageView::new(ImageInfo::rgba8(width, height), &pixel_data);

    // Create a window with default options and display the image
    let window = create_window("image", Default::default()).unwrap();
    window.set_image("image-001", image);

    thread::sleep(Duration::from_secs(2));
    
    Ok(())
}