fn main() {
    match opsctl::release::capability_policy_manifest::render_json() {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprintln!("capability policy manifest rendering failed: {error}");
            std::process::exit(2);
        }
    }
}
