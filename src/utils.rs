use tokio::{fs::File, io::AsyncReadExt, io::AsyncWriteExt};
use steganography::encoder::*;
use steganography::decoder::*;
use steganography::util::*;
use base64;


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
pub async fn server_decrypt_img(base_img_path: &str, output_hidden_img_path: &str) {
    // Extract the hidden message (base64-encoded image)
    let encoded_message = read_from_img(base_img_path).await;
    print!("encoded_message: {}", encoded_message);
    // Decode the base64-encoded message back to image bytes
    let decoded_img_bytes = base64::decode(&encoded_message).expect("Failed to decode base64");

    // Write the decoded image bytes to a new image file
    let mut output_file = File::create(output_hidden_img_path).await.expect("Failed to create output image file");
    output_file.write_all(&decoded_img_bytes).await.expect("Failed to write decoded image");
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