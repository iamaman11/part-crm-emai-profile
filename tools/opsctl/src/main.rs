use std::env;

fn main() {
    match opsctl::parse_invocation(env::args_os()).and_then(opsctl::execute) {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprint!("{}", error.json());
            std::process::exit(2);
        }
    }
}
