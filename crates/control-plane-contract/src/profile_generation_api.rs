use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
pub enum ProfileGrantRoleDto {
    ProfileViewer,
    ProfileOperator,
}

impl ProfileGrantRoleDto {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProfileViewer => "PROFILE_VIEWER",
            Self::ProfileOperator => "PROFILE_OPERATOR",
        }
    }
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
    pub role: ProfileGrantRoleDto,
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
                "post": profile_post_operation("createProfile", "ProfileCreateRequest", &["200", "201"])
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}": {
                "get": profile_get_operation("getProfile", "ProfileProjectionDto")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/assignment": {
                "put": profile_mutation_operation("assignProfile", "ProfileAssignmentRequest", false)
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/grants/{actorId}": {
                "put": profile_grant_operation("setProfileGrant", "PUT"),
                "delete": profile_grant_operation("revokeProfileGrant", "DELETE")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations": {
                "post": generation_mutation_operation("registerGeneration", "RegisterGenerationRequest", "201")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}": {
                "get": generation_get_operation("getGeneration", "GenerationProjectionDto")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/verify": {
                "post": generation_mutation_operation("verifyGeneration", "VerifyGenerationRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/activate": {
                "post": generation_mutation_operation("activateGeneration", "ProfileGenerationVersionRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/deactivate": {
                "post": generation_mutation_operation("deactivateGeneration", "ProfileGenerationVersionRequest", "200")
            },
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine": {
                "post": generation_mutation_operation("quarantineGeneration", "QuarantineGenerationRequest", "200")
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
                        "requestDigest": digest_schema()
                    }
                },
                "ProfileAssignmentRequest": {
                    "type": "object",
                    "required": ["assignmentId", "clientId", "reason", "expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "assignmentId": opaque_id_schema(),
                        "clientId": opaque_id_schema(),
                        "reason": bounded_text_schema(1, 512),
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": digest_schema()
                    }
                },
                "ProfileGrantRequest": {
                    "type": "object",
                    "required": ["role", "reason", "expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "role": schema_ref("ProfileGrantRoleDto"),
                        "reason": bounded_text_schema(1, 512),
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": digest_schema()
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
                        "verificationReference": {"type": "string", "nullable": true, "minLength": 1, "maxLength": 512}
                    }
                },
                "RegisterGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["generationId", "objectKey", "metadataDigest", "containerDigest", "requestDigest"],
                    "properties": {
                        "generationId": opaque_id_schema(),
                        "objectKey": bounded_text_schema(1, 1024),
                        "metadataDigest": sha256_schema(),
                        "containerDigest": sha256_schema(),
                        "requestDigest": digest_schema()
                    }
                },
                "VerifyGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion", "verificationReference", "requestDigest"],
                    "properties": {
                        "expectedGenerationVersion": positive_version_schema(),
                        "verificationReference": bounded_text_schema(1, 512),
                        "requestDigest": digest_schema()
                    }
                },
                "ProfileGenerationVersionRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedProfileVersion", "requestDigest"],
                    "properties": {
                        "expectedProfileVersion": positive_version_schema(),
                        "requestDigest": digest_schema()
                    }
                },
                "QuarantineGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion", "requestDigest"],
                    "properties": {
                        "expectedGenerationVersion": positive_version_schema(),
                        "requestDigest": digest_schema()
                    }
                }
            }
        }
    })
}

fn profile_post_operation(operation_id: &str, request_schema: &str, success_codes: &[&str]) -> Value {
    let mut responses = serde_json::Map::new();
    for code in success_codes {
        responses.insert((*code).to_owned(), mutation_response());
    }
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    json!({
        "operationId": operation_id,
        "parameters": [tenant_parameter()],
        "requestBody": json_request(request_schema),
        "responses": responses
    })
}

fn profile_get_operation(operation_id: &str, response_schema: &str) -> Value {
    json!({
        "operationId": operation_id,
        "parameters": [tenant_parameter(), path_parameter("profileId")],
        "responses": {
            "200": json_response(response_schema, "Authorized Profile projection"),
            "404": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn profile_mutation_operation(operation_id: &str, request_schema: &str, no_content: bool) -> Value {
    let success = if no_content {
        json!({"description": "Mutation completed"})
    } else {
        mutation_response()
    };
    json!({
        "operationId": operation_id,
        "parameters": [tenant_parameter(), path_parameter("profileId")],
        "requestBody": json_request(request_schema),
        "responses": {
            "200": success,
            "400": problem_response(),
            "404": problem_response(),
            "409": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn profile_grant_operation(operation_id: &str, method: &str) -> Value {
    let success = if method == "DELETE" {
        json!({"description": "Grant revoked"})
    } else {
        mutation_response()
    };
    let success_code = if method == "DELETE" { "204" } else { "200" };
    json!({
        "operationId": operation_id,
        "parameters": [tenant_parameter(), path_parameter("profileId"), path_parameter("actorId")],
        "requestBody": json_request("ProfileGrantRequest"),
        "responses": {
            success_code: success,
            "400": problem_response(),
            "404": problem_response(),
            "409": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn generation_get_operation(operation_id: &str, response_schema: &str) -> Value {
    json!({
        "operationId": operation_id,
        "parameters": generation_path_parameters(),
        "responses": {
            "200": json_response(response_schema, "Authorized generation projection"),
            "404": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn generation_mutation_operation(operation_id: &str, request_schema: &str, success_code: &str) -> Value {
    let mut responses = serde_json::Map::new();
    responses.insert(success_code.to_owned(), mutation_response());
    add_problem_responses(&mut responses, &["400", "404", "409", "500", "503"]);
    json!({
        "operationId": operation_id,
        "parameters": generation_path_parameters(),
        "requestBody": json_request(request_schema),
        "responses": responses
    })
}

fn generation_path_parameters() -> Vec<Value> {
    vec![
        tenant_parameter(),
        path_parameter("profileId"),
        path_parameter("generationId"),
    ]
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

fn mutation_response() -> Value {
    json_response("MutationReceipt", "Mutation receipt")
}

fn add_problem_responses(responses: &mut serde_json::Map<String, Value>, codes: &[&str]) {
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

fn digest_schema() -> Value {
    json!({"type": "string", "minLength": 8, "maxLength": 256})
}

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn bounded_text_schema(minimum: u64, maximum: u64) -> Value {
    json!({"type": "string", "minLength": minimum, "maxLength": maximum})
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileCreateRequest, ProfileGrantRequest, ProfileGrantRoleDto, ProfileProjectionDto,
        ProfileStatusDto, RegisterGenerationRequest, openapi_fragment,
    };

    #[test]
    fn profile_contract_preserves_legacy_unknown_field_tolerance() {
        let value = r#"{"profileId":"profile_01JTEST","requestDigest":"digest_01JTEST","futureField":true}"#;
        assert!(serde_json::from_str::<ProfileCreateRequest>(value).is_ok());
    }

    #[test]
    fn generation_contract_rejects_unknown_fields() {
        let value = r#"{"generationId":"generation_01JTEST","objectKey":"profiles/generation.enc","metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","requestDigest":"digest_01JTEST","futureField":true}"#;
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

        let grant = serde_json::to_value(ProfileGrantRequest {
            role: ProfileGrantRoleDto::ProfileOperator,
            reason: "operator access".to_owned(),
            expected_profile_version: 7,
            request_digest: "digest_01JTEST".to_owned(),
        })?;
        assert_eq!(grant["role"], "PROFILE_OPERATOR");
        Ok(())
    }

    #[test]
    fn fragment_covers_existing_profile_and_generation_routes_without_list_get_duplication() {
        let document = openapi_fragment();
        let profile_collection = &document["paths"]["/api/v1/tenants/{tenantId}/profiles"];
        assert!(profile_collection["post"].is_object());
        assert!(profile_collection.get("get").is_none());
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}"]["get"].is_object());
        assert!(document["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/generations/{generationId}/quarantine"]["post"].is_object());
    }
}