use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;

async fn send_file(file_path: &Path, server_addr: &str) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(server_addr).await?;

    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len() as usize;
    let file_name = file_path.file_name().unwrap().to_string_lossy();
    let metadata = format!("{}|{}\n", file_name, file_size);  // '\n' ensures end of metadata
    stream.write_all(metadata.as_bytes()).await?;

    let mut buffer = [0; 1024];
    let mut sent_size = 0;
    while sent_size < file_size {
        let n = file.read(&mut buffer)?;
        if n == 0 { break; }  //eof
        stream.write_all(&buffer[..n]).await?;
        sent_size += n;
    }

    println!("File {} sent successfully!", file_name);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let file_path = Path::new("photo.png");
    let server_addr = "127.0.0.1:8080";

    // Send the file
    send_file(file_path, server_addr).await?;
    Ok(())
}



/// server





use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader}; //, AsyncWriteExt
use std::error::Error;
use std::fs::File;
use std::path::PathBuf; //, Path
use std::io::Write;
use std::time::SystemTime;

async fn handle_connection(stream: TcpStream) -> Result<(), Box<dyn Error>> {

    let mut reader = BufReader::new(stream);
    let mut metadata_buf = Vec::new();
    let n = reader.read_until(b'\n', &mut metadata_buf).await?;
    let metadata = String::from_utf8_lossy(&metadata_buf[..n]);

    let parts: Vec<&str> = metadata.trim_end().split('|').collect();
    if parts.len() != 2 {
        return Err("Invalid metadata format".into());
    }
    let file_name = parts[0].trim();
    let file_size: usize = parts[1].trim().parse().map_err(|_| "Invalid file size")?;

    let new_file_name = format!("received_{}.jpg", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs());
    let new_file_path = PathBuf::from("received_files").join(&new_file_name);

    std::fs::create_dir_all(new_file_path.parent().unwrap())?;

    let mut file = File::create(&new_file_path)?;
    let mut received_size = 0;
    let mut buffer = [0; 1024];
    while received_size < file_size {
        let n = reader.read(&mut buffer).await?;
        if n == 0 { break; }  //eos
        file.write_all(&buffer[..n])?;
        received_size += n;
    }

    println!("File {} received successfully and saved to {}", file_name, new_file_path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on port 8080...");

    // Accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}
