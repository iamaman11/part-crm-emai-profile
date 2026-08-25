#[path = "../frontend_transport.rs"]
mod frontend_transport;

use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(argument) = std::env::args().nth(1) {
        return Err(format!("unknown export_frontend_openapi argument: {argument}").into());
    }

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let value: serde_json::Value = serde_json::from_str(&input)?;

    frontend_transport::validate_compiler_input(&value)?;

    let mut rendered = serde_json::to_string_pretty(&value)?;
    rendered.push('\n');
    print!("{rendered}");
    Ok(())
}
