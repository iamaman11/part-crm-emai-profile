use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_ID_PREFIX_V3: &str = "release-set-v3-sha256-";
pub const EXPECTED_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSetSchemaVersion {
    V3,
}

impl ReleaseSetSchemaVersion {
    pub const CURRENT: Self = Self::V3;

    #[must_use]
    pub const fn number(self) -> u64 {
        match self {
            Self::V3 => 3,
        }
    }

    #[must_use]
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::V3 => RELEASE_SET_ID_PREFIX_V3,
        }
    }

    pub fn from_number(value: u64) -> Result<Self, ReleaseModelError> {
        match value {
            3 => Ok(Self::V3),
            other => Err(ReleaseModelError::new(format!(
                "unsupported release-set schema_version: {other}; current contract is v3"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityDecision {
    Compatible,
    Incompatible,
    Unknown,
}

impl CompatibilityDecision {
    pub fn parse(value: &str) -> Result<Self, ReleaseModelError> {
        match value {
            "COMPATIBLE" => Ok(Self::Compatible),
            "INCOMPATIBLE" => Ok(Self::Incompatible),
            "UNKNOWN" => Ok(Self::Unknown),
            other => Err(ReleaseModelError::new(format!(
                "unsupported compatibility decision: {other}"
            ))),
        }
    }

    #[must_use]
    pub const fn is_compatible(self) -> bool {
        matches!(self, Self::Compatible)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetSource {
    pub repository: String,
    pub commit_sha: String,
    pub accepted_main: bool,
    pub accepted_main_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseComponentIdentity {
    pub component_id: String,
    pub release_id: String,
    pub source_commit_sha: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub component_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceFileIdentity {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractsIdentity {
    pub files: Vec<ProvenanceFileIdentity>,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolIdentity {
    pub public_api_contract_sha256: String,
    pub camouhost_ipc_version: u64,
    pub profile_bridge_protocol_version: u64,
    pub resolver_protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCompatibilityWindow {
    pub database_component: String,
    pub target_schema_revision: String,
    pub supported_schema_min: String,
    pub supported_schema_max: String,
    pub migration_history_digest: String,
    pub compatibility_policy_digest: String,
}

impl SchemaCompatibilityWindow {
    #[must_use]
    pub fn supports(&self, revision: &str) -> bool {
        revision >= self.supported_schema_min.as_str()
            && revision <= self.supported_schema_max.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIdentity {
    pub d1_repository_identity_sha256: String,
    pub catalog: SchemaCompatibilityWindow,
    pub resolver: SchemaCompatibilityWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCompatibilityIdentity {
    pub runtime_lock_sha256: String,
    pub runtime_role: String,
    pub profile_format: String,
    pub browser_identity_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProvenanceIdentity {
    pub cargo_lock_sha256: String,
    pub rust_toolchain_sha256: String,
    pub frontend_lock_sha256: String,
    pub release_architecture_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetManifest {
    pub schema_version: ReleaseSetSchemaVersion,
    pub release_set_id: String,
    pub display_version: Option<String>,
    pub source: ReleaseSetSource,
    pub components: BTreeMap<String, ReleaseComponentIdentity>,
    pub contracts: ContractsIdentity,
    pub protocols: ProtocolIdentity,
    pub schemas: SchemaIdentity,
    pub runtime_compatibility: RuntimeCompatibilityIdentity,
    pub capability_profile_compatibility: Vec<String>,
    pub build_provenance: BuildProvenanceIdentity,
    pub artifact_inventory: Vec<ArtifactIdentity>,
}

impl ReleaseSetManifest {
    #[must_use]
    pub fn component_ids(&self) -> Vec<&str> {
        self.components.keys().map(String::as_str).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseModelError {
    message: String,
}

impl ReleaseModelError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReleaseModelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseModelError {}

#[cfg(test)]
mod tests {
    use super::{RELEASE_SET_ID_PREFIX_V3, ReleaseSetSchemaVersion};

    #[test]
    fn release_set_version_owns_number_and_identity_prefix_together() {
        assert_eq!(ReleaseSetSchemaVersion::CURRENT.number(), 3);
        assert_eq!(
            ReleaseSetSchemaVersion::CURRENT.id_prefix(),
            RELEASE_SET_ID_PREFIX_V3
        );
    }

    #[test]
    fn unsupported_versions_fail_closed() {
        assert!(ReleaseSetSchemaVersion::from_number(2).is_err());
        assert!(ReleaseSetSchemaVersion::from_number(4).is_err());
    }
}
