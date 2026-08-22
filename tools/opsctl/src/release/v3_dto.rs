use crate::canonical::{DEFAULT_MAX_JSON_DEPTH, parse_strict_json_with_limits};
use opsctl_core::release as core;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// Release Set v3 semantic identity DTO admission budget.
///
/// This is an external representation limit, not a pure-core semantic invariant.
pub const MAX_RELEASE_SET_V3_BYTES: usize = 4 * 1024 * 1024;
pub const RELEASE_SET_V3_SCHEMA_VERSION: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSetV3DtoErrorKind {
    ByteBudget,
    Utf8,
    JsonAdmission,
    DtoShape,
    Version,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetV3DtoError {
    kind: ReleaseSetV3DtoErrorKind,
    message: String,
}

impl ReleaseSetV3DtoError {
    fn new(kind: ReleaseSetV3DtoErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ReleaseSetV3DtoErrorKind {
        self.kind
    }
}

impl Display for ReleaseSetV3DtoError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseSetV3DtoError {}

/// Versioned external representation of the Release Set v3 semantic identity payload.
///
/// `release_set_id` and presentation-only fields are intentionally not part of this DTO: R3b
/// binds the canonical bytes of this payload to the external content address. The pure core owns
/// cross-section semantics; this DTO owns only external field names and transport shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSetV3Dto {
    pub schema_version: u64,
    pub source: ReleaseSetSourceDto,
    pub components: BTreeMap<String, ReleaseComponentIdentityDto>,
    pub contracts: ContractsIdentityDto,
    pub protocols: ProtocolIdentityDto,
    pub schemas: SchemaIdentityDto,
    pub runtime_compatibility: RuntimeCompatibilityIdentityDto,
    pub capability_profile_compatibility: Vec<String>,
    pub build_provenance: BuildProvenanceIdentityDto,
    pub artifact_inventory: Vec<ArtifactIdentityDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSetSourceDto {
    pub repository: String,
    pub commit_sha: String,
    pub accepted_main: bool,
    pub accepted_main_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseComponentIdentityDto {
    pub component_id: String,
    pub release_id: String,
    pub source_commit_sha: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub component_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentityDto {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceFileIdentityDto {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractsIdentityDto {
    pub files: Vec<ProvenanceFileIdentityDto>,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolIdentityDto {
    pub public_api_contract_sha256: String,
    pub camouhost_ipc_version: u64,
    pub profile_bridge_protocol_version: u64,
    pub resolver_protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaCompatibilityWindowDto {
    pub database_component: String,
    pub target_schema_revision: String,
    pub supported_schema_min: String,
    pub supported_schema_max: String,
    pub migration_history_digest: String,
    pub compatibility_policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaIdentityDto {
    pub d1_repository_identity_sha256: String,
    pub catalog: SchemaCompatibilityWindowDto,
    pub resolver: SchemaCompatibilityWindowDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCompatibilityIdentityDto {
    pub runtime_lock_sha256: String,
    pub runtime_role: String,
    pub profile_format: String,
    pub browser_identity_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProvenanceIdentityDto {
    pub cargo_lock_sha256: String,
    pub rust_toolchain_sha256: String,
    pub frontend_lock_sha256: String,
    pub release_architecture_sha256: String,
}

/// Strictly admit external Release Set v3 identity bytes into the versioned DTO.
///
/// Duplicate members are rejected during the first parse, before Serde can collapse an object
/// into a map. Unknown fields are rejected by every fixed DTO object. Numeric admission is bounded
/// by the shared strict JSON adapter before typed integer decoding occurs.
pub fn parse_release_set_v3_dto(bytes: &[u8]) -> Result<ReleaseSetV3Dto, ReleaseSetV3DtoError> {
    if bytes.len() > MAX_RELEASE_SET_V3_BYTES {
        return Err(ReleaseSetV3DtoError::new(
            ReleaseSetV3DtoErrorKind::ByteBudget,
            format!(
                "Release Set v3 JSON exceeds byte budget: observed={} max={MAX_RELEASE_SET_V3_BYTES}",
                bytes.len()
            ),
        ));
    }
    let input = std::str::from_utf8(bytes).map_err(|error| {
        ReleaseSetV3DtoError::new(
            ReleaseSetV3DtoErrorKind::Utf8,
            format!("Release Set v3 input is not UTF-8: {error}"),
        )
    })?;
    let value = parse_strict_json_with_limits(
        input,
        MAX_RELEASE_SET_V3_BYTES,
        DEFAULT_MAX_JSON_DEPTH,
    )
    .map_err(|error| {
        ReleaseSetV3DtoError::new(
            ReleaseSetV3DtoErrorKind::JsonAdmission,
            format!("Release Set v3 strict JSON admission failed: {error}"),
        )
    })?;
    let dto: ReleaseSetV3Dto = serde_json::from_value(value).map_err(|error| {
        ReleaseSetV3DtoError::new(
            ReleaseSetV3DtoErrorKind::DtoShape,
            format!("Release Set v3 DTO shape invalid: {error}"),
        )
    })?;
    if dto.schema_version != RELEASE_SET_V3_SCHEMA_VERSION
        || dto.schema_version != core::ReleaseSetSchemaVersion::CURRENT.number()
    {
        return Err(ReleaseSetV3DtoError::new(
            ReleaseSetV3DtoErrorKind::Version,
            format!(
                "unsupported Release Set schema_version: {}; current version is {}",
                dto.schema_version, RELEASE_SET_V3_SCHEMA_VERSION
            ),
        ));
    }
    Ok(dto)
}

/// Strict external bytes -> versioned DTO -> one pure semantic owner.
pub fn decode_release_set_v3(bytes: &[u8]) -> Result<core::ReleaseSetV3, ReleaseSetV3DtoError> {
    parse_release_set_v3_dto(bytes)?.into_core()
}

impl ReleaseSetV3Dto {
    pub fn into_core(self) -> Result<core::ReleaseSetV3, ReleaseSetV3DtoError> {
        if self.schema_version != RELEASE_SET_V3_SCHEMA_VERSION
            || self.schema_version != core::ReleaseSetSchemaVersion::CURRENT.number()
        {
            return Err(ReleaseSetV3DtoError::new(
                ReleaseSetV3DtoErrorKind::Version,
                format!(
                    "unsupported Release Set schema_version: {}; current version is {}",
                    self.schema_version, RELEASE_SET_V3_SCHEMA_VERSION
                ),
            ));
        }
        core::ReleaseSetV3::new(core::ReleaseSetV3Parts {
            source: self.source.into_core(),
            components: self
                .components
                .into_iter()
                .map(|(key, value)| (key, value.into_core()))
                .collect(),
            contracts: self.contracts.into_core(),
            protocols: self.protocols.into_core(),
            schemas: self.schemas.into_core(),
            runtime_compatibility: self.runtime_compatibility.into_core(),
            capability_profile_compatibility: self.capability_profile_compatibility,
            build_provenance: self.build_provenance.into_core(),
            artifact_inventory: self
                .artifact_inventory
                .into_iter()
                .map(ArtifactIdentityDto::into_core)
                .collect(),
        })
        .map_err(|error| {
            ReleaseSetV3DtoError::new(
                ReleaseSetV3DtoErrorKind::Semantic,
                format!("Release Set v3 semantic validation failed: {error}"),
            )
        })
    }
}

impl ReleaseSetSourceDto {
    fn into_core(self) -> core::ReleaseSetSource {
        core::ReleaseSetSource {
            repository: self.repository,
            commit_sha: self.commit_sha,
            accepted_main: self.accepted_main,
            accepted_main_evidence_sha256: self.accepted_main_evidence_sha256,
        }
    }
}

impl ReleaseComponentIdentityDto {
    fn into_core(self) -> core::ReleaseComponentIdentity {
        core::ReleaseComponentIdentity {
            component_id: self.component_id,
            release_id: self.release_id,
            source_commit_sha: self.source_commit_sha,
            artifact_path: self.artifact_path,
            artifact_sha256: self.artifact_sha256,
            artifact_size_bytes: self.artifact_size_bytes,
            component_manifest_sha256: self.component_manifest_sha256,
        }
    }
}

impl ArtifactIdentityDto {
    fn into_core(self) -> core::ArtifactIdentity {
        core::ArtifactIdentity {
            path: self.path,
            sha256: self.sha256,
            size_bytes: self.size_bytes,
            kind: self.kind,
        }
    }
}

impl ProvenanceFileIdentityDto {
    fn into_core(self) -> core::ProvenanceFileIdentity {
        core::ProvenanceFileIdentity {
            path: self.path,
            sha256: self.sha256,
            size_bytes: self.size_bytes,
        }
    }
}

impl ContractsIdentityDto {
    fn into_core(self) -> core::ContractsIdentity {
        core::ContractsIdentity {
            files: self
                .files
                .into_iter()
                .map(ProvenanceFileIdentityDto::into_core)
                .collect(),
            sha256: self.sha256,
        }
    }
}

impl ProtocolIdentityDto {
    fn into_core(self) -> core::ProtocolIdentity {
        core::ProtocolIdentity {
            public_api_contract_sha256: self.public_api_contract_sha256,
            camouhost_ipc_version: self.camouhost_ipc_version,
            profile_bridge_protocol_version: self.profile_bridge_protocol_version,
            resolver_protocol: self.resolver_protocol,
        }
    }
}

impl SchemaCompatibilityWindowDto {
    fn into_core(self) -> core::SchemaCompatibilityWindow {
        core::SchemaCompatibilityWindow {
            database_component: self.database_component,
            target_schema_revision: self.target_schema_revision,
            supported_schema_min: self.supported_schema_min,
            supported_schema_max: self.supported_schema_max,
            migration_history_digest: self.migration_history_digest,
            compatibility_policy_digest: self.compatibility_policy_digest,
        }
    }
}

impl SchemaIdentityDto {
    fn into_core(self) -> core::SchemaIdentity {
        core::SchemaIdentity {
            d1_repository_identity_sha256: self.d1_repository_identity_sha256,
            catalog: self.catalog.into_core(),
            resolver: self.resolver.into_core(),
        }
    }
}

impl RuntimeCompatibilityIdentityDto {
    fn into_core(self) -> core::RuntimeCompatibilityIdentity {
        core::RuntimeCompatibilityIdentity {
            runtime_lock_sha256: self.runtime_lock_sha256,
            runtime_role: self.runtime_role,
            profile_format: self.profile_format,
            browser_identity_policy: self.browser_identity_policy,
        }
    }
}

impl BuildProvenanceIdentityDto {
    fn into_core(self) -> core::BuildProvenanceIdentity {
        core::BuildProvenanceIdentity {
            cargo_lock_sha256: self.cargo_lock_sha256,
            rust_toolchain_sha256: self.rust_toolchain_sha256,
            frontend_lock_sha256: self.frontend_lock_sha256,
            release_architecture_sha256: self.release_architecture_sha256,
        }
    }
}

impl From<&core::ReleaseSetV3> for ReleaseSetV3Dto {
    fn from(value: &core::ReleaseSetV3) -> Self {
        Self {
            schema_version: value.schema_version.number(),
            source: ReleaseSetSourceDto::from(&value.source),
            components: value
                .components
                .iter()
                .map(|(key, component)| (key.clone(), ReleaseComponentIdentityDto::from(component)))
                .collect(),
            contracts: ContractsIdentityDto::from(&value.contracts),
            protocols: ProtocolIdentityDto::from(&value.protocols),
            schemas: SchemaIdentityDto::from(&value.schemas),
            runtime_compatibility: RuntimeCompatibilityIdentityDto::from(
                &value.runtime_compatibility,
            ),
            capability_profile_compatibility: value.capability_profile_compatibility.clone(),
            build_provenance: BuildProvenanceIdentityDto::from(&value.build_provenance),
            artifact_inventory: value
                .artifact_inventory
                .iter()
                .map(ArtifactIdentityDto::from)
                .collect(),
        }
    }
}

impl From<&core::ReleaseSetSource> for ReleaseSetSourceDto {
    fn from(value: &core::ReleaseSetSource) -> Self {
        Self {
            repository: value.repository.clone(),
            commit_sha: value.commit_sha.clone(),
            accepted_main: value.accepted_main,
            accepted_main_evidence_sha256: value.accepted_main_evidence_sha256.clone(),
        }
    }
}

impl From<&core::ReleaseComponentIdentity> for ReleaseComponentIdentityDto {
    fn from(value: &core::ReleaseComponentIdentity) -> Self {
        Self {
            component_id: value.component_id.clone(),
            release_id: value.release_id.clone(),
            source_commit_sha: value.source_commit_sha.clone(),
            artifact_path: value.artifact_path.clone(),
            artifact_sha256: value.artifact_sha256.clone(),
            artifact_size_bytes: value.artifact_size_bytes,
            component_manifest_sha256: value.component_manifest_sha256.clone(),
        }
    }
}

impl From<&core::ArtifactIdentity> for ArtifactIdentityDto {
    fn from(value: &core::ArtifactIdentity) -> Self {
        Self {
            path: value.path.clone(),
            sha256: value.sha256.clone(),
            size_bytes: value.size_bytes,
            kind: value.kind.clone(),
        }
    }
}

impl From<&core::ProvenanceFileIdentity> for ProvenanceFileIdentityDto {
    fn from(value: &core::ProvenanceFileIdentity) -> Self {
        Self {
            path: value.path.clone(),
            sha256: value.sha256.clone(),
            size_bytes: value.size_bytes,
        }
    }
}

impl From<&core::ContractsIdentity> for ContractsIdentityDto {
    fn from(value: &core::ContractsIdentity) -> Self {
        Self {
            files: value
                .files
                .iter()
                .map(ProvenanceFileIdentityDto::from)
                .collect(),
            sha256: value.sha256.clone(),
        }
    }
}

impl From<&core::ProtocolIdentity> for ProtocolIdentityDto {
    fn from(value: &core::ProtocolIdentity) -> Self {
        Self {
            public_api_contract_sha256: value.public_api_contract_sha256.clone(),
            camouhost_ipc_version: value.camouhost_ipc_version,
            profile_bridge_protocol_version: value.profile_bridge_protocol_version,
            resolver_protocol: value.resolver_protocol.clone(),
        }
    }
}

impl From<&core::SchemaCompatibilityWindow> for SchemaCompatibilityWindowDto {
    fn from(value: &core::SchemaCompatibilityWindow) -> Self {
        Self {
            database_component: value.database_component.clone(),
            target_schema_revision: value.target_schema_revision.clone(),
            supported_schema_min: value.supported_schema_min.clone(),
            supported_schema_max: value.supported_schema_max.clone(),
            migration_history_digest: value.migration_history_digest.clone(),
            compatibility_policy_digest: value.compatibility_policy_digest.clone(),
        }
    }
}

impl From<&core::SchemaIdentity> for SchemaIdentityDto {
    fn from(value: &core::SchemaIdentity) -> Self {
        Self {
            d1_repository_identity_sha256: value.d1_repository_identity_sha256.clone(),
            catalog: SchemaCompatibilityWindowDto::from(&value.catalog),
            resolver: SchemaCompatibilityWindowDto::from(&value.resolver),
        }
    }
}

impl From<&core::RuntimeCompatibilityIdentity> for RuntimeCompatibilityIdentityDto {
    fn from(value: &core::RuntimeCompatibilityIdentity) -> Self {
        Self {
            runtime_lock_sha256: value.runtime_lock_sha256.clone(),
            runtime_role: value.runtime_role.clone(),
            profile_format: value.profile_format.clone(),
            browser_identity_policy: value.browser_identity_policy.clone(),
        }
    }
}

impl From<&core::BuildProvenanceIdentity> for BuildProvenanceIdentityDto {
    fn from(value: &core::BuildProvenanceIdentity) -> Self {
        Self {
            cargo_lock_sha256: value.cargo_lock_sha256.clone(),
            rust_toolchain_sha256: value.rust_toolchain_sha256.clone(),
            frontend_lock_sha256: value.frontend_lock_sha256.clone(),
            release_architecture_sha256: value.release_architecture_sha256.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactIdentityDto, BuildProvenanceIdentityDto, ContractsIdentityDto,
        MAX_RELEASE_SET_V3_BYTES, ProtocolIdentityDto, ProvenanceFileIdentityDto,
        RELEASE_SET_V3_SCHEMA_VERSION, ReleaseComponentIdentityDto, ReleaseSetSourceDto,
        ReleaseSetV3Dto, ReleaseSetV3DtoError, ReleaseSetV3DtoErrorKind,
        RuntimeCompatibilityIdentityDto, SchemaCompatibilityWindowDto, SchemaIdentityDto,
        decode_release_set_v3, parse_release_set_v3_dto,
    };
    use opsctl_core::release as core;
    use std::collections::BTreeMap;

    fn require_error<T>(
        result: Result<T, ReleaseSetV3DtoError>,
        label: &str,
    ) -> Result<ReleaseSetV3DtoError, String> {
        match result {
            Err(error) => Ok(error),
            Ok(_) => Err(label.to_owned()),
        }
    }

    fn digest(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn component(
        component_id: &str,
        path: &str,
        sha256: &str,
        size_bytes: u64,
    ) -> ReleaseComponentIdentityDto {
        ReleaseComponentIdentityDto {
            component_id: component_id.to_owned(),
            release_id: format!("{component_id}-release-v1"),
            source_commit_sha: "1111111111111111111111111111111111111111".to_owned(),
            artifact_path: path.to_owned(),
            artifact_sha256: sha256.to_owned(),
            artifact_size_bytes: size_bytes,
            component_manifest_sha256: digest('e'),
        }
    }

    fn schema_window(component: &str) -> SchemaCompatibilityWindowDto {
        SchemaCompatibilityWindowDto {
            database_component: component.to_owned(),
            target_schema_revision: "0001".to_owned(),
            supported_schema_min: "0001".to_owned(),
            supported_schema_max: "0002".to_owned(),
            migration_history_digest: digest('c'),
            compatibility_policy_digest: digest('d'),
        }
    }

    fn valid_dto() -> ReleaseSetV3Dto {
        let control_sha = digest('1');
        let resolver_sha = digest('2');
        let runtime_sha = digest('3');
        let mut components = BTreeMap::new();
        components.insert(
            "control_plane".to_owned(),
            component(
                "control_plane",
                "components/control-plane.tar",
                &control_sha,
                11,
            ),
        );
        components.insert(
            "runtime_bundle".to_owned(),
            component(
                "runtime_bundle",
                "components/runtime-bundle.tar",
                &runtime_sha,
                13,
            ),
        );
        components.insert(
            "secret_resolver".to_owned(),
            component(
                "secret_resolver",
                "components/secret-resolver.tar",
                &resolver_sha,
                12,
            ),
        );

        ReleaseSetV3Dto {
            schema_version: RELEASE_SET_V3_SCHEMA_VERSION,
            source: ReleaseSetSourceDto {
                repository: core::EXPECTED_REPOSITORY.to_owned(),
                commit_sha: "1111111111111111111111111111111111111111".to_owned(),
                accepted_main: true,
                accepted_main_evidence_sha256: digest('a'),
            },
            components,
            contracts: ContractsIdentityDto {
                files: vec![ProvenanceFileIdentityDto {
                    path: "contracts/public-api.json".to_owned(),
                    sha256: digest('b'),
                    size_bytes: 7,
                }],
                sha256: digest('c'),
            },
            protocols: ProtocolIdentityDto {
                public_api_contract_sha256: digest('d'),
                camouhost_ipc_version: 1,
                profile_bridge_protocol_version: 1,
                resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
            },
            schemas: SchemaIdentityDto {
                d1_repository_identity_sha256: digest('e'),
                catalog: schema_window("catalog"),
                resolver: schema_window("resolver"),
            },
            runtime_compatibility: RuntimeCompatibilityIdentityDto {
                runtime_lock_sha256: digest('f'),
                runtime_role: "camouhost".to_owned(),
                profile_format: "profile-v1".to_owned(),
                browser_identity_policy: "browser-identity-v1".to_owned(),
            },
            capability_profile_compatibility: vec!["production-core-v1".to_owned()],
            build_provenance: BuildProvenanceIdentityDto {
                cargo_lock_sha256: digest('1'),
                rust_toolchain_sha256: digest('2'),
                frontend_lock_sha256: digest('3'),
                release_architecture_sha256: digest('4'),
            },
            artifact_inventory: vec![
                ArtifactIdentityDto {
                    path: "components/control-plane.tar".to_owned(),
                    sha256: control_sha,
                    size_bytes: 11,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/runtime-bundle.tar".to_owned(),
                    sha256: runtime_sha,
                    size_bytes: 13,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/secret-resolver.tar".to_owned(),
                    sha256: resolver_sha,
                    size_bytes: 12,
                    kind: "component".to_owned(),
                },
            ],
        }
    }

    fn valid_bytes() -> Result<Vec<u8>, String> {
        serde_json::to_vec(&valid_dto()).map_err(|error| error.to_string())
    }

    #[test]
    fn strict_v3_dto_converts_directly_to_and_from_pure_core() -> Result<(), String> {
        let dto = valid_dto();
        let model = dto
            .clone()
            .into_core()
            .map_err(|error| error.to_string())?;
        assert_eq!(model.schema_version, core::ReleaseSetSchemaVersion::V3);
        assert_eq!(ReleaseSetV3Dto::from(&model), dto);

        let decoded = decode_release_set_v3(&valid_bytes()?).map_err(|error| error.to_string())?;
        assert_eq!(decoded, model);
        Ok(())
    }

    #[test]
    fn duplicate_members_fail_closed_before_dto_decode() -> Result<(), String> {
        let text = String::from_utf8(valid_bytes()?).map_err(|error| error.to_string())?;
        let input = text.replacen(
            "{",
            &format!("{{\"schema_version\":{RELEASE_SET_V3_SCHEMA_VERSION},"),
            1,
        );
        let error = require_error(
            parse_release_set_v3_dto(input.as_bytes()),
            "duplicate root member was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::JsonAdmission);

        let nested = br#"{"outer":{"same":1,"same":2}}"#;
        let error = require_error(
            parse_release_set_v3_dto(nested),
            "duplicate nested member was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::JsonAdmission);
        Ok(())
    }

    #[test]
    fn unknown_fields_and_version_mismatch_fail_closed() -> Result<(), String> {
        let text = String::from_utf8(valid_bytes()?).map_err(|error| error.to_string())?;
        let unknown = text.replacen("{", "{\"unexpected\":true,", 1);
        let error = require_error(
            parse_release_set_v3_dto(unknown.as_bytes()),
            "unknown root field was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::DtoShape);

        let wrong_version = text.replacen(
            &format!("\"schema_version\":{RELEASE_SET_V3_SCHEMA_VERSION}"),
            "\"schema_version\":4",
            1,
        );
        let error = require_error(
            parse_release_set_v3_dto(wrong_version.as_bytes()),
            "wrong version was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::Version);
        Ok(())
    }

    #[test]
    fn byte_depth_and_i_json_number_budgets_fail_closed() -> Result<(), String> {
        let oversized = vec![b' '; MAX_RELEASE_SET_V3_BYTES + 1];
        let error = require_error(
            parse_release_set_v3_dto(&oversized),
            "oversized document was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::ByteBudget);

        let nested = format!(
            "{{\"probe\":{}0{}}}",
            "[".repeat(super::DEFAULT_MAX_JSON_DEPTH + 1),
            "]".repeat(super::DEFAULT_MAX_JSON_DEPTH + 1)
        );
        let error = require_error(
            parse_release_set_v3_dto(nested.as_bytes()),
            "over-depth document was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::JsonAdmission);

        let text = String::from_utf8(valid_bytes()?).map_err(|error| error.to_string())?;
        let unsafe_integer = text.replacen(
            "\"artifact_size_bytes\":11",
            "\"artifact_size_bytes\":9007199254740992",
            1,
        );
        let error = require_error(
            parse_release_set_v3_dto(unsafe_integer.as_bytes()),
            "unsafe integer was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::JsonAdmission);
        Ok(())
    }

    #[test]
    fn semantic_rules_are_owned_only_by_pure_core() -> Result<(), String> {
        let mut dto = valid_dto();
        dto.source.accepted_main = false;
        let bytes = serde_json::to_vec(&dto).map_err(|error| error.to_string())?;
        let error = require_error(
            decode_release_set_v3(&bytes),
            "semantic violation was accepted",
        )?;
        assert_eq!(error.kind(), ReleaseSetV3DtoErrorKind::Semantic);
        Ok(())
    }
}
