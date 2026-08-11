#[path = "../mailbox_api.rs"]
mod mailbox_api;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        serde_json::to_string(&mailbox_api::openapi_fragment())?
    );
    Ok(())
}
