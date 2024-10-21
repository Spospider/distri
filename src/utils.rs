use std::collections::HashMap;
use sha2::{Sha256, Digest}; // For checksum verification

/// Splits the data into smaller chunks and adds a sequence number and checksum
pub fn chunk_data(data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
    let total_chunks = (data.len() + chunk_size - 1) / chunk_size;
    let mut chunked_data = Vec::new();
    
    let mut sequence_number:u32 = 0;
    for chunk in data.chunks(chunk_size) {
        let mut packet = Vec::new();
        
        // Add sequence number as the first 4 bytes (u32)
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        
        // Add the actual data chunk
        packet.extend_from_slice(chunk);
        
        // Calculate checksum for the chunk (SHA-256)
        let mut hasher = Sha256::new();
        hasher.update(chunk);
        let checksum = hasher.finalize();
        packet.extend_from_slice(&checksum[..8]);  // Add only first 8 bytes of the checksum
        
        chunked_data.push(packet);

        // Increment seq number
        sequence_number = (sequence_number+1) % u32::MAX;

    }    
    chunked_data
}

/// Reconstructs the data from received chunks and verifies checksum
pub fn reconstruct_data(chunks: Vec<Vec<u8>>, chunk_size: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut groups: HashMap<u32, Vec<Vec<u8>>> = HashMap::new();
    let mut current_group: u32 = 0;
    let mut last_seq_num: u32 = 0;

    // Reordering in case of sequence numbers and possible wrap-around
    for chunk in chunks {
        let sequence_number = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);

        // If we detect that the sequence number has wrapped around
        if last_seq_num - sequence_number > u32::MAX - 100 {
            current_group += 1;
        }
        last_seq_num = sequence_number;

        // Group chunks by their wrap-around group
        groups.entry(current_group)
            .or_insert_with(Vec::new)
            .push(chunk);
    }
    
    let mut sorted_chunks:Vec<Vec<u8>> = Vec::new();

    for group in 0..=current_group {
        let mut sorted_group = groups.get(&group).unwrap().clone();

        // Sort chunks within the group by sequence number
        sorted_group.sort_by_key(|chunk| {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        });

        // Append the sorted group chunks to the sorted_chunks vector
        sorted_chunks.append(&mut sorted_group.clone()); // Note the use of `&mut`
    }

    // Assembling the data and removing overhead.
    let mut assembled_data = Vec::new();
    
    // Reassemble data and verify checksum
    for chunk in sorted_chunks {
        let sequence_number = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        
        // Extract the data and checksum
        let data_chunk = &chunk[4..(chunk.len() - 8)];
        let received_checksum = &chunk[(chunk.len() - 8)..];

        // Verify checksum
        let mut hasher = Sha256::new();
        hasher.update(data_chunk);
        let calculated_checksum = hasher.finalize();
        if &calculated_checksum[..8] != received_checksum {
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "Checksum mismatch")));
        }

        // Append valid data chunk
        assembled_data.extend_from_slice(data_chunk);
    }

    Ok(assembled_data)
}