#[allow(dead_code)]
#[path = "../standards_mailbox_onboarding_api.rs"]
mod standards_mailbox_onboarding_api;

use standards_mailbox_onboarding_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
