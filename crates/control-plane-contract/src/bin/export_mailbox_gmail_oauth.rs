#[allow(dead_code)]
#[path = "../mailbox_gmail_oauth_api.rs"]
mod mailbox_gmail_oauth_api;

use mailbox_gmail_oauth_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
