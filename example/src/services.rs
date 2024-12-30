use std::{any::Any, collections::HashMap};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io;
use steganography::encoder::*;
use steganography::util::*;
use async_trait::async_trait;

use distri::service::Service;

pub struct EncryptService;

#[async_trait]
impl Service for EncryptService {
    fn name(&self) -> &'static str {
        "Encrypt"
    }

    fn serialize_request(&self, data: Box<dyn Any + Send>) -> Result<Vec<u8>, std::io::Error> {
        // Assume the input is a Vec<u8>
        Ok(*data.downcast::<Vec<u8>>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Failed to downcast request data"))?)
    }

    fn deserialize_request(&self, data: Vec<u8>) -> Result<Box<dyn Any + Send>, std::io::Error> {
        // Simply wrap the Vec<u8> into a Box
        Ok(Box::new(data))
    }

    fn serialize_response(&self, data: Box<dyn Any + Send>) -> Result<Vec<u8>, std::io::Error> {
        // Assume the response is Vec<u8>, no additional serialization needed
        Ok(*data.downcast::<Vec<u8>>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Failed to downcast response data"))?)
    }

    fn deserialize_response(&self, data: Vec<u8>) -> Result<Box<dyn Any + Send>, std::io::Error> {
        // Simply wrap the Vec<u8> into a Box
        Ok(Box::new(data))
    }

    async fn process(
        &self,
        _args: HashMap<String, String>,
        data: Option<Box<dyn Any + Send>>,
    ) -> Result<Box<dyn Any + Send>, std::io::Error> {
        // Ensure `data` is provided and downcast to Vec<u8>
        let data = data
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "No input data provided"))?
            .downcast::<Vec<u8>>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Failed to downcast data"))?;

        let img_path = "files/to_encrypt.jpg";
        let output_path = "files/encrypted_output.png";

        // Write data bytes to the file if it doesn't exist
        if !Path::new(img_path).exists() {
            let mut file = File::create(img_path).await?;
            file.write_all(&data).await?;
        }

        // Perform encryption on the file
        server_encrypt_img("files/placeholder.jpg", img_path, output_path).await;

        // Read the encrypted output file as bytes
        let mut encrypted_file = File::open(output_path).await?;
        let mut encrypted_data = Vec::new();
        encrypted_file.read_to_end(&mut encrypted_data).await?;

        // Return the encrypted data
        Ok(Box::new(encrypted_data))
    }
}

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

async fn write_to_img(message: &str, img_path: &str, output_path: &str) {
    let binding = message.to_string();
    let payload = str_to_bytes(&binding);
    let destination_image = file_as_dynamic_image(img_path.to_string());
    let encoder = Encoder::new(payload, destination_image);
    let result = encoder.encode_alpha();
    save_image_buffer(result, output_path.to_string());
}
