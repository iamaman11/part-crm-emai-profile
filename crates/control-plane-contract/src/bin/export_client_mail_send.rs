#[allow(dead_code)]
#[path = "../client_mail_send_api.rs"]
mod client_mail_send_api;

use client_mail_send_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
