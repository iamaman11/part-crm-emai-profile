use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileAssignmentRequest {
    pub client_id: String,
    pub reason: String,
    pub expected_profile_version: u64,
}

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
                "put": {
                    "operationId": "assignProfileToClient",
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
                                "schema": {"$ref": "#/components/schemas/AssignmentRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Historical profile/client assignment updated without changing authorization",
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
                },
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
                "AssignmentRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["clientId", "reason", "expectedProfileVersion"],
                    "properties": {
                        "clientId": {"$ref": "#/components/schemas/OpaqueId"},
                        "reason": {"type": "string", "minLength": 1, "maxLength": 500},
                        "expectedProfileVersion": {"$ref": "#/components/schemas/AggregateVersion"}
                    }
                },
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
    use super::{ProfileAssignmentRequest, ProfileDetachmentRequest, openapi_fragment};

    #[test]
    fn assignment_request_is_strict_and_keeps_relation_identity_server_owned() {
        assert!(
            serde_json::from_str::<ProfileAssignmentRequest>(
                r#"{"clientId":"client_01JTEST","reason":"operator assign","expectedProfileVersion":3}"#
            )
            .is_ok()
        );
        for invalid in [
            r#"{"clientId":"client_01JTEST","reason":"operator assign","expectedProfileVersion":3,"assignmentId":"assignment_01JTEST"}"#,
            r#"{"clientId":"client_01JTEST","reason":"operator assign","expectedProfileVersion":3,"requestDigest":"legacy"}"#,
            r#"{"clientId":"client_01JTEST","reason":"operator assign","expectedProfileVersion":3,"futureField":true}"#,
        ] {
            assert!(serde_json::from_str::<ProfileAssignmentRequest>(invalid).is_err());
        }
    }

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
    fn fragment_owns_assignment_and_detachment_operation_semantics() {
        let fragment = openapi_fragment();
        let path = &fragment["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/assignment"];
        assert_eq!(path["put"]["operationId"], "assignProfileToClient");
        assert_eq!(path["delete"]["operationId"], "detachProfileFromClient");
        assert!(fragment["components"]["schemas"]["AssignmentRequest"].is_object());
        assert!(fragment["components"]["schemas"]["ProfileDetachmentRequest"].is_object());
    }
}
