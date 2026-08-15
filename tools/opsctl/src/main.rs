#![forbid(unsafe_code)]

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let invocation = opsctl::parse_invocation(std::env::args_os())?;
    let output = opsctl::execute(invocation)?;
    print!("{output}");
    Ok(())
}
