#[path = "../frontend_transport.rs"]
mod frontend_transport;

use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let document = serde_json::from_str(&input)?;
    let closed = frontend_transport::close_compiler_input(document)?;
    let mut rendered = serde_json::to_string_pretty(&closed)?;
    rendered.push('\n');
    print!("{rendered}");
    Ok(())
}
