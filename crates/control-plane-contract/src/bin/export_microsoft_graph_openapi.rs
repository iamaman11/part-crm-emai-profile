#[allow(dead_code)]
#[path = "../mailbox_microsoft_graph_onboarding_api.rs"]
mod mailbox_microsoft_graph_onboarding_api;

use mailbox_microsoft_graph_onboarding_api::openapi_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&openapi_fragment())?);
    Ok(())
}
