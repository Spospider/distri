use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration, timeout};
use std::net::SocketAddr;
use std::collections::HashMap;
use std::io;



/// Constants
pub const END_OF_TRANSMISSION: &str = "END_OF_TRANSMISSION";
pub const DEFAULT_TIMEOUT: u64 = 15; // in seconds
pub const RETRY_INTERVAL:Duration = Duration::from_millis(1000); // in seconds
pub const CHUNK_SIZE: usize = 1024; // in seconds
pub const MAX_RETRIES: u8 = 5;
pub const FINAL_MSG:&str = "CHUNKS_END";



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
                return Ok(());
            },
            Err(e) => {
                eprintln!("Failed to send message to {}: {:?} (attempt {}/{})", addr, e, attempts, max_retries);
                if attempts < max_retries {
                    // Wait a bit before retrying
                    sleep(RETRY_INTERVAL).await;
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


pub async fn recv_with_timeout(
    socket: &UdpSocket,
    buffer: &mut [u8],
    timeout_duration: Duration,
) -> Result<(usize, SocketAddr), io::Error> {
    // Use `timeout` to limit the time we wait for `recv_from`
    match timeout(timeout_duration, socket.recv_from(buffer)).await {
        Ok(Ok((size, addr))) => Ok((size, addr)), // Successfully received data within timeout
        Ok(Err(e)) => Err(e),                      // Error from `recv_from`
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "recv_from timed out")), // Timeout error
    }
}


pub async fn send_reliable(
    socket: &UdpSocket,
    data: &[u8],
    addr: SocketAddr,
) -> Result<(), io::Error> {
    let mut sequence_number = 0u64;
    let mut chunk_start = 0;
    

    while chunk_start < data.len() {
        // Determine the end of the current chunk
        let chunk_end = (chunk_start + CHUNK_SIZE).min(data.len());
        let chunk = &data[chunk_start..chunk_end];

        // Serialize sequence number and append it to the chunk
        let mut packet = Vec::with_capacity(8 + chunk.len());
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(chunk);

        // Send the chunk with retries and wait for an ACK
        let mut attempts = 0;
        let mut ack_received = false;

        while attempts < MAX_RETRIES && !ack_received {
            // Send the chunk with sequence number
            send_with_retry(socket, &packet, addr, MAX_RETRIES).await?;
            attempts += 1;

            // Wait for an ACK with a timeout
            let mut ack_buffer = [0u8; 8];
            match recv_with_timeout(socket, &mut ack_buffer, Duration::from_secs(DEFAULT_TIMEOUT)).await {
                Ok((size, src)) if size == 8 && src == addr => {
                    // Verify that the ACK contains the correct sequence number
                    let received_sequence_number = u64::from_be_bytes(ack_buffer);
                    if received_sequence_number == sequence_number {
                        ack_received = true;
                        // println!("Received ACK for sequence number {}", sequence_number);
                    }
                }
                Ok(_) => {
                    // Received a packet, but it's not the correct ACK or from an unexpected source, skip
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    if attempts < MAX_RETRIES {
                        // Wait a bit before retrying
                        sleep(RETRY_INTERVAL).await;
                    } else {
                        // Exhausted retries, break out
                        break;
                    } 
                    eprintln!("Timeout waiting for ACK for sequence number {}", sequence_number);
                }
                Err(e) => {
                    // Some other I/O error occurred
                    return Err(e);
                }
            }
        }

        if !ack_received {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to receive ACK after retries",
            ));
        }

        // Move to the next chunk
        sequence_number += 1;
        chunk_start = chunk_end;
    }
    
    let mut packet = Vec::with_capacity(8+FINAL_MSG.len());
    packet.extend_from_slice(&sequence_number.to_be_bytes());
    packet.extend_from_slice(FINAL_MSG.as_bytes());
    // println!("Sent Final Msg, {:?}", packet);

    send_with_retry(socket, &packet, addr, MAX_RETRIES).await?;

    Ok(())
}


pub async fn recv_reliable(socket: &UdpSocket, duration:Option<Duration>) -> Result<(Vec<u8>, usize, SocketAddr), io::Error> {
    let mut received_data: HashMap<u64, Vec<u8>> = HashMap::new(); // Store received chunks by sequence number
    let mut expected_sequence_number = 0u64;
    let mut address:SocketAddr;
    // println!("Start recv chunks");

    loop {
        let mut buffer = vec![0u8; CHUNK_SIZE + 8]; // Buffer to receive chunk + sequence number
        // Receive a chunk
        let (size, addr) = recv_with_timeout(&socket, &mut buffer, duration.unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT))).await?;
        // if address != addr {
        //     continue;
        // }
        address = addr.clone();
        
        // Check if the chunk contains at least the sequence number (8 bytes)
        if size <= 8 {
            // eprintln!("Received packet too small to contain a sequence number");
            continue; // Ignore incomplete packet
        }

        // Extract sequence number and chunk data
        let received_sequence_number = u64::from_be_bytes(buffer[..8].try_into().unwrap());
        let chunk = &buffer[8..size]; // Extract the actual data

        // Acknowledge receipt of this sequence number
        let ack = received_sequence_number.to_be_bytes();
        // println!("ACKed {:?} to {}", received_sequence_number, addr);
        socket.send_to(&ack, addr).await?;

        // If this chunk is what we expected, store it and update the expected sequence number
        if received_sequence_number == expected_sequence_number {
            received_data.insert(received_sequence_number, chunk.to_vec());
            // println!("recv {} == expected {}", received_sequence_number, expected_sequence_number);

            // Check if this is the last chunk (indicated by empty data)
            // if chunk.len() < 20 {
            //     println!("Chunk received {:?}", chunk);
            // }
            if chunk == FINAL_MSG.as_bytes() {
                break;
            }
            expected_sequence_number += 1;

        } else {
            // println!("out of order");
            // println!("recv {} != expected {}", received_sequence_number, expected_sequence_number);
            // Handle out-of-order packets by storing them for future use
            received_data.entry(received_sequence_number).or_insert_with(|| chunk.to_vec());
        }
    }

    // Reassemble the data in the correct order
    let mut complete_data = Vec::new();
    for seq_num in 0..expected_sequence_number {
        if let Some(chunk) = received_data.remove(&seq_num) {
            complete_data.extend(chunk);
        } else {
            return Err(io::Error::new(io::ErrorKind::Other, "Missing data chunk"));
        }
    }
    let f_size = complete_data.len();
    // println!("end recv chunks");

    Ok((complete_data, f_size, address))
}