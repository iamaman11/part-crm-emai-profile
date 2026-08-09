use crate::public_api::ClientProjection;
use core::fmt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const CLIENT_CONTACT_KINDS: [&str; 3] = ["EMAIL", "PHONE", "URL"];
pub const CLIENT_CONTACT_STATUSES: [&str; 2] = ["ACTIVE", "ARCHIVED"];
pub const CLIENT_ASSIGNMENT_STATUSES: [&str; 2] = ["ACTIVE", "CLOSED"];

const CLIENT_REGISTRY_SCHEMA_NAMES: [&str; 13] = [
    "ClientContactKind",
    "ClientContactStatus",
    "ClientAssignmentStatus",
    "ClientListProjection",
    "ClientUpdateRequest",
    "ClientArchiveRequest",
    "ClientContactUpsertRequest",
    "ClientContactArchiveRequest",
    "ClientMergeRequest",
    "ClientContactProjection",
    "ClientAssignmentProjection",
    "ClientActivityProjection",
    "ClientHistoryProjection",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientRegistryOpenApiError {
    MissingPathsObject,
    MissingSchemasObject,
    InvalidPathItem,
    DuplicateOperation,
    DuplicateSchema,
}

impl fmt::Display for ClientRegistryOpenApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingPathsObject => "client registry OpenAPI paths object is missing",
            Self::MissingSchemasObject => "client registry OpenAPI schemas object is missing",
            Self::InvalidPathItem => "client registry OpenAPI path item is invalid",
            Self::DuplicateOperation => "client registry OpenAPI operation already exists",
            Self::DuplicateSchema => "client registry OpenAPI schema already exists",
        })
    }
}

impl std::error::Error for ClientRegistryOpenApiError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientListProjection {
    pub clients: Vec<ClientProjection>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientUpdateRequest {
    pub display_name: String,
    pub expected_client_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientArchiveRequest {
    pub expected_client_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientContactUpsertRequest {
    pub kind: String,
    pub value: String,
    pub expected_client_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientContactArchiveRequest {
    pub kind: String,
    pub expected_client_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientMergeRequest {
    pub target_client_id: String,
    pub expected_source_version: u64,
    pub expected_target_version: u64,
    pub reason: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientContactProjection {
    pub contact_point_id: String,
    pub kind: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientAssignmentProjection {
    pub assignment_id: String,
    pub profile_id: String,
    pub status: String,
    pub assigned_at_ms: u64,
    pub closed_at_ms: Option<u64>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientActivityProjection {
    pub audit_event_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub result_code: String,
    pub occurred_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientHistoryProjection {
    pub contacts: Vec<ClientContactProjection>,
    pub assignments: Vec<ClientAssignmentProjection>,
    pub activity: Vec<ClientActivityProjection>,
}

pub fn extend_openapi(document: &mut Value) -> Result<(), ClientRegistryOpenApiError> {
    let mut candidate = document.clone();
    extend_openapi_in_place(&mut candidate)?;
    *document = candidate;
    Ok(())
}

fn extend_openapi_in_place(document: &mut Value) -> Result<(), ClientRegistryOpenApiError> {
    let paths = document
        .get_mut("paths")
        .and_then(Value::as_object_mut)
        .ok_or(ClientRegistryOpenApiError::MissingPathsObject)?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients",
        "get",
        json!({
            "operationId": "listClients",
            "parameters": [tenant_path_parameter()],
            "responses": {
                "200": json_response("ClientListProjection"),
                "404": problem_response(),
                "500": problem_response(),
                "503": problem_response()
            }
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}",
        "patch",
        json!({
            "operationId": "updateClient",
            "parameters": [tenant_path_parameter(), opaque_path_parameter("clientId")],
            "requestBody": json_request("ClientUpdateRequest"),
            "responses": mutation_responses()
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}/archive",
        "post",
        json!({
            "operationId": "archiveClient",
            "parameters": [tenant_path_parameter(), opaque_path_parameter("clientId")],
            "requestBody": json_request("ClientArchiveRequest"),
            "responses": mutation_responses()
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}/contacts/{contactPointId}",
        "put",
        json!({
            "operationId": "upsertClientContact",
            "parameters": [
                tenant_path_parameter(),
                opaque_path_parameter("clientId"),
                opaque_path_parameter("contactPointId")
            ],
            "requestBody": json_request("ClientContactUpsertRequest"),
            "responses": mutation_responses()
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}/contacts/{contactPointId}",
        "delete",
        json!({
            "operationId": "archiveClientContact",
            "parameters": [
                tenant_path_parameter(),
                opaque_path_parameter("clientId"),
                opaque_path_parameter("contactPointId")
            ],
            "requestBody": json_request("ClientContactArchiveRequest"),
            "responses": mutation_responses()
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}/merge",
        "post",
        json!({
            "operationId": "mergeClient",
            "parameters": [tenant_path_parameter(), opaque_path_parameter("clientId")],
            "requestBody": json_request("ClientMergeRequest"),
            "responses": mutation_responses()
        }),
    )?;
    insert_operation(
        paths,
        "/api/v1/tenants/{tenantId}/clients/{clientId}/history",
        "get",
        json!({
            "operationId": "getClientHistory",
            "parameters": [tenant_path_parameter(), opaque_path_parameter("clientId")],
            "responses": {
                "200": json_response("ClientHistoryProjection"),
                "404": problem_response(),
                "500": problem_response(),
                "503": problem_response()
            }
        }),
    )?;

    let schemas = document
        .get_mut("components")
        .and_then(|value| value.get_mut("schemas"))
        .and_then(Value::as_object_mut)
        .ok_or(ClientRegistryOpenApiError::MissingSchemasObject)?;
    if CLIENT_REGISTRY_SCHEMA_NAMES
        .iter()
        .any(|name| schemas.contains_key(*name))
    {
        return Err(ClientRegistryOpenApiError::DuplicateSchema);
    }
    schemas.insert(
        "ClientContactKind".to_owned(),
        string_enum(&CLIENT_CONTACT_KINDS),
    );
    schemas.insert(
        "ClientContactStatus".to_owned(),
        string_enum(&CLIENT_CONTACT_STATUSES),
    );
    schemas.insert(
        "ClientAssignmentStatus".to_owned(),
        string_enum(&CLIENT_ASSIGNMENT_STATUSES),
    );
    schemas.insert(
        "ClientListProjection".to_owned(),
        object_schema(
            &["clients"],
            json!({
                "clients": {
                    "type": "array",
                    "maxItems": 500,
                    "items": schema_ref("ClientProjection")
                }
            }),
        ),
    );
    schemas.insert(
        "ClientUpdateRequest".to_owned(),
        object_schema(
            &["displayName", "expectedClientVersion", "requestDigest"],
            json!({
                "displayName": {"type": "string", "minLength": 1, "maxLength": 200},
                "expectedClientVersion": positive_version_schema(),
                "requestDigest": digest_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientArchiveRequest".to_owned(),
        object_schema(
            &["expectedClientVersion", "requestDigest"],
            json!({
                "expectedClientVersion": positive_version_schema(),
                "requestDigest": digest_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientContactUpsertRequest".to_owned(),
        object_schema(
            &["kind", "value", "expectedClientVersion", "requestDigest"],
            json!({
                "kind": schema_ref("ClientContactKind"),
                "value": {"type": "string", "minLength": 1, "maxLength": 2048},
                "expectedClientVersion": positive_version_schema(),
                "requestDigest": digest_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientContactArchiveRequest".to_owned(),
        object_schema(
            &["kind", "expectedClientVersion", "requestDigest"],
            json!({
                "kind": schema_ref("ClientContactKind"),
                "expectedClientVersion": positive_version_schema(),
                "requestDigest": digest_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientMergeRequest".to_owned(),
        object_schema(
            &[
                "targetClientId",
                "expectedSourceVersion",
                "expectedTargetVersion",
                "reason",
                "requestDigest",
            ],
            json!({
                "targetClientId": {"type": "string"},
                "expectedSourceVersion": positive_version_schema(),
                "expectedTargetVersion": positive_version_schema(),
                "reason": {"type": "string", "minLength": 1, "maxLength": 500},
                "requestDigest": digest_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientContactProjection".to_owned(),
        object_schema(
            &["contactPointId", "kind", "status"],
            json!({
                "contactPointId": {"type": "string"},
                "kind": schema_ref("ClientContactKind"),
                "status": schema_ref("ClientContactStatus")
            }),
        ),
    );
    schemas.insert(
        "ClientAssignmentProjection".to_owned(),
        object_schema(
            &[
                "assignmentId",
                "profileId",
                "status",
                "assignedAtMs",
                "closedAtMs",
                "reason",
            ],
            json!({
                "assignmentId": {"type": "string"},
                "profileId": {"type": "string"},
                "status": schema_ref("ClientAssignmentStatus"),
                "assignedAtMs": non_negative_time_schema(),
                "closedAtMs": {"type": "integer", "format": "uint64", "minimum": 0, "nullable": true},
                "reason": {"type": "string"}
            }),
        ),
    );
    schemas.insert(
        "ClientActivityProjection".to_owned(),
        object_schema(
            &[
                "auditEventId",
                "action",
                "resourceType",
                "resourceId",
                "resultCode",
                "occurredAtMs",
            ],
            json!({
                "auditEventId": {"type": "string"},
                "action": {"type": "string"},
                "resourceType": {"type": "string"},
                "resourceId": {"type": "string"},
                "resultCode": {"type": "string"},
                "occurredAtMs": non_negative_time_schema()
            }),
        ),
    );
    schemas.insert(
        "ClientHistoryProjection".to_owned(),
        object_schema(
            &["contacts", "assignments", "activity"],
            json!({
                "contacts": {"type": "array", "maxItems": 500, "items": schema_ref("ClientContactProjection")},
                "assignments": {"type": "array", "maxItems": 500, "items": schema_ref("ClientAssignmentProjection")},
                "activity": {"type": "array", "maxItems": 100, "items": schema_ref("ClientActivityProjection")}
            }),
        ),
    );
    Ok(())
}

fn insert_operation(
    paths: &mut Map<String, Value>,
    path: &str,
    method: &str,
    operation: Value,
) -> Result<(), ClientRegistryOpenApiError> {
    let path_item = paths
        .entry(path.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let path_item = path_item
        .as_object_mut()
        .ok_or(ClientRegistryOpenApiError::InvalidPathItem)?;
    if path_item.contains_key(method) {
        return Err(ClientRegistryOpenApiError::DuplicateOperation);
    }
    path_item.insert(method.to_owned(), operation);
    Ok(())
}

fn mutation_responses() -> Value {
    json!({
        "200": json_response("MutationReceipt"),
        "400": problem_response(),
        "404": problem_response(),
        "409": problem_response(),
        "500": problem_response(),
        "503": problem_response()
    })
}

fn object_schema(required: &[&str], properties: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties
    })
}

fn positive_version_schema() -> Value {
    json!({"type": "integer", "format": "uint64", "minimum": 1})
}

fn non_negative_time_schema() -> Value {
    json!({"type": "integer", "format": "uint64", "minimum": 0})
}

fn digest_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
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
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn json_response(schema: &str) -> Value {
    json!({
        "description": "Successful response",
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": schema_ref("ProblemPayload")}}
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ClientArchiveRequest, ClientAssignmentProjection, ClientContactArchiveRequest,
        ClientContactProjection, ClientContactUpsertRequest, ClientHistoryProjection,
        ClientListProjection, ClientMergeRequest, ClientUpdateRequest, extend_openapi,
    };
    use crate::public_api::{ClientProjection, openapi_document};
    use serde_json::json;

    #[test]
    fn client_registry_models_keep_camel_case_wire_names() -> Result<(), Box<dyn std::error::Error>>
    {
        let update = serde_json::to_value(ClientUpdateRequest {
            display_name: "Renamed".to_owned(),
            expected_client_version: 2,
            request_digest: "a".repeat(64),
        })?;
        assert!(update.get("displayName").is_some());
        assert!(update.get("expectedClientVersion").is_some());
        assert!(update.get("display_name").is_none());

        let history = serde_json::to_value(ClientHistoryProjection {
            contacts: vec![ClientContactProjection {
                contact_point_id: "contact_01JCONTRACT".to_owned(),
                kind: "EMAIL".to_owned(),
                status: "ACTIVE".to_owned(),
            }],
            assignments: vec![ClientAssignmentProjection {
                assignment_id: "assignment_01JCONTRACT".to_owned(),
                profile_id: "profile_01JCONTRACT".to_owned(),
                status: "ACTIVE".to_owned(),
                assigned_at_ms: 10,
                closed_at_ms: None,
                reason: "primary".to_owned(),
            }],
            activity: Vec::new(),
        })?;
        assert!(history.get("contacts").is_some());
        assert!(history.get("assignments").is_some());
        Ok(())
    }

    #[test]
    fn client_registry_openapi_extension_is_additive_and_complete()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut document = openapi_document();
        extend_openapi(&mut document)?;
        let paths = &document["paths"];
        assert!(paths["/api/v1/tenants/{tenantId}/clients"]["post"].is_object());
        assert!(paths["/api/v1/tenants/{tenantId}/clients"]["get"].is_object());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}"]["get"].is_object());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}"]["patch"].is_object());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}/history"]["get"].is_object());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}/merge"]["post"].is_object());
        let schemas = &document["components"]["schemas"];
        for name in [
            "ClientListProjection",
            "ClientUpdateRequest",
            "ClientArchiveRequest",
            "ClientContactUpsertRequest",
            "ClientContactArchiveRequest",
            "ClientMergeRequest",
            "ClientContactProjection",
            "ClientAssignmentProjection",
            "ClientActivityProjection",
            "ClientHistoryProjection",
        ] {
            assert!(schemas.get(name).is_some(), "missing schema {name}");
        }
        Ok(())
    }

    #[test]
    fn malformed_and_duplicate_openapi_extensions_fail_atomically() {
        let mut missing_paths = json!({"components": {"schemas": {}}});
        let original = missing_paths.clone();
        assert_eq!(
            extend_openapi(&mut missing_paths),
            Err(super::ClientRegistryOpenApiError::MissingPathsObject)
        );
        assert_eq!(missing_paths, original);

        let mut duplicate = openapi_document();
        duplicate["paths"]["/api/v1/tenants/{tenantId}/clients"]["get"] = json!({});
        let original = duplicate.clone();
        assert_eq!(
            extend_openapi(&mut duplicate),
            Err(super::ClientRegistryOpenApiError::DuplicateOperation)
        );
        assert_eq!(duplicate, original);
    }

    #[test]
    fn request_models_reject_unknown_fields() {
        let digest = "a".repeat(64);
        let archive = format!(
            "{{\"expectedClientVersion\":1,\"requestDigest\":\"{digest}\",\"extra\":true}}"
        );
        assert!(serde_json::from_str::<ClientArchiveRequest>(&archive).is_err());
        let contact_archive = format!(
            "{{\"kind\":\"EMAIL\",\"expectedClientVersion\":1,\"requestDigest\":\"{digest}\",\"extra\":true}}"
        );
        assert!(serde_json::from_str::<ClientContactArchiveRequest>(&contact_archive).is_err());
    }

    #[test]
    fn other_registry_request_models_are_constructible() {
        let digest = "a".repeat(64);
        let _list = ClientListProjection {
            clients: vec![ClientProjection {
                client_id: "client_01JCONTRACT".to_owned(),
                kind: "PERSON".to_owned(),
                display_name: "Client".to_owned(),
                status: "ACTIVE".to_owned(),
                version: 1,
            }],
        };
        let _contact_upsert = ClientContactUpsertRequest {
            kind: "EMAIL".to_owned(),
            value: "person@example.com".to_owned(),
            expected_client_version: 1,
            request_digest: digest.clone(),
        };
        let _merge = ClientMergeRequest {
            target_client_id: "client_02JCONTRACT".to_owned(),
            expected_source_version: 1,
            expected_target_version: 1,
            reason: "duplicate".to_owned(),
            request_digest: digest,
        };
    }
}
