#[allow(dead_code)]
#[path = "../mailbox_client_association_api.rs"]
mod mailbox_client_association_api;

use mailbox_client_association_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
