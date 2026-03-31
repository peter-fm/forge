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
