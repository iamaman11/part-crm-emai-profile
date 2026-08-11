use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const PROFILE_STATUSES: [&str; 9] = [
    "DRAFT",
    "QUARANTINED",
    "READY",
    "IN_USE",
    "DIRTY_LOCAL",
    "SYNCING",
    "SUSPENDED",
    "DELETING",
    "DELETED",
];
pub const PROFILE_GRANT_ROLES: [&str; 2] = ["PROFILE_VIEWER", "PROFILE_OPERATOR"];
pub const GENERATION_STATUSES: [&str; 3] = ["REGISTERED", "VERIFIED", "QUARANTINED"];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileStatusDto {
    Draft,
    Quarantined,
    Ready,
    InUse,
    DirtyLocal,
    Syncing,
    Suspended,
    Deleting,
    Deleted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GenerationStatusDto {
    Registered,
    Verified,
    Quarantined,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProjectionDto {
    pub profile_id: String,
    pub status: ProfileStatusDto,
    pub version: u64,
    pub linked_client_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCreateRequest {
    pub profile_id: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAssignmentRequest {
    pub assignment_id: String,
    pub client_id: String,
    pub reason: String,
    pub expected_profile_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileGrantRequest {
    pub role: String,
    pub reason: String,
    pub expected_profile_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationProjectionDto {
    pub generation_id: String,
    pub metadata_digest: String,
    pub container_digest: String,
    pub status: GenerationStatusDto,
    pub version: u64,
    pub verification_reference: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterGenerationRequest {
    pub generation_id: String,
    pub object_key: String,
    pub metadata_digest: String,
    pub container_digest: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyGenerationRequest {
    pub expected_generation_version: u64,
    pub verification_reference: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileGenerationVersionRequest {
    pub expected_profile_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuarantineGenerationRequest {
    pub expected_generation_version: u64,
    pub request_digest: String,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/profiles": {
                "post": profile_create_operation()
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}": {
                "get": profile_get_operation()
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/assignment": {
                "put": profile_mutation_operation("assignProfile", "ProfileAssignmentRequest")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/grants/{actorId}": {
                "put": profile_grant_operation("setProfileGrant", false),
                "delete": profile_grant_operation("revokeProfileGrant", true)
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations": {
                "post": generation_collection_mutation_operation("registerGeneration", "RegisterGenerationRequest", "201")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}": {
                "get": generation_get_operation()
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/verify": {
                "post": generation_resource_mutation_operation("verifyGeneration", "VerifyGenerationRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/activate": {
                "post": generation_resource_mutation_operation("activateGeneration", "ProfileGenerationVersionRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/deactivate": {
                "post": generation_resource_mutation_operation("deactivateGeneration", "ProfileGenerationVersionRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine": {
                "post": generation_resource_mutation_operation("quarantineGeneration", "QuarantineGenerationRequest", "200")
            }
        },
        "components": {
            "schemas": {
                "ProfileStatusDto": string_enum(&PROFILE_STATUSES),
                "ProfileGrantRoleDto": string_enum(&PROFILE_GRANT_ROLES),
                "GenerationStatusDto": string_enum(&GENERATION_STATUSES),
                "ProfileProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["profileId", "status", "version", "linkedClientId"],
                    "properties": {
                        "profileId": opaque_id_schema(),
                        "status": schema_ref("ProfileStatusDto"),
                        "version": positive_version_schema(),
                        "linkedClientId": nullable_opaque_id_schema()
                    }
                },
                "ProfileCreateRequest": {
                    "type": "object",
                    "required": ["profileId", "requestDigest"],
                    "properties": {
                        "profileId": opaque_id_schema(),
                        "requestDigest": request_digest_schema()
                    }
                },
                "ProfileAssignmentRequest": {
                    "type": "object",
                    "required": ["assignmentId", "clientId", "reason", "expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "assignmentId": opaque_id_schema(),
                        "clientId": opaque_id_schema(),
                        "reason": {"type": "string"},
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": request_digest_schema()
                    }
                },
                "ProfileGrantRequest": {
                    "type": "object",
                    "required": ["role", "reason", "expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "role": schema_ref("ProfileGrantRoleDto"),
                        "reason": {"type": "string"},
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": request_digest_schema()
                    }
                },
                "GenerationProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["generationId", "metadataDigest", "containerDigest", "status", "version", "verificationReference"],
                    "properties": {
                        "generationId": opaque_id_schema(),
                        "metadataDigest": sha256_schema(),
                        "containerDigest": sha256_schema(),
                        "status": schema_ref("GenerationStatusDto"),
                        "version": positive_version_schema(),
                        "verificationReference": {"type": "string", "nullable": true, "minLength": 8, "maxLength": 256, "pattern": "^[A-Za-z0-9_:-]+$"}
                    }
                },
                "RegisterGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["generationId", "objectKey", "metadataDigest", "containerDigest", "requestDigest"],
                    "properties": {
                        "generationId": opaque_id_schema(),
                        "objectKey": {"type": "string", "minLength": 16, "maxLength": 512, "pattern": "^(?!/)(?!.*\\.\\.)[A-Za-z0-9_.:/-]+$"},
                        "metadataDigest": sha256_schema(),
                        "containerDigest": sha256_schema(),
                        "requestDigest": request_digest_schema()
                    }
                },
                "VerifyGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion", "verificationReference", "requestDigest"],
                    "properties": {
                        "expectedGenerationVersion": positive_version_schema(),
                        "verificationReference": {"type": "string", "minLength": 8, "maxLength": 256, "pattern": "^[A-Za-z0-9_:-]+$"},
                        "requestDigest": request_digest_schema()
                    }
                },
                "ProfileGenerationVersionRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": request_digest_schema()
                    }
                },
                "QuarantineGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion", "requestDigest"],
                    "properties": {
                        "expectedGenerationVersion": positive_version_schema(),
                        "requestDigest": request_digest_schema()
                    }
                }
            }
        }
    })
}

fn profile_create_operation() -> Value {
    let mut responses = Map::new();
    responses.insert("200".to_owned(), mutation_response("Idempotent replay"));
    responses.insert("201".to_owned(), mutation_response("Profile created"));
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    operation(
        "createProfile",
        vec![tenant_parameter()],
        Some("ProfileCreateRequest"),
        responses,
    )
}

fn profile_get_operation() -> Value {
    let mut responses = Map::new();
    responses.insert(
        "200".to_owned(),
        json_response("ProfileProjectionDto", "Authorized Profile projection"),
    );
    add_problem_responses(&mut responses, &["404", "500", "503"]);
    operation(
        "getProfile",
        vec![tenant_parameter(), path_parameter("profileId")],
        None,
        responses,
    )
}

fn profile_mutation_operation(operation_id: &str, request_schema: &str) -> Value {
    let mut responses = Map::new();
    responses.insert("200".to_owned(), mutation_response("Mutation receipt"));
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    operation(
        operation_id,
        vec![tenant_parameter(), path_parameter("profileId")],
        Some(request_schema),
        responses,
    )
}

fn profile_grant_operation(operation_id: &str, revoke: bool) -> Value {
    let mut responses = Map::new();
    if revoke {
        responses.insert("204".to_owned(), json!({"description": "Grant revoked"}));
    } else {
        responses.insert("200".to_owned(), mutation_response("Grant mutation receipt"));
    }
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    operation(
        operation_id,
        vec![
            tenant_parameter(),
            path_parameter("profileId"),
            path_parameter("actorId"),
        ],
        Some("ProfileGrantRequest"),
        responses,
    )
}

fn generation_collection_mutation_operation(
    operation_id: &str,
    request_schema: &str,
    success_code: &str,
) -> Value {
    generation_mutation_operation(
        operation_id,
        request_schema,
        success_code,
        vec![tenant_parameter(), path_parameter("profileId")],
    )
}

fn generation_resource_mutation_operation(
    operation_id: &str,
    request_schema: &str,
    success_code: &str,
) -> Value {
    generation_mutation_operation(
        operation_id,
        request_schema,
        success_code,
        generation_resource_parameters(),
    )
}

fn generation_mutation_operation(
    operation_id: &str,
    request_schema: &str,
    success_code: &str,
    parameters: Vec<Value>,
) -> Value {
    let mut responses = Map::new();
    responses.insert(success_code.to_owned(), mutation_response("Generation mutation receipt"));
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    operation(
        operation_id,
        parameters,
        Some(request_schema),
        responses,
    )
}

fn generation_get_operation() -> Value {
    let mut responses = Map::new();
    responses.insert(
        "200".to_owned(),
        json_response("GenerationProjectionDto", "Authorized generation projection"),
    );
    add_problem_responses(&mut responses, &["404", "500", "503"]);
    operation(
        "getGeneration",
        generation_resource_parameters(),
        None,
        responses,
    )
}

fn generation_resource_parameters() -> Vec<Value> {
    vec![
        tenant_parameter(),
        path_parameter("profileId"),
        path_parameter("generationId"),
    ]
}

fn operation(
    operation_id: &str,
    parameters: Vec<Value>,
    request_schema: Option<&str>,
    responses: Map<String, Value>,
) -> Value {
    let mut value = Map::new();
    value.insert("operationId".to_owned(), Value::String(operation_id.to_owned()));
    value.insert("parameters".to_owned(), Value::Array(parameters));
    if let Some(schema) = request_schema {
        value.insert("requestBody".to_owned(), json_request(schema));
    }
    value.insert("responses".to_owned(), Value::Object(responses));
    Value::Object(value)
}

fn tenant_parameter() -> Value {
    path_parameter("tenantId")
}

fn path_parameter(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": opaque_id_schema()
    })
}

fn json_request(schema: &str) -> Value {
    json!({
        "required": true,
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn json_response(schema: &str, description: &str) -> Value {
    json!({
        "description": description,
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn mutation_response(description: &str) -> Value {
    json_response("MutationReceipt", description)
}

fn add_problem_responses(responses: &mut Map<String, Value>, codes: &[&str]) {
    for code in codes {
        responses.insert((*code).to_owned(), problem_response());
    }
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": {"type": "object"}}}
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn opaque_id_schema() -> Value {
    json!({"type": "string", "minLength": 8, "maxLength": 96})
}

fn nullable_opaque_id_schema() -> Value {
    json!({"type": "string", "nullable": true, "minLength": 8, "maxLength": 96})
}

fn positive_version_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn request_digest_schema() -> Value {
    json!({"type": "string", "minLength": 16, "maxLength": 256})
}

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileAssignmentRequest, ProfileCreateRequest, ProfileGrantRequest, ProfileProjectionDto,
        ProfileStatusDto, RegisterGenerationRequest, openapi_fragment,
    };
    use serde_json::Value;

    #[test]
    fn profile_contract_preserves_legacy_unknown_field_tolerance() {
        let create = r#"{"profileId":"profile_01JTEST","requestDigest":"request-digest-01JTEST","futureField":true}"#;
        assert!(serde_json::from_str::<ProfileCreateRequest>(create).is_ok());
        let assignment = r#"{"assignmentId":"assignment_01JTEST","clientId":"client_01JTEST","reason":"legacy-compatible","expectedProfileVersion":1,"requestDigest":"request-digest-01JTEST","futureField":true}"#;
        assert!(serde_json::from_str::<ProfileAssignmentRequest>(assignment).is_ok());
        let grant = r#"{"role":"PROFILE_VIEWER","reason":"legacy-compatible","expectedProfileVersion":1,"requestDigest":"request-digest-01JTEST","futureField":true}"#;
        assert!(serde_json::from_str::<ProfileGrantRequest>(grant).is_ok());
    }

    #[test]
    fn profile_grant_role_remains_application_validated() {
        let value = r#"{"role":"FUTURE_UNKNOWN","reason":"still reaches application validation","expectedProfileVersion":1,"requestDigest":"request-digest-01JTEST"}"#;
        assert!(serde_json::from_str::<ProfileGrantRequest>(value).is_ok());
    }

    #[test]
    fn generation_contract_rejects_unknown_fields() {
        let value = r#"{"generationId":"generation_01JTEST","objectKey":"profiles/v1/generation.enc","metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","requestDigest":"request-digest-01JTEST","futureField":true}"#;
        assert!(serde_json::from_str::<RegisterGenerationRequest>(value).is_err());
    }

    #[test]
    fn wire_enums_and_nullable_projection_are_stable() -> Result<(), Box<dyn std::error::Error>> {
        let profile = serde_json::to_value(ProfileProjectionDto {
            profile_id: "profile_01JTEST".to_owned(),
            status: ProfileStatusDto::DirtyLocal,
            version: 7,
            linked_client_id: None,
        })?;
        assert_eq!(profile["status"], "DIRTY_LOCAL");
        assert!(profile.get("linkedClientId").is_some_and(Value::is_null));
        Ok(())
    }

    #[test]
    fn fragment_covers_existing_profile_and_generation_routes_without_list_get_duplication() {
        let document = openapi_fragment();
        let profile_collection = &document["paths"]["/api/v1/tenants/{tenantId}/profiles"];
        assert!(profile_collection["post"].is_object());
        assert!(profile_collection.get("get").is_none());
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}"]["get"].is_object());
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/generations"]["post"]["parameters"].as_array().is_some_and(|parameters| parameters.len() == 2));
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine"]["post"].is_object());
        let revoke = &document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/grants/{actorId}"]["delete"];
        assert!(revoke["responses"]["204"].is_object());
        assert!(revoke["responses"].get("success_code").is_none());
    }
}