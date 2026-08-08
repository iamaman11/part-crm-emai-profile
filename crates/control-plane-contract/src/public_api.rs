use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";
pub const MEMBERSHIP_ROLES: [&str; 2] = ["TENANT_OWNER", "MEMBER"];
pub const CLIENT_KINDS: [&str; 2] = ["PERSON", "ORGANIZATION"];
pub const CLIENT_STATUSES: [&str; 3] = ["ACTIVE", "ARCHIVED", "MERGED"];
pub const CLIENT_GRANT_ROLES: [&str; 2] = ["CLIENT_VIEWER", "CLIENT_EDITOR"];
pub const PROBLEM_CODES: [&str; 11] = [
    "not_found",
    "forbidden",
    "invalid_request",
    "invalid_state",
    "version_conflict",
    "lease_conflict",
    "replay_rejected",
    "dependency_unavailable",
    "integrity_failure",
    "internal_failure",
    "conflict",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorSession {
    pub tenant_id: String,
    pub actor_id: String,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationReceipt {
    pub result_code: String,
    pub resource_id: String,
    pub aggregate_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProjection {
    pub client_id: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCreateRequest {
    pub client_id: String,
    pub kind: String,
    pub display_name: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientGrantRequest {
    pub role: String,
    pub reason: String,
    pub expected_client_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProblemPayload {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub code: String,
    pub correlation_id: String,
}

#[must_use]
pub fn problem_type_for_code(code: &str) -> &'static str {
    match code {
        "not_found" => "urn:part-crm:problem:not-found",
        "forbidden" => "urn:part-crm:problem:forbidden",
        "invalid_request" => "urn:part-crm:problem:invalid-request",
        "invalid_state" => "urn:part-crm:problem:invalid-state",
        "version_conflict" => "urn:part-crm:problem:version-conflict",
        "lease_conflict" => "urn:part-crm:problem:lease-conflict",
        "replay_rejected" => "urn:part-crm:problem:replay-rejected",
        "dependency_unavailable" => "urn:part-crm:problem:dependency-unavailable",
        "integrity_failure" => "urn:part-crm:problem:integrity-failure",
        "internal_failure" => "urn:part-crm:problem:internal-failure",
        "conflict" => "urn:part-crm:problem:conflict",
        _ => "urn:part-crm:problem:internal-failure",
    }
}

#[must_use]
pub fn openapi_document() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Part CRM Control Plane Public API",
            "version": "1.0.0"
        },
        "paths": {
            "/api/v1/session": {
                "get": {
                    "operationId": "getSession",
                    "responses": {
                        "200": json_response("ActorSession"),
                        "404": problem_response()
                    }
                }
            },
            "/api/v1/tenants/{tenantId}/clients": {
                "post": {
                    "operationId": "createClient",
                    "parameters": [tenant_path_parameter()],
                    "requestBody": json_request("ClientCreateRequest"),
                    "responses": {
                        "200": json_response("MutationReceipt"),
                        "201": json_response("MutationReceipt"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            },
            "/api/v1/tenants/{tenantId}/clients/{clientId}": {
                "get": {
                    "operationId": "getClient",
                    "parameters": [tenant_path_parameter(), opaque_path_parameter("clientId")],
                    "responses": {
                        "200": json_response("ClientProjection"),
                        "404": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            },
            "/api/v1/tenants/{tenantId}/clients/{clientId}/grants/{actorId}": {
                "put": {
                    "operationId": "setClientGrant",
                    "parameters": [
                        tenant_path_parameter(),
                        opaque_path_parameter("clientId"),
                        opaque_path_parameter("actorId")
                    ],
                    "requestBody": json_request("ClientGrantRequest"),
                    "responses": {
                        "200": json_response("MutationReceipt"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                },
                "delete": {
                    "operationId": "revokeClientGrant",
                    "parameters": [
                        tenant_path_parameter(),
                        opaque_path_parameter("clientId"),
                        opaque_path_parameter("actorId")
                    ],
                    "requestBody": json_request("ClientGrantRequest"),
                    "responses": {
                        "204": {"description": "Grant revoked"},
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "MembershipRole": string_enum(&MEMBERSHIP_ROLES),
                "ClientKind": string_enum(&CLIENT_KINDS),
                "ClientStatus": string_enum(&CLIENT_STATUSES),
                "ClientGrantRole": string_enum(&CLIENT_GRANT_ROLES),
                "ProblemCode": string_enum(&PROBLEM_CODES),
                "ActorSession": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["tenantId", "actorId", "role"],
                    "properties": {
                        "tenantId": {"type": "string"},
                        "actorId": {"type": "string"},
                        "role": schema_ref("MembershipRole")
                    }
                },
                "MutationReceipt": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["resultCode", "resourceId", "aggregateVersion"],
                    "properties": {
                        "resultCode": {"type": "string"},
                        "resourceId": {"type": "string"},
                        "aggregateVersion": {"type": "integer", "format": "uint64", "minimum": 1}
                    }
                },
                "ClientProjection": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["clientId", "kind", "displayName", "status", "version"],
                    "properties": {
                        "clientId": {"type": "string"},
                        "kind": schema_ref("ClientKind"),
                        "displayName": {"type": "string"},
                        "status": schema_ref("ClientStatus"),
                        "version": {"type": "integer", "format": "uint64", "minimum": 1}
                    }
                },
                "ClientCreateRequest": {
                    "type": "object",
                    "required": ["clientId", "kind", "displayName", "requestDigest"],
                    "properties": {
                        "clientId": {"type": "string"},
                        "kind": schema_ref("ClientKind"),
                        "displayName": {"type": "string"},
                        "requestDigest": digest_schema()
                    }
                },
                "ClientGrantRequest": {
                    "type": "object",
                    "required": ["role", "reason", "expectedClientVersion", "requestDigest"],
                    "properties": {
                        "role": schema_ref("ClientGrantRole"),
                        "reason": {"type": "string"},
                        "expectedClientVersion": {"type": "integer", "format": "uint64", "minimum": 1},
                        "requestDigest": digest_schema()
                    }
                },
                "ProblemPayload": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["type", "title", "status", "code", "correlation_id"],
                    "properties": {
                        "type": {"type": "string"},
                        "title": {"type": "string"},
                        "status": {"type": "integer", "format": "uint16", "minimum": 400, "maximum": 599},
                        "code": schema_ref("ProblemCode"),
                        "correlation_id": {"type": "string"}
                    }
                }
            }
        }
    })
}

pub fn openapi_json_pretty() -> Result<String, serde_json::Error> {
    let mut rendered = serde_json::to_string_pretty(&openapi_document())?;
    rendered.push('\n');
    Ok(rendered)
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn digest_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn tenant_path_parameter() -> Value {
    opaque_path_parameter("tenantId")
}

fn opaque_path_parameter(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": {"type": "string"}
    })
}

fn json_request(schema: &str) -> Value {
    json!({
        "required": true,
        "content": {
            "application/json": {
                "schema": schema_ref(schema)
            }
        }
    })
}

fn json_response(schema: &str) -> Value {
    json!({
        "description": "Successful response",
        "content": {
            "application/json": {
                "schema": schema_ref(schema)
            }
        }
    })
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {
            "application/problem+json": {
                "schema": schema_ref("ProblemPayload")
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ActorSession, ClientCreateRequest, ClientProjection, MutationReceipt, PROBLEM_CODES,
        ProblemPayload, openapi_document, problem_type_for_code,
    };

    #[test]
    fn canonical_transport_models_keep_wire_field_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let session = serde_json::to_value(ActorSession {
            tenant_id: "tenant_01JCONTRACT".to_owned(),
            actor_id: "actor_01JCONTRACT".to_owned(),
            role: "TENANT_OWNER".to_owned(),
        })?;
        assert!(session.get("tenantId").is_some());
        assert!(session.get("actorId").is_some());

        let receipt = serde_json::to_value(MutationReceipt {
            result_code: "created".to_owned(),
            resource_id: "client_01JCONTRACT".to_owned(),
            aggregate_version: 1,
        })?;
        assert!(receipt.get("resultCode").is_some());
        assert!(receipt.get("resourceId").is_some());
        assert!(receipt.get("aggregateVersion").is_some());

        let client = serde_json::to_value(ClientProjection {
            client_id: "client_01JCONTRACT".to_owned(),
            kind: "PERSON".to_owned(),
            display_name: "Client".to_owned(),
            status: "ACTIVE".to_owned(),
            version: 1,
        })?;
        assert!(client.get("clientId").is_some());
        assert!(client.get("displayName").is_some());

        let problem = serde_json::to_value(ProblemPayload {
            problem_type: "urn:part-crm:problem:not-found".to_owned(),
            title: "Not Found".to_owned(),
            status: 404,
            code: "not_found".to_owned(),
            correlation_id: "corr_01JCONTRACT".to_owned(),
        })?;
        assert!(problem.get("type").is_some());
        assert!(problem.get("correlation_id").is_some());
        Ok(())
    }

    #[test]
    fn client_request_keeps_legacy_unknown_field_tolerance() {
        let payload = r#"{
            "clientId":"client_01JCONTRACT",
            "kind":"PERSON",
            "displayName":"Client",
            "requestDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "legacyIgnoredField":"still-tolerated"
        }"#;
        assert!(serde_json::from_str::<ClientCreateRequest>(payload).is_ok());
    }

    #[test]
    fn openapi_contains_migrated_components_and_paths() {
        let document = openapi_document();
        let schemas = &document["components"]["schemas"];
        for name in [
            "ActorSession",
            "MutationReceipt",
            "ClientProjection",
            "ClientCreateRequest",
            "ClientGrantRequest",
            "ProblemPayload",
        ] {
            assert!(schemas.get(name).is_some(), "missing schema {name}");
        }
        assert!(document["paths"]["/api/v1/session"]["get"].is_object());
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/clients"]["post"].is_object());
    }

    #[test]
    fn every_problem_code_has_a_stable_problem_type() {
        for code in PROBLEM_CODES {
            assert_ne!(
                problem_type_for_code(code),
                "urn:part-crm:problem:internal-failure",
                "known problem code {code} fell through"
            );
        }
        assert_eq!(
            problem_type_for_code("internal_failure"),
            "urn:part-crm:problem:internal-failure"
        );
        assert_eq!(
            problem_type_for_code("unknown_code"),
            "urn:part-crm:problem:internal-failure"
        );
    }
}
