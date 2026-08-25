#[allow(dead_code)]
#[path = "../src/mailbox_gmail_oauth_api.rs"]
mod mailbox_gmail_oauth_api;
#[allow(dead_code)]
#[path = "../src/mailbox_microsoft_graph_onboarding_api.rs"]
mod mailbox_microsoft_graph_onboarding_api;
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
}
