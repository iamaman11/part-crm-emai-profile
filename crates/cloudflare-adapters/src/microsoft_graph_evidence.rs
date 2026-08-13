#![cfg(test)]

#[test]
fn c3g_microsoft_graph_boundaries_are_permanent_and_fail_closed() {
    let provider = include_str!("../../mailbox-domain/src/runtime_lane.rs");
    assert!(provider.contains("MicrosoftGraph"));
    assert!(provider.contains("Self::MicrosoftGraph => \"MICROSOFT_GRAPH\""));
    assert!(provider.contains("Self::GmailApi | Self::Imap"));

    let migration = include_str!("../../../migrations/d1/0025_microsoft_graph_provider.sql");
    assert!(migration.contains("'MICROSOFT_GRAPH'"));
    assert!(migration.contains("PRAGMA defer_foreign_keys = ON"));

    let browser = include_str!("../../../migrations/d1/0020_browser_mailbox_execution_bindings.sql");
    assert!(browser.contains("provider = 'BROWSER_FALLBACK'"));
    assert!(!browser.contains("MICROSOFT_GRAPH"));

    let oauth_port = include_str!("../../application-ports/src/microsoft_graph_oauth_onboarding.rs");
    for forbidden in ["access_token", "refresh_token", "code_verifier", "code_challenge"] {
        assert!(
            !oauth_port.contains(forbidden),
            "Graph OAuth application port leaked {forbidden}"
        );
    }
    assert!(oauth_port.contains("Result<SecretHandle"));

    let oauth = include_str!("microsoft_graph_oauth_provisioning.rs");
    let oauth_production = oauth.split("#[cfg(test)]").next().unwrap_or(oauth);
    assert!(oauth_production.contains("openid offline_access https://graph.microsoft.com/Mail.Read"));
    assert!(oauth_production.contains("const PKCE_METHOD: &str = \"S256\""));
    assert!(!oauth_production.contains("Mail.Send"));
    assert!(!oauth_production.contains("refresh_token"));
    assert!(!oauth_production.contains("code_verifier"));

    let secrets = include_str!("cloud_mailbox_secrets.rs");
    assert!(secrets.contains("MicrosoftGraphCredential"));
    assert!(secrets.contains("refresh_microsoft_graph_credential"));
    assert!(secrets.contains("self.access_token.zeroize()"));
    assert!(secrets.contains("refresh_token\":\"forbidden"));

    let authorization = include_str!("microsoft_graph_authorization.rs");
    for required in [
        "binding.provider = 'MICROSOFT_GRAPH'",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "client.status = 'ACTIVE'",
        "requester.status = 'ACTIVE'",
        "requester.role = 'TENANT_OWNER'",
        "requester.role = 'MEMBER'",
        "FROM client_grants AS grant_row",
        "mailbox_client_association_state AS association",
    ] {
        assert!(authorization.contains(required), "missing Graph authorization invariant: {required}");
    }

    let eligibility = include_str!("d1_client_mail_eligibility.rs");
    assert!(eligibility.contains("binding.provider IN ('GMAIL_API', 'IMAP')"));
    assert!(eligibility.contains("OR binding.provider = 'MICROSOFT_GRAPH'"));

    let query = include_str!("microsoft_graph_mail_query.rs");
    assert!(query.contains("MAX_GRAPH_QUERY_PAGE_SIZE: u16 = 25"));
    assert!(query.contains("$select=id,subject,from,receivedDateTime"));
    assert!(query.contains("recheck_client_query"));
    assert!(query.contains("refresh_microsoft_graph_credential"));
    assert!(query.contains("https://graph.microsoft.com/v1.0/"));

    let query_cursor = include_str!("microsoft_graph_cursor.rs");
    assert!(query_cursor.contains("const QUERY_CURSOR_PREFIX: &str = \"graph-page:\""));
    assert!(query_cursor.contains("CURSOR_STORE_ENDPOINT"));
    assert!(query_cursor.contains("CURSOR_RESOLVE_ENDPOINT"));
    assert!(query_cursor.contains("body.zeroize()"));

    let delta = include_str!("microsoft_graph_delta.rs");
    assert!(delta.contains("/mailFolders/inbox/messages/delta"));
    assert!(delta.contains("$select=id&$top=100"));
    assert!(delta.contains("response.status_code() == 401") || delta.contains("response.status_code() != 401"));
    assert!(delta.contains("refresh_microsoft_graph_credential"));
    assert!(delta.contains("recheck_job"));
    assert!(delta.contains("429"));
    assert!(delta.contains("retry_after_hint"));
    assert!(delta.contains("400 | 410"));
    assert!(delta.contains("GRAPH_DELTA_RESEEDED"));

    let delta_cursor = include_str!("microsoft_graph_delta_cursor.rs");
    assert!(delta_cursor.contains("const DELTA_CURSOR_PREFIX: &str = \"graph-delta:\""));
    assert!(delta_cursor.contains("CURSOR_STORE_ENDPOINT"));
    assert!(delta_cursor.contains("CURSOR_RESOLVE_ENDPOINT"));
    assert!(delta_cursor.contains("body.zeroize()"));

    let worker = include_str!("../../../apps/control-plane-worker/src/mailbox_microsoft_graph_oauth.rs");
    assert!(worker.contains("/api/v1/mailbox/microsoft-graph/oauth/callback"));
    assert!(worker.contains("cache-control\", \"no-store"));
    assert!(worker.contains("referrer-policy\", \"no-referrer"));
    assert!(!worker.contains("access_token"));
    assert!(!worker.contains("refresh_token"));
    assert!(!worker.contains("code_verifier"));

    let openapi = include_str!("../../../openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json");
    assert!(openapi.contains("microsoft-graph-oauth"));
    assert!(openapi.contains("/api/v1/mailbox/microsoft-graph/oauth/callback"));
    for forbidden in ["accessToken", "refreshToken", "codeVerifier", "clientSecret"] {
        assert!(!openapi.contains(forbidden), "Graph public contract leaked {forbidden}");
    }
}
