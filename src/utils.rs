use tokio::{fs::File, io::AsyncReadExt, io::AsyncWriteExt};
use steganography::encoder::*;
use steganography::decoder::*;
use steganography::util::*;
use base64;
use std::error::Error;
use std::io::Write;
use std::net::SocketAddr;
use std::io;
use std::collections::HashMap;
use regex::Regex;
use tempfile::Builder;
use std::fs::File as __File;
use serde_json::{json, to_string};
use serde::Serialize;



/// Data structures
#[derive(Clone)]
pub struct NodeInfo {
    pub load: i32,
    pub id: u16,
    pub addr: SocketAddr,
    pub db_version: u64,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum DistriError {
    // Operational error - issues with processing the request
    #[error("Operational error: {0}")]
    OperationalError(String),

    // Validation error - invalid or missing inputs
    #[error("Validation error: {0}")]
    ValidationError(String),

    // Network error - network issues such as timeouts or connectivity issues
    #[error("Network error: {0}")]
    NetworkError(String),

    // Orchestration error - issues in orchestrating the cloud nodes
    #[error("Orchestration error: {0}")]
    OrchestrationError(String),
}
impl DistriError {
    // Method to convert the error into a JSON string
    pub fn to_json(&self) -> String {
        match self {
            DistriError::OperationalError(msg) => {
                to_string(&json!({"ErrorType": "OperationalError", "Error": msg})).unwrap()
            }
            DistriError::ValidationError(msg) => {
                to_string(&json!({"ErrorType": "ValidationError", "Error": msg})).unwrap()
            }
            DistriError::NetworkError(msg) => {
                to_string(&json!({"ErrorType": "NetworkError", "Error": msg})).unwrap()
            }
            DistriError::OrchestrationError(msg) => {
                to_string(&json!({"ErrorType": "OrchestrationError", "Error": msg})).unwrap()
            }
        }
    }
}
impl From<std::io::Error> for DistriError {
    fn from(err: std::io::Error) -> Self {
        DistriError::OperationalError(err.to_string())
    }
}
pub enum DBOp {
    READ,
    ADD,
    UPDATE,
    DELETE,
    CREATECOLLECTION,
    DELETECOLLECTION
}
impl DBOp {
    pub fn to_string(&self) -> &str {
        match self {
            DBOp::READ => "READ",
            DBOp::ADD => "ADD",
            DBOp::UPDATE => "UPDATE",
            DBOp::DELETE => "DELETE",
            DBOp::CREATECOLLECTION => "CREATECOLLECTION",
            DBOp::DELETECOLLECTION => "DELETECOLLECTION",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "READ" => Some(DBOp::READ),
            "ADD" => Some(DBOp::ADD),
            "UPDATE" => Some(DBOp::UPDATE),
            "DELETE" => Some(DBOp::DELETE),
            "CREATECOLLECTION" => Some(DBOp::CREATECOLLECTION),
            "DELETECOLLECTION" => Some(DBOp::DELETECOLLECTION),
            _ => None,
        }
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
pub async fn write_to_file(file_path: &str, data: &[u8]) -> Result<(), std::io::Error> {
    // return Ok(());
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

// peer decrypt: gets raw data of an encrypted image extracts the hidden image and returns it as bytes following the logic of read_from_img but using the raw image istead of the image path
pub async fn peer_decrypt_img(encoded_img: &Vec<u8>) -> Result<Vec<u8>, Box<dyn Error + 'static>> {
    let  mut temp_file = Builder::new()
        .suffix(".png") // Specify the .jpg extension
        .tempfile()?;
    // Write the bytes to the temporary file
    let img_path = temp_file.path().to_path_buf(); // Get the file path
    {
        let mut file = __File::create(&img_path)?;
        file.write_all(encoded_img)?;
    }
    // let path = Path::new("resources/tmp.png");
    // write_to_file("resources/tmp.png", encoded_img).await?;
    // temp_file.write_all(encoded_img).await?;
    // let img_path = img_path;
    let encoded_image = file_as_image_buffer(img_path.to_string_lossy().to_string());

    let decoder = Decoder::new(encoded_image);
    let out_buffer = decoder.decode_alpha();
    let clean_buffer: Vec<u8> = out_buffer.into_iter()
                                    .filter(|b| {
                                        *b != 0xff_u8
                                    })
                                    .collect();
    let message = bytes_to_str(clean_buffer.as_slice());
    // println!("{:?}", message);
    // message.to_string() 
    
    // Extract the hidden message (base64-encoded image)
    // let encoded_message = bytes_to_str(encoded_img.as_slice());
    let encoded_message = message.to_string();
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


pub fn extract_args(input: &str) -> Result<HashMap<String, String>, io::Error> {
    /// Extracts string args passed in requests in the form: <key:value,key2:value2,...>
    // Define a regex pattern to match key-value pairs inside <key:value,key2:value2,...>
    let re = Regex::new(r"<([^>]+)>").unwrap(); // This will match anything between < and >

    // Apply the regex and capture the key-value pairs
    if let Some(captures) = re.captures(input) {
        // Capture the string inside the <...>
        let args_str = &captures[1];
        
        // Initialize a HashMap to store the key-value pairs
        let mut args_map = HashMap::new();

        // Split the captured string by commas to separate key-value pairs
        for pair in args_str.split(',') {
            // Split each key-value pair by colon
            let mut key_value = pair.splitn(2, ':'); // Split only once at the first colon
            if let (Some(key), Some(value)) = (key_value.next(), key_value.next()) {
                args_map.insert(key.trim().to_string(), value.trim().to_string());
            } else {
                // If there's an invalid pair (missing colon or value), return an error
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid key-value pair"));
            }
        }
        Ok(args_map)
    } else {
        // return empty string-string hashmap 
        Ok(HashMap::new())
        // Err(io::Error::new(io::ErrorKind::Other, "No arguments found"))
    }
}


// A helper function to serialize `Result` into JSON response
pub fn generate_response<T>(result: Result<T, DistriError>) -> String
where
    T: Serialize,
{
    match result {
        Ok(value) => {
            // If the operation was successful, serialize the result into the "result" field
            to_string(&json!({"result": value})).unwrap()
        }
        Err(e) => {
            // If an error occurred, serialize the error message
            e.to_json()
        }
    }
}
