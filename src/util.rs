// use anyhow::Result;
use sha2::{Digest, Sha256}; // docker digest uses sha256
use std::error::Error;
use std::{fs::File, io::Read};

pub fn hash_buffer(buffer: &str) -> Result<String, Box<dyn Error>> {
    let hash = format!("{:X}", Sha256::digest(buffer));
    Ok(hash)
}

fn hash_file(file: &str) -> Result<String, Box<dyn Error>> {
    // https://www.thorsten-hans.com/weekly-rust-trivia-compute-a-sha256-hash-of-a-file/
    // Open the file
    let mut file = File::open(file)?;

    // Create a SHA-256 "hasher"
    let mut hasher = Sha256::new();

    // Read the file in 4KB chunks and feed them to the hasher
    let mut buffer = [0; 4096];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // Finalize the hash and get the result as a byte array
    Ok(format!("{:x}", hasher.finalize()))
}
