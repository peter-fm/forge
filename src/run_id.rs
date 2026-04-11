use crate::error::ForgeError;
use std::fs::File;
use std::io::Read;

pub fn generate_run_id(blueprint: &str) -> Result<String, ForgeError> {
    let mut bytes = [0_u8; 2];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(format!(
        "{}-{:02x}{:02x}",
        slugify_blueprint(blueprint),
        bytes[0],
        bytes[1]
    ))
}

pub fn session_uuid(seed: &str) -> String {
    let mut bytes = [0_u8; 16];
    let first = fnv1a64(seed.as_bytes());
    let second = fnv1a64(format!("{seed}:session").as_bytes());
    bytes[..8].copy_from_slice(&first.to_be_bytes());
    bytes[8..].copy_from_slice(&second.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut state = 0xcbf29ce484222325_u64;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100000001b3);
    }
    state
}

fn slugify_blueprint(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        "run".to_string()
    } else {
        output
    }
}
