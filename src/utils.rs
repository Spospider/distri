use serde::{Serialize, Deserialize};
use serde_json;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use base64::{encode, decode};
use std::error::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct FileData {
    pub file_name: String,
    pub file_content: String,  // Base64 encoded file content
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataPacket {
    pub message: String,
    pub file: Option<FileData>,  // Optional file object
}

/// Serialize the data into JSON with optional file contents.
pub fn serialize_data(message: &str, file_path: Option<&Path>) -> Result<String, Box<dyn Error>> {
    let file_data = if let Some(path) = file_path {
        // Read the file and encode as base64
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let encoded_file = encode(&buffer);  // Base64 encode the file content

        Some(FileData {
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            file_content: encoded_file,
        })
    } else {
        None
    };

    let data_packet = DataPacket {
        message: message.to_string(),
        file: file_data,
    };

    // Serialize the entire DataPacket to a JSON string
    let serialized = serde_json::to_string(&data_packet)?;
    Ok(serialized)
}

/// Deserialize the JSON data back into a DataPacket struct.
pub fn deserialize_data(json_data: &str) -> Result<DataPacket, Box<dyn Error>> {
    let deserialized: DataPacket = serde_json::from_str(json_data)?;
    Ok(deserialized)
}