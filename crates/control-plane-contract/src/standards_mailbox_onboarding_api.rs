use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StandardsMailTransportSecurityDto {
    ImplicitTls,
    Starttls,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordProtocolConfigurationDto {
    pub host: String,
    pub port: u16,
    pub transport_security: StandardsMailTransportSecurityDto,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvisionPasswordStandardsMailboxRequestDto {
    pub expected_version: u64,
    pub imap: PasswordProtocolConfigurationDto,
    pub smtp: PasswordProtocolConfigurationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMicrosoftStandardsOAuthRequestDto {
    pub expected_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MicrosoftStandardsOAuthStartReceiptDto {
    pub onboarding_id: String,
    pub expected_version: u64,
    pub authentication_mode: String,
    pub ceremony_id: String,
    pub authorization_url: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StandardsMailboxActivationReceiptDto {
    pub result_code: String,
    pub onboarding_id: String,
    pub onboarding_version: u64,
    pub authentication_mode: String,
    pub imap_read_search_ready: bool,
    pub smtp_send_ready: bool,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/imap-smtp/password": {
                "post": {
                    "operationId": "provisionPasswordStandardsMailbox",
                    "parameters": onboarding_path_parameters(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref("ProvisionPasswordStandardsMailboxRequestDto")
                            }
                        }
                    },
                    "responses": standard_activation_responses("Provision and validate encrypted IMAP/SMTP password credentials")
                }
            },
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/imap-smtp/microsoft-oauth": {
                "post": {
                    "operationId": "startMicrosoftStandardsMailboxOAuth",
                    "parameters": onboarding_path_parameters(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref("StartMicrosoftStandardsOAuthRequestDto")
                            }
                        }
                    },
                    "responses": {
                        "200": json_response("Short-lived Microsoft standards-mail OAuth ceremony", "MicrosoftStandardsOAuthStartReceiptDto"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            },
            "/api/v1/mailbox/imap-smtp/microsoft-oauth/callback": {
                "get": {
                    "operationId": "completeMicrosoftStandardsMailboxOAuth",
                    "parameters": callback_parameters(),
                    "responses": standard_activation_responses("Bounded Microsoft standards-mail OAuth completion result")
                }
            }
        },
        "components": {
            "schemas": {
                "PasswordProtocolConfigurationDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["host", "port", "transportSecurity", "username", "password"],
                    "properties": {
                        "host": {"type": "string", "minLength": 4, "maxLength": 253},
                        "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                        "transportSecurity": {"type": "string", "enum": ["IMPLICIT_TLS", "STARTTLS"]},
                        "username": {"type": "string", "minLength": 1, "maxLength": 512},
                        "password": {"type": "string", "minLength": 1, "maxLength": 8192, "format": "password", "writeOnly": true}
                    }
                },
                "ProvisionPasswordStandardsMailboxRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedVersion", "imap", "smtp"],
                    "properties": {
                        "expectedVersion": version_schema(),
                        "imap": schema_ref("PasswordProtocolConfigurationDto"),
                        "smtp": schema_ref("PasswordProtocolConfigurationDto")
                    }
                },
                "StartMicrosoftStandardsOAuthRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedVersion"],
                    "properties": {"expectedVersion": version_schema()}
                },
                "MicrosoftStandardsOAuthStartReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["onboardingId", "expectedVersion", "authenticationMode", "ceremonyId", "authorizationUrl", "expiresAtMs"],
                    "properties": {
                        "onboardingId": opaque_id_schema(),
                        "expectedVersion": version_schema(),
                        "authenticationMode": {"type": "string", "enum": ["MICROSOFT_OAUTH2"]},
                        "ceremonyId": {"type": "string", "minLength": 8, "maxLength": 128},
                        "authorizationUrl": {"type": "string", "format": "uri", "maxLength": 4096},
                        "expiresAtMs": timestamp_schema()
                    }
                },
                "StandardsMailboxActivationReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["resultCode", "onboardingId", "onboardingVersion", "authenticationMode", "imapReadSearchReady", "smtpSendReady"],
                    "properties": {
                        "resultCode": {"type": "string", "enum": ["activated", "denied"]},
                        "onboardingId": opaque_id_schema(),
                        "onboardingVersion": version_schema(),
                        "authenticationMode": {"type": "string", "enum": ["PASSWORD", "MICROSOFT_OAUTH2"]},
                        "imapReadSearchReady": {"type": "boolean"},
                        "smtpSendReady": {"type": "boolean"}
                    }
                }
            }
        }
    })
}

fn standard_activation_responses(description: &str) -> Value {
    json!({
        "200": json_response(description, "StandardsMailboxActivationReceiptDto"),
        "400": problem_response(),
        "404": problem_response(),
        "409": problem_response(),
        "410": problem_response(),
        "500": problem_response(),
        "503": problem_response()
    })
}

fn onboarding_path_parameters() -> Value {
    json!([
        {"name": "tenantId", "in": "path", "required": true, "schema": opaque_id_schema()},
        {"name": "onboardingId", "in": "path", "required": true, "schema": opaque_id_schema()}
    ])
}

fn callback_parameters() -> Value {
    json!([
        {"name": "state", "in": "query", "required": true, "schema": {"type": "string", "minLength": 16, "maxLength": 2048}},
        {"name": "code", "in": "query", "required": false, "schema": {"type": "string", "minLength": 1, "maxLength": 8192}},
        {"name": "error", "in": "query", "required": false, "schema": {"type": "string", "maxLength": 128}}
    ])
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn opaque_id_schema() -> Value {
    json!({"type": "string", "minLength": 8, "maxLength": 96})
}

fn version_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn timestamp_schema() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn json_response(description: &str, schema: &str) -> Value {
    json!({
        "description": description,
        "headers": {
            "Cache-Control": {
                "schema": {"type": "string"},
                "description": "Always no-store for mailbox credential/onboarding responses"
            }
        },
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": {"type": "object"}}}
    })
}

#[cfg(test)]
mod tests {
    use super::{ProvisionPasswordStandardsMailboxRequestDto, openapi_fragment};

    #[test]
    fn password_request_is_input_only_and_rejects_unknown_secret_fields() {
        let valid = r#"{"expectedVersion":1,"imap":{"host":"imap.example.com","port":993,"transportSecurity":"IMPLICIT_TLS","username":"user@example.com","password":"secret"},"smtp":{"host":"smtp.example.com","port":587,"transportSecurity":"STARTTLS","username":"user@example.com","password":"secret"}}"#;
        assert!(serde_json::from_str::<ProvisionPasswordStandardsMailboxRequestDto>(valid).is_ok());
        let forbidden = r#"{"expectedVersion":1,"imap":{"host":"imap.example.com","port":993,"transportSecurity":"IMPLICIT_TLS","username":"user@example.com","password":"secret","accessToken":"forbidden"},"smtp":{"host":"smtp.example.com","port":587,"transportSecurity":"STARTTLS","username":"user@example.com","password":"secret"}}"#;
        assert!(
            serde_json::from_str::<ProvisionPasswordStandardsMailboxRequestDto>(forbidden).is_err()
        );
    }

    #[test]
    fn fragment_is_exact_graph_free_and_secrets_are_request_only() {
        let fragment = openapi_fragment();
        let paths = &fragment["paths"];
        assert!(paths.as_object().is_some_and(|value| value.len() == 3));
        let encoded = fragment.to_string();
        for forbidden in [
            "accessToken",
            "refreshToken",
            "authorizationCode",
            "pkceVerifier",
            "clientSecret",
            "secretHandle",
            "graph.microsoft.com",
            "Mail.Read",
            "Mail.Send",
        ] {
            assert!(!encoded.contains(forbidden), "fragment leaked {forbidden}");
        }
        assert_eq!(
            fragment["components"]["schemas"]["PasswordProtocolConfigurationDto"]["properties"]["password"]
                ["writeOnly"],
            true
        );
    }

    #[test]
    fn fragment_matches_the_accepted_generated_artifact_byte_for_byte() {
        let generated = openapi_fragment().to_string();
        let accepted =
            include_str!("../../../openapi/v1/fragments/mailbox-imap-smtp-onboarding.json")
                .trim_end();
        assert_eq!(generated, accepted);
    }
}
