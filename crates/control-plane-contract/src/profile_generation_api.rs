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
pub const BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-successor/upload-capability";
pub const BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-successor/commit";

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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileCreateRequest {
    pub profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileGrantRequest {
    pub role: String,
    pub reason: String,
    pub expected_profile_version: u64,
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyGenerationRequest {
    pub expected_generation_version: u64,
    pub verification_reference: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileGenerationVersionRequest {
    pub expected_profile_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuarantineGenerationRequest {
    pub expected_generation_version: u64,
}

/// Canonical machine request for both immutable successor upload preparation and final commit.
/// Machine identity, actor, tenant/profile path identity, optimistic profile version and coordinator
/// version/sequence are intentionally server-owned and therefore absent from this DTO.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeProfileGenerationSuccessorRequest {
    base_generation_id: String,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl BridgeProfileGenerationSuccessorRequest {
    #[must_use]
    pub fn base_generation_id(&self) -> &str {
        &self.base_generation_id
    }

    #[must_use]
    pub fn generation_id(&self) -> &str {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }

    #[must_use]
    pub fn coordinator_session_id(&self) -> &str {
        &self.coordinator_session_id
    }

    #[must_use]
    pub fn coordinator_fencing_token(&self) -> &str {
        &self.coordinator_fencing_token
    }

    #[must_use]
    pub const fn coordinator_epoch(&self) -> u64 {
        self.coordinator_epoch
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BridgeGenerationUploadHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationUploadCapabilityResponse {
    pub state: String,
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Vec<BridgeGenerationUploadHeader>,
    pub expires_seconds: Option<u32>,
}

impl BridgeGenerationUploadCapabilityResponse {
    #[must_use]
    pub fn verified() -> Self {
        Self {
            state: "verified".to_owned(),
            method: None,
            url: None,
            headers: Vec::new(),
            expires_seconds: None,
        }
    }

    #[must_use]
    pub fn upload_required(url: &str, headers: &[(String, String)], expires_seconds: u32) -> Self {
        Self {
            state: "uploadRequired".to_owned(),
            method: Some("PUT".to_owned()),
            url: Some(url.to_owned()),
            headers: headers
                .iter()
                .map(|(name, value)| BridgeGenerationUploadHeader {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
            expires_seconds: Some(expires_seconds),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BridgeGenerationSuccessorCommitOutcomeDto {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationSuccessorCommitResponse {
    pub outcome: BridgeGenerationSuccessorCommitOutcomeDto,
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
                    "additionalProperties": false,
                    "required": ["profileId"],
                    "properties": {"profileId": opaque_id_schema()}
                },
                "ProfileGrantRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "reason", "expectedProfileVersion"],
                    "properties": {
                        "role": schema_ref("ProfileGrantRoleDto"),
                        "reason": {"type": "string"},
                        "expectedProfileVersion": positive_version_schema()
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
                        "verificationReference": nullable_verification_reference_schema()
                    }
                },
                "RegisterGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["generationId", "objectKey", "metadataDigest", "containerDigest"],
                    "properties": {
                        "generationId": opaque_id_schema(),
                        "objectKey": {"type": "string", "minLength": 16, "maxLength": 512, "pattern": "^(?!/)(?!.*\\.\\.)[A-Za-z0-9_.:/-]+$"},
                        "metadataDigest": sha256_schema(),
                        "containerDigest": sha256_schema()
                    }
                },
                "VerifyGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion", "verificationReference"],
                    "properties": {
                        "expectedGenerationVersion": positive_version_schema(),
                        "verificationReference": {"type": "string", "minLength": 8, "maxLength": 256, "pattern": "^[A-Za-z0-9_:-]+$"}
                    }
                },
                "ProfileGenerationVersionRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedProfileVersion"],
                    "properties": {"expectedProfileVersion": positive_version_schema()}
                },
                "QuarantineGenerationRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedGenerationVersion"],
                    "properties": {"expectedGenerationVersion": positive_version_schema()}
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
    json!({"oneOf": [{"type": "string", "minLength": 8, "maxLength": 96}, {"type": "null"}]})
}

fn nullable_verification_reference_schema() -> Value {
    json!({"oneOf": [{"type": "string", "minLength": 8, "maxLength": 256, "pattern": "^[A-Za-z0-9_:-]+$"}, {"type": "null"}]})
}

fn positive_version_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
        ProfileCreateRequest, ProfileGrantRequest, ProfileProjectionDto, ProfileStatusDto,
        RegisterGenerationRequest, openapi_fragment,
    };
    use serde_json::{Value, json};

    #[test]
    fn profile_mutations_are_strict_and_reject_legacy_request_digest() {
        assert!(
            serde_json::from_str::<ProfileCreateRequest>(r#"{"profileId":"profile_01JTEST"}"#)
                .is_ok()
        );
        for value in [
            r#"{"profileId":"profile_01JTEST","requestDigest":"legacy"}"#,
            r#"{"role":"PROFILE_VIEWER","reason":"grant","expectedProfileVersion":1,"requestDigest":"legacy"}"#,
        ] {
            assert!(serde_json::from_str::<serde_json::Value>(value).is_ok());
        }
        assert!(
            serde_json::from_str::<ProfileCreateRequest>(
                r#"{"profileId":"profile_01JTEST","requestDigest":"legacy"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<ProfileGrantRequest>(r#"{"role":"PROFILE_VIEWER","reason":"grant","expectedProfileVersion":1,"requestDigest":"legacy"}"#).is_err());
    }

    #[test]
    fn profile_grant_role_remains_application_validated() {
        let value = r#"{"role":"FUTURE_UNKNOWN","reason":"still reaches application validation","expectedProfileVersion":1}"#;
        assert!(serde_json::from_str::<ProfileGrantRequest>(value).is_ok());
    }

    #[test]
    fn generation_contract_keeps_artifact_digests_but_rejects_command_digest() {
        let valid = r#"{"generationId":"generation_01JTEST","objectKey":"profiles/v1/generation.enc","metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;
        assert!(serde_json::from_str::<RegisterGenerationRequest>(valid).is_ok());
        let legacy = r#"{"generationId":"generation_01JTEST","objectKey":"profiles/v1/generation.enc","metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","requestDigest":"legacy"}"#;
        assert!(serde_json::from_str::<RegisterGenerationRequest>(legacy).is_err());
    }

    #[test]
    fn bridge_successor_request_is_metadata_only_and_rejects_server_authority_fields() {
        let base = r#"{
            "baseGenerationId":"generation_bridge_base_01",
            "generationId":"generation_bridge_next_01",
            "objectKey":"tenants/tenant_bridge_01/profiles/profile_bridge_01/generations/generation_bridge_next_01.bpgc",
            "metadataDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "containerDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "containerBytes":4096,
            "coordinatorSessionId":"session_bridge_01",
            "coordinatorFencingToken":"fence_bridge_01",
            "coordinatorEpoch":3
        }"#;
        assert!(serde_json::from_str::<BridgeProfileGenerationSuccessorRequest>(base).is_ok());
        for forbidden in [
            "tenantId",
            "actorId",
            "profileId",
            "deviceId",
            "expectedProfileVersion",
            "coordinatorVersion",
            "coordinatorSequence",
            "observedAt",
            "clientClock",
            "ciphertext",
            "container",
            "rawDek",
        ] {
            let tampered = base.replacen('}', &format!(r#", "{forbidden}": 1}}"#), 1);
            assert!(
                serde_json::from_str::<BridgeProfileGenerationSuccessorRequest>(&tampered).is_err(),
                "forbidden Bridge successor field unexpectedly accepted: {forbidden}"
            );
        }
    }

    #[test]
    fn verified_upload_response_contains_no_capability() -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_value(BridgeGenerationUploadCapabilityResponse::verified())?;
        assert_eq!(json["state"], "verified");
        assert!(json["method"].is_null());
        assert!(json["url"].is_null());
        assert_eq!(json["headers"], serde_json::json!([]));
        assert!(json["expiresSeconds"].is_null());
        Ok(())
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
            "ProfileGrantRequestDto",
            "GenerationProjectionDto",
            "RegisterGenerationRequest",
            "VerifyGenerationRequest",
            "ProfileGenerationVersionRequest",
            "QuarantineGenerationRequest",
        ] {
            assert!(schemas[name].is_object(), "missing schema {name}");
        }
        assert!(
            schemas["RegisterGenerationRequest"]["properties"]
                .get("metadataDigest")
                .is_some()
        );
        assert!(
            schemas["RegisterGenerationRequest"]["properties"]
                .get("requestDigest")
                .is_none()
        );
    }
}
