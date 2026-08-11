#[path = "../query_mail_api.rs"]
mod query_mail_api;

use query_mail_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
