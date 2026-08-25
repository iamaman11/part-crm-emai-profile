use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMicrosoftGraphOAuthRequestDto {
    pub expected_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MicrosoftGraphOAuthStartReceiptDto {
    pub onboarding_id: String,
    pub expected_version: u64,
    pub ceremony_id: String,
    pub authorization_url: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MicrosoftGraphOAuthCallbackReceiptDto {
    pub result_code: String,
    pub onboarding_id: String,
    pub onboarding_version: u64,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/microsoft-graph-oauth": {
                "post": {
                    "operationId": "startMicrosoftGraphOAuthOnboarding",
                    "parameters": onboarding_path_parameters(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref("StartMicrosoftGraphOAuthRequestDto")
                            }
                        }
                    },
                    "responses": {
                        "200": json_response("Short-lived Microsoft Graph authorization ceremony", "MicrosoftGraphOAuthStartReceiptDto"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            },
            "/api/v1/mailbox/microsoft-graph/oauth/callback": {
                "get": {
                    "operationId": "completeMicrosoftGraphOAuthOnboarding",
                    "parameters": callback_parameters(),
                    "responses": {
                        "200": json_response("Bounded Microsoft Graph OAuth completion result", "MicrosoftGraphOAuthCallbackReceiptDto"),
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
                "StartMicrosoftGraphOAuthRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedVersion"],
                    "properties": {
                        "expectedVersion": version_schema()
                    }
                },
                "MicrosoftGraphOAuthStartReceiptDto": {
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
                "MicrosoftGraphOAuthCallbackReceiptDto": {
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
        "content": {
            "application/problem+json": {
                "schema": schema_ref("Problem")
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MicrosoftGraphOAuthCallbackReceiptDto, MicrosoftGraphOAuthStartReceiptDto,
        StartMicrosoftGraphOAuthRequestDto, openapi_fragment,
    };

    #[test]
    fn public_dtos_reject_token_and_secret_fields() -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            serde_json::from_str::<StartMicrosoftGraphOAuthRequestDto>(r#"{"expectedVersion":1}"#)
                .is_ok()
        );
        for forbidden in [
            "accessToken",
            "refreshToken",
            "authorizationCode",
            "pkceVerifier",
            "clientSecret",
            "secretHandle",
            "mailSendScope",
        ] {
            let invalid = format!(r#"{{"expectedVersion":1,"{forbidden}":"forbidden"}}"#);
            assert!(
                serde_json::from_str::<StartMicrosoftGraphOAuthRequestDto>(&invalid).is_err()
            );
        }
        let _ = MicrosoftGraphOAuthStartReceiptDto {
            onboarding_id: "onboarding_01JC3GRAPH".to_owned(),
            expected_version: 1,
            ceremony_id: "ceremony_01JC3GRAPH".to_owned(),
            authorization_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_owned(),
            expires_at_ms: 1,
        };
        let _ = MicrosoftGraphOAuthCallbackReceiptDto {
            result_code: "activated".to_owned(),
            onboarding_id: "onboarding_01JC3GRAPH".to_owned(),
            onboarding_version: 2,
        };
        Ok(())
    }

    #[test]
    fn fragment_is_graph_specific_read_onboarding_without_secret_surface() {
        let fragment = openapi_fragment();
        let paths = &fragment["paths"];
        assert!(paths.as_object().is_some_and(|value| value.len() == 2));
        assert!(
            paths
                .get("/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/microsoft-graph-oauth")
                .is_some()
        );
        assert!(
            paths
                .get("/api/v1/mailbox/microsoft-graph/oauth/callback")
                .is_some()
        );
        let encoded = fragment.to_string();
        for forbidden in [
            "accessToken",
            "refreshToken",
            "pkceVerifier",
            "clientSecret",
            "Mail.Send",
            "IMAP.AccessAsUser.All",
            "SMTP.Send",
        ] {
            assert!(!encoded.contains(forbidden), "fragment leaked {forbidden}");
        }
    }

    #[test]
    fn fragment_matches_the_generated_artifact_byte_for_byte() {
        let generated = openapi_fragment().to_string();
        let accepted = include_str!(
            "../../../openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json"
        )
        .trim_end();
        assert_eq!(generated, accepted);
    }
}
