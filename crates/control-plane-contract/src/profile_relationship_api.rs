use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileDetachmentRequest {
    pub reason: String,
    pub expected_profile_version: u64,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/assignment": {
                "delete": {
                    "operationId": "detachProfileFromClient",
                    "security": [{"cloudflareAccessJwt": []}],
                    "parameters": [
                        {"$ref": "#/components/parameters/TenantPath"},
                        {"$ref": "#/components/parameters/ProfilePath"},
                        {"$ref": "#/components/parameters/CorrelationHeader"},
                        {"$ref": "#/components/parameters/IdempotencyHeader"}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/ProfileDetachmentRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Active profile/client assignment detached without changing authorization",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/MutationReceipt"}
                                }
                            }
                        },
                        "400": {"$ref": "#/components/responses/InvalidRequest"},
                        "404": {"$ref": "#/components/responses/NeutralNotFound"},
                        "409": {"$ref": "#/components/responses/Conflict"},
                        "500": {"$ref": "#/components/responses/InternalFailure"},
                        "503": {"$ref": "#/components/responses/DependencyUnavailable"}
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "ProfileDetachmentRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["reason", "expectedProfileVersion"],
                    "properties": {
                        "reason": {"type": "string", "minLength": 1, "maxLength": 500},
                        "expectedProfileVersion": {"$ref": "#/components/schemas/AggregateVersion"}
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{ProfileDetachmentRequest, openapi_fragment};

    #[test]
    fn detachment_request_is_strict_and_keeps_relation_identity_server_owned() {
        assert!(
            serde_json::from_str::<ProfileDetachmentRequest>(
                r#"{"reason":"operator detach","expectedProfileVersion":3}"#
            )
            .is_ok()
        );
        for invalid in [
            r#"{"reason":"operator detach","expectedProfileVersion":3,"assignmentId":"assignment_01JTEST"}"#,
            r#"{"reason":"operator detach","expectedProfileVersion":3,"clientId":"client_01JTEST"}"#,
            r#"{"reason":"operator detach","expectedProfileVersion":3,"requestDigest":"legacy"}"#,
            r#"{"reason":"operator detach","expectedProfileVersion":3,"futureField":true}"#,
        ] {
            assert!(serde_json::from_str::<ProfileDetachmentRequest>(invalid).is_err());
        }
    }

    #[test]
    fn fragment_adds_only_detach_to_existing_assignment_resource() {
        let fragment = openapi_fragment();
        let path = &fragment["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/assignment"];
        assert!(path.get("delete").is_some());
        assert!(path.get("put").is_none());
        assert_eq!(path["delete"]["operationId"], "detachProfileFromClient");
        assert!(fragment["components"]["schemas"]["ProfileDetachmentRequest"].is_object());
    }
}
