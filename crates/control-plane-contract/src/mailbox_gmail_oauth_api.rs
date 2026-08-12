use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartGmailOAuthRequestDto {
    pub expected_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GmailOAuthStartReceiptDto {
    pub onboarding_id: String,
    pub expected_version: u64,
    pub ceremony_id: String,
    pub authorization_url: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GmailOAuthCallbackReceiptDto {
    pub result_code: String,
    pub onboarding_id: String,
    pub onboarding_version: u64,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/gmail-oauth": {
                "post": {
                    "operationId": "startGmailOAuthOnboarding",
                    "parameters": onboarding_path_parameters(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref("StartGmailOAuthRequestDto")
                            }
                        }
                    },
                    "responses": {
                        "200": json_response("Short-lived Gmail authorization ceremony", "GmailOAuthStartReceiptDto"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            },
            "/api/v1/mailbox/gmail/oauth/callback": {
                "get": {
                    "operationId": "completeGmailOAuthOnboarding",
                    "parameters": callback_parameters(),
                    "responses": {
                        "200": json_response("Bounded Gmail OAuth completion result", "GmailOAuthCallbackReceiptDto"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "410": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "StartGmailOAuthRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedVersion", "requestDigest"],
                    "properties": {
                        "expectedVersion": version_schema(),
                        "requestDigest": sha256_schema()
                    }
                },
                "GmailOAuthStartReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["onboardingId", "expectedVersion", "ceremonyId", "authorizationUrl", "expiresAtMs"],
                    "properties": {
                        "onboardingId": opaque_id_schema(),
                        "expectedVersion": version_schema(),
                        "ceremonyId": {"type": "string", "minLength": 8, "maxLength": 128},
                        "authorizationUrl": {"type": "string", "format": "uri", "maxLength": 4096},
                        "expiresAtMs": timestamp_schema()
                    }
                },
                "GmailOAuthCallbackReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["resultCode", "onboardingId", "onboardingVersion"],
                    "properties": {
                        "resultCode": {"type": "string", "enum": ["activated", "denied"]},
                        "onboardingId": opaque_id_schema(),
                        "onboardingVersion": version_schema()
                    }
                }
            }
        }
    })
}

fn onboarding_path_parameters() -> Value {
    json!([
        {
            "name": "tenantId",
            "in": "path",
            "required": true,
            "schema": opaque_id_schema()
        },
        {
            "name": "onboardingId",
            "in": "path",
            "required": true,
            "schema": opaque_id_schema()
        }
    ])
}

fn callback_parameters() -> Value {
    json!([
        {
            "name": "state",
            "in": "query",
            "required": true,
            "schema": {"type": "string", "minLength": 16, "maxLength": 2048}
        },
        {
            "name": "code",
            "in": "query",
            "required": false,
            "schema": {"type": "string", "minLength": 1, "maxLength": 8192}
        },
        {
            "name": "error",
            "in": "query",
            "required": false,
            "schema": {"type": "string", "maxLength": 128}
        }
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

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn json_response(description: &str, schema: &str) -> Value {
    json!({
        "description": description,
        "headers": {
            "Cache-Control": {
                "schema": {"type": "string"},
                "description": "Always no-store for OAuth ceremony responses"
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
    use super::{GmailOAuthCallbackReceiptDto, GmailOAuthStartReceiptDto, StartGmailOAuthRequestDto, openapi_fragment};

    #[test]
    fn public_dtos_reject_credential_and_token_fields() -> Result<(), Box<dyn std::error::Error>> {
        let digest = "a".repeat(64);
        let valid = format!(r#"{{"expectedVersion":1,"requestDigest":"{digest}"}}"#);
        assert!(serde_json::from_str::<StartGmailOAuthRequestDto>(&valid).is_ok());
        for forbidden in [
            "accessToken",
            "refreshToken",
            "authorizationCode",
            "pkceVerifier",
            "clientSecret",
            "secretHandle",
            "gmailSendScope",
        ] {
            let invalid = format!(
                r#"{{"expectedVersion":1,"requestDigest":"{digest}","{forbidden}":"forbidden"}}"#
            );
            assert!(serde_json::from_str::<StartGmailOAuthRequestDto>(&invalid).is_err());
        }
        let _ = GmailOAuthStartReceiptDto {
            onboarding_id: "onboarding_01JC2GMAIL".to_owned(),
            expected_version: 1,
            ceremony_id: "ceremony_01JC2GMAIL".to_owned(),
            authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
            expires_at_ms: 1,
        };
        let _ = GmailOAuthCallbackReceiptDto {
            result_code: "activated".to_owned(),
            onboarding_id: "onboarding_01JC2GMAIL".to_owned(),
            onboarding_version: 2,
        };
        Ok(())
    }

    #[test]
    fn fragment_contains_only_start_and_fixed_callback_surfaces() {
        let fragment = openapi_fragment();
        let paths = fragment["paths"].as_object().expect("paths must be object");
        assert_eq!(paths.len(), 2);
        assert!(paths.contains_key(
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/gmail-oauth"
        ));
        assert!(paths.contains_key("/api/v1/mailbox/gmail/oauth/callback"));
        let encoded = fragment.to_string();
        for forbidden in ["accessToken", "refreshToken", "pkceVerifier", "clientSecret", "gmail.send"] {
            assert!(!encoded.contains(forbidden), "fragment leaked {forbidden}");
        }
    }
}
