#[path = "../frontend_transport.rs"]
mod frontend_transport;

use serde_json::json;
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "openapi".to_owned());
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let value = serde_json::from_str(&input)?;

    match mode.as_str() {
        "openapi" => {
            let closed = frontend_transport::close_compiler_input(value)?;
            let mut rendered = serde_json::to_string_pretty(&closed)?;
            rendered.push('\n');
            print!("{rendered}");
        }
        "--digest-material" => {
            let material = frontend_transport::canonical_digest_material(&value)?;
            let material_utf8 = String::from_utf8(material)?;
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "algorithm": "sha-256",
                    "canonicalUtf8": material_utf8,
                    "encoding": "lowercase-hex"
                }))?
            );
        }
        _ => return Err(format!("unknown export_frontend_openapi mode: {mode}").into()),
    }
    Ok(())
}
