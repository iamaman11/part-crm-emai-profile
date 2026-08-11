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
        "paths": {},
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
                "ProfileCreateRequestDto": {
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
                "ProfileGrantRequestDto": {
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
    fn fragment_is_schema_authority_without_duplicate_operation_paths() {
        let document = openapi_fragment();
        assert_eq!(document["paths"], json!({}));
        let schemas = &document["components"]["schemas"];
        for name in [
            "ProfileProjectionDto",
            "ProfileCreateRequestDto",
            "ProfileAssignmentRequest",
            "ProfileGrantRequestDto",
            "GenerationProjectionDto",
            "RegisterGenerationRequest",
            "VerifyGenerationRequest",
            "ProfileGenerationVersionRequest",
            "QuarantineGenerationRequest",
        ] {
            assert!(schemas[name].is_object(), "missing schema {name}");
        }
    }
}
