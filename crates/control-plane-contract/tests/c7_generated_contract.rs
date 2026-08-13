#[path = "../src/client_mail_send_api.rs"]
mod client_mail_send_api;

use client_mail_send_api::openapi_fragment;
use serde_json::Value;

const COMMITTED_OPENAPI: &str =
    include_str!("../../../contracts/generated/client-mail-send.openapi.json");

#[test]
fn committed_c7_openapi_matches_canonical_rust() -> Result<(), Box<dyn std::error::Error>> {
    let committed = serde_json::from_str::<Value>(COMMITTED_OPENAPI)?;
    assert_eq!(committed, openapi_fragment());
    Ok(())
}
