#[allow(dead_code)]
#[path = "../src/mailbox_gmail_oauth_api.rs"]
mod mailbox_gmail_oauth_api;
#[allow(dead_code)]
#[path = "../src/mailbox_microsoft_graph_onboarding_api.rs"]
mod mailbox_microsoft_graph_onboarding_api;
#[allow(dead_code)]
#[path = "../src/operator_query_api.rs"]
mod operator_query_api;
#[allow(dead_code)]
#[path = "../src/profile_relationship_api.rs"]
mod profile_relationship_api;
#[allow(dead_code)]
#[path = "../src/standards_mailbox_onboarding_api.rs"]
mod standards_mailbox_onboarding_api;

fn assert_fragment(generated: serde_json::Value, accepted: &str) {
    assert_eq!(generated.to_string(), accepted.trim_end());
}

#[test]
fn rust_owned_capability_openapi_fragments_match_accepted_projections_byte_for_byte() {
    assert_fragment(
        mailbox_gmail_oauth_api::openapi_fragment(),
        include_str!("../../../openapi/v1/fragments/mailbox-gmail-oauth.json"),
    );
    assert_fragment(
        standards_mailbox_onboarding_api::openapi_fragment(),
        include_str!("../../../openapi/v1/fragments/mailbox-imap-smtp-onboarding.json"),
    );
    assert_fragment(
        mailbox_microsoft_graph_onboarding_api::openapi_fragment(),
        include_str!("../../../openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json"),
    );
    assert_fragment(
        operator_query_api::openapi_fragment(),
        include_str!("../../../openapi/v1/fragments/operator-query.json"),
    );
    assert_fragment(
        profile_relationship_api::openapi_fragment(),
        include_str!("../../../openapi/v1/fragments/profile-relationship.json"),
    );
}

#[test]
fn shared_problem_schema_accepts_every_rust_producer_code() -> Result<(), Box<dyn std::error::Error>>
{
    use control_plane_contract::public_api::PROBLEM_CODES;

    let root: serde_json::Value =
        serde_json::from_str(include_str!("../../../openapi/v1/openapi.json"))?;
    let codes = root["components"]["schemas"]["Problem"]["properties"]["code"]["enum"]
        .as_array()
        .ok_or("shared Problem must have a closed error-code enum")?;
    for code in PROBLEM_CODES {
        assert!(
            codes.iter().any(|value| value.as_str() == Some(code)),
            "shared Problem rejects Rust producer code {code}"
        );
    }
    // The pre-existing public unauthorized code is retained; all other values
    // must belong to the existing Rust public error taxonomy.
    assert_eq!(codes.len(), PROBLEM_CODES.len() + 1);
    assert!(
        codes
            .iter()
            .any(|value| value.as_str() == Some("unauthorized"))
    );
    Ok(())
}
