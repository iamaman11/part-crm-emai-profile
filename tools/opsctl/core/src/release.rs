use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_ID_PREFIX_V3: &str = "release-set-v3-sha256-";
pub const EXPECTED_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
pub const MAX_JCS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

const REQUIRED_COMPONENTS: [&str; 3] = ["control_plane", "secret_resolver", "runtime_bundle"];
const ALLOWED_COMPONENTS: [&str; 5] = [
    "control_plane",
    "frontend",
    "secret_resolver",
    "runtime_bundle",
    "profile_bridge",
];

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

/// Pure, representation-independent semantic identity for the current Release Set.
///
/// This type deliberately excludes `release_set_id` and display-only fields. The outer
/// adapter renders this typed identity with the canonical external representation and
/// binds the resulting digest back through `into_manifest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetDraft {
    pub schema_version: ReleaseSetSchemaVersion,
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

impl ReleaseSetDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: ReleaseSetSource,
        components: BTreeMap<String, ReleaseComponentIdentity>,
        mut contracts: ContractsIdentity,
        protocols: ProtocolIdentity,
        schemas: SchemaIdentity,
        runtime_compatibility: RuntimeCompatibilityIdentity,
        mut capability_profile_compatibility: Vec<String>,
        build_provenance: BuildProvenanceIdentity,
        mut artifact_inventory: Vec<ArtifactIdentity>,
    ) -> Result<Self, ReleaseModelError> {
        contracts.files.sort_by(|left, right| left.path.cmp(&right.path));
        capability_profile_compatibility.sort();
        artifact_inventory.sort_by(|left, right| left.path.cmp(&right.path));
        let draft = Self {
            schema_version: ReleaseSetSchemaVersion::CURRENT,
            source,
            components,
            contracts,
            protocols,
            schemas,
            runtime_compatibility,
            capability_profile_compatibility,
            build_provenance,
            artifact_inventory,
        };
        draft.validate()?;
        Ok(draft)
    }

    pub fn from_manifest(manifest: &ReleaseSetManifest) -> Result<Self, ReleaseModelError> {
        if manifest.schema_version != ReleaseSetSchemaVersion::CURRENT {
            return Err(ReleaseModelError::new(
                "Release Set manifest is not the current semantic contract",
            ));
        }
        Self::new(
            manifest.source.clone(),
            manifest.components.clone(),
            manifest.contracts.clone(),
            manifest.protocols.clone(),
            manifest.schemas.clone(),
            manifest.runtime_compatibility.clone(),
            manifest.capability_profile_compatibility.clone(),
            manifest.build_provenance.clone(),
            manifest.artifact_inventory.clone(),
        )
    }

    pub fn into_manifest(self, release_set_id: String) -> Result<ReleaseSetManifest, ReleaseModelError> {
        validate_release_set_id(&release_set_id, self.schema_version)?;
        Ok(ReleaseSetManifest {
            schema_version: self.schema_version,
            release_set_id,
            display_version: None,
            source: self.source,
            components: self.components,
            contracts: self.contracts,
            protocols: self.protocols,
            schemas: self.schemas,
            runtime_compatibility: self.runtime_compatibility,
            capability_profile_compatibility: self.capability_profile_compatibility,
            build_provenance: self.build_provenance,
            artifact_inventory: self.artifact_inventory,
        })
    }

    pub fn validate(&self) -> Result<(), ReleaseModelError> {
        validate_source(&self.source)?;
        validate_components(&self.components, &self.source.commit_sha)?;
        validate_contracts(&self.contracts)?;
        validate_protocols(&self.protocols)?;
        validate_schemas(&self.schemas)?;
        validate_runtime(&self.runtime_compatibility)?;
        validate_profiles(&self.capability_profile_compatibility)?;
        validate_build_provenance(&self.build_provenance)?;
        validate_artifacts(&self.artifact_inventory)?;
        validate_component_artifacts(&self.components, &self.artifact_inventory)?;
        Ok(())
    }
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

    pub fn semantic_identity(&self) -> Result<ReleaseSetDraft, ReleaseModelError> {
        ReleaseSetDraft::from_manifest(self)
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

fn validate_source(source: &ReleaseSetSource) -> Result<(), ReleaseModelError> {
    if source.repository != EXPECTED_REPOSITORY {
        return Err(ReleaseModelError::new(format!(
            "SOURCE_NOT_ACCEPTED: repository must be {EXPECTED_REPOSITORY}"
        )));
    }
    validate_git_sha(&source.commit_sha, "source.commit_sha")?;
    if !source.accepted_main {
        return Err(ReleaseModelError::new(
            "SOURCE_NOT_ACCEPTED: accepted_main must be true",
        ));
    }
    validate_sha256(&source.accepted_main_evidence_sha256, "source.accepted_main_evidence_sha256")
}

fn validate_components(
    components: &BTreeMap<String, ReleaseComponentIdentity>,
    source_commit_sha: &str,
) -> Result<(), ReleaseModelError> {
    if components.is_empty() {
        return Err(ReleaseModelError::new("components must not be empty"));
    }
    for required in REQUIRED_COMPONENTS {
        if !components.contains_key(required) {
            return Err(ReleaseModelError::new(format!("missing required component: {required}")));
        }
    }
    for (component_id, component) in components {
        if !ALLOWED_COMPONENTS.contains(&component_id.as_str()) {
            return Err(ReleaseModelError::new(format!("unknown component: {component_id}")));
        }
        if component.component_id != *component_id {
            return Err(ReleaseModelError::new(format!(
                "component identity key mismatch: expected {component_id}, observed {}",
                component.component_id
            )));
        }
        non_empty(&component.release_id, &format!("components.{component_id}.release_id"))?;
        validate_git_sha(
            &component.source_commit_sha,
            &format!("components.{component_id}.source_commit_sha"),
        )?;
        if component.source_commit_sha != source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {component_id} source SHA differs from release source"
            )));
        }
        validate_relative_path(&component.artifact_path, "artifact path")?;
        validate_sha256(
            &component.artifact_sha256,
            &format!("components.{component_id}.artifact_sha256"),
        )?;
        validate_positive_jcs_integer(
            component.artifact_size_bytes,
            &format!("components.{component_id}.artifact_size_bytes"),
        )?;
        validate_sha256(
            &component.component_manifest_sha256,
            &format!("components.{component_id}.component_manifest_sha256"),
        )?;
    }
    Ok(())
}

fn validate_contracts(contracts: &ContractsIdentity) -> Result<(), ReleaseModelError> {
    if contracts.files.is_empty() {
        return Err(ReleaseModelError::new("contracts.files must not be empty"));
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for file in &contracts.files {
        validate_relative_path(&file.path, "contract path")?;
        if !seen.insert(file.path.as_str()) {
            return Err(ReleaseModelError::new(format!("duplicate contract path: {}", file.path)));
        }
        if previous.is_some_and(|value| value > file.path.as_str()) {
            return Err(ReleaseModelError::new("contracts.files must use canonical path ordering"));
        }
        previous = Some(file.path.as_str());
        validate_sha256(&file.sha256, &format!("contracts.{}.sha256", file.path))?;
        validate_positive_jcs_integer(file.size_bytes, &format!("contracts.{}.size_bytes", file.path))?;
    }
    validate_sha256(&contracts.sha256, "contracts.sha256")
}

fn validate_protocols(protocols: &ProtocolIdentity) -> Result<(), ReleaseModelError> {
    validate_sha256(
        &protocols.public_api_contract_sha256,
        "protocols.public_api_contract_sha256",
    )?;
    non_empty(&protocols.resolver_protocol, "protocols.resolver_protocol")
}

fn validate_schema_window(
    window: &SchemaCompatibilityWindow,
    expected_component: &str,
) -> Result<(), ReleaseModelError> {
    if window.database_component != expected_component {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: expected {expected_component}, observed {}",
            window.database_component
        )));
    }
    for (value, field) in [
        (&window.target_schema_revision, "target_schema_revision"),
        (&window.supported_schema_min, "supported_schema_min"),
        (&window.supported_schema_max, "supported_schema_max"),
    ] {
        non_empty(value, field)?;
    }
    if window.supported_schema_min > window.supported_schema_max
        || window.target_schema_revision < window.supported_schema_min
        || window.target_schema_revision > window.supported_schema_max
    {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: invalid compatibility window for {expected_component}"
        )));
    }
    validate_sha256(&window.migration_history_digest, "schemas.migration_history_digest")?;
    validate_sha256(
        &window.compatibility_policy_digest,
        "schemas.compatibility_policy_digest",
    )
}

fn validate_schemas(schemas: &SchemaIdentity) -> Result<(), ReleaseModelError> {
    validate_sha256(
        &schemas.d1_repository_identity_sha256,
        "schemas.d1_repository_identity_sha256",
    )?;
    validate_schema_window(&schemas.catalog, "catalog")?;
    validate_schema_window(&schemas.resolver, "resolver")
}

fn validate_runtime(runtime: &RuntimeCompatibilityIdentity) -> Result<(), ReleaseModelError> {
    validate_sha256(
        &runtime.runtime_lock_sha256,
        "runtime_compatibility.runtime_lock_sha256",
    )?;
    non_empty(&runtime.runtime_role, "runtime_compatibility.runtime_role")?;
    non_empty(&runtime.profile_format, "runtime_compatibility.profile_format")?;
    non_empty(
        &runtime.browser_identity_policy,
        "runtime_compatibility.browser_identity_policy",
    )
}

fn validate_profiles(profiles: &[String]) -> Result<(), ReleaseModelError> {
    if profiles.is_empty() {
        return Err(ReleaseModelError::new(
            "capability_profile_compatibility must not be empty",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for profile in profiles {
        non_empty(profile, "capability_profile_compatibility")?;
        if !seen.insert(profile.as_str()) {
            return Err(ReleaseModelError::new(
                "capability_profile_compatibility contains a duplicate value",
            ));
        }
        if previous.is_some_and(|value| value > profile.as_str()) {
            return Err(ReleaseModelError::new(
                "capability_profile_compatibility must use canonical ordering",
            ));
        }
        previous = Some(profile.as_str());
    }
    Ok(())
}

fn validate_build_provenance(build: &BuildProvenanceIdentity) -> Result<(), ReleaseModelError> {
    for (value, field) in [
        (&build.cargo_lock_sha256, "build_provenance.cargo_lock_sha256"),
        (
            &build.rust_toolchain_sha256,
            "build_provenance.rust_toolchain_sha256",
        ),
        (
            &build.frontend_lock_sha256,
            "build_provenance.frontend_lock_sha256",
        ),
        (
            &build.release_architecture_sha256,
            "build_provenance.release_architecture_sha256",
        ),
    ] {
        validate_sha256(value, field)?;
    }
    Ok(())
}

fn validate_artifacts(artifacts: &[ArtifactIdentity]) -> Result<(), ReleaseModelError> {
    if artifacts.is_empty() {
        return Err(ReleaseModelError::new("artifact_inventory must not be empty"));
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for artifact in artifacts {
        validate_relative_path(&artifact.path, "artifact path")?;
        if !seen.insert(artifact.path.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "duplicate artifact path: {}",
                artifact.path
            )));
        }
        if previous.is_some_and(|value| value > artifact.path.as_str()) {
            return Err(ReleaseModelError::new(
                "artifact_inventory must use canonical path ordering",
            ));
        }
        previous = Some(artifact.path.as_str());
        validate_sha256(&artifact.sha256, &format!("artifact_inventory.{}.sha256", artifact.path))?;
        validate_positive_jcs_integer(
            artifact.size_bytes,
            &format!("artifact_inventory.{}.size_bytes", artifact.path),
        )?;
        if artifact.kind != "component" {
            return Err(ReleaseModelError::new(format!(
                "Release Set v3 artifact {} must be a component archive, observed kind={}",
                artifact.path, artifact.kind
            )));
        }
    }
    Ok(())
}

fn validate_component_artifacts(
    components: &BTreeMap<String, ReleaseComponentIdentity>,
    artifacts: &[ArtifactIdentity],
) -> Result<(), ReleaseModelError> {
    let by_path = artifacts
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    for component in components.values() {
        let artifact = by_path.get(component.artifact_path.as_str()).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "component {} artifact is absent from artifact_inventory",
                component.component_id
            ))
        })?;
        if artifact.sha256 != component.artifact_sha256
            || artifact.size_bytes != component.artifact_size_bytes
        {
            return Err(ReleaseModelError::new(format!(
                "component {} artifact identity disagrees with artifact_inventory",
                component.component_id
            )));
        }
    }
    Ok(())
}

fn validate_release_set_id(
    value: &str,
    version: ReleaseSetSchemaVersion,
) -> Result<(), ReleaseModelError> {
    let digest = value.strip_prefix(version.id_prefix()).ok_or_else(|| {
        ReleaseModelError::new(format!(
            "release_set_id must use the schema-owned {} prefix",
            version.id_prefix()
        ))
    })?;
    validate_sha256(digest, "release_set_id digest")
}

fn validate_relative_path(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.contains('\0')
        || value.contains(':')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(ReleaseModelError::new(format!("unsafe {field}: {value:?}")));
    }
    Ok(())
}

fn validate_git_sha(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 40 || !value.bytes().all(is_lower_hex) {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 64 || !value.bytes().all(is_lower_hex) {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_positive_jcs_integer(value: u64, field: &str) -> Result<(), ReleaseModelError> {
    if value == 0 || value > MAX_JCS_SAFE_INTEGER {
        return Err(ReleaseModelError::new(format!(
            "{field} must be positive and within RFC 8785/I-JSON safe integer range"
        )));
    }
    Ok(())
}

fn non_empty(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!("{field} must not be empty")));
    }
    Ok(())
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactIdentity, BuildProvenanceIdentity, ContractsIdentity, ProtocolIdentity,
        ProvenanceFileIdentity, RELEASE_SET_ID_PREFIX_V3, ReleaseComponentIdentity,
        ReleaseSetDraft, ReleaseSetSchemaVersion, ReleaseSetSource, RuntimeCompatibilityIdentity,
        SchemaCompatibilityWindow, SchemaIdentity,
    };
    use std::collections::BTreeMap;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn schema(component: &str) -> SchemaCompatibilityWindow {
        SchemaCompatibilityWindow {
            database_component: component.to_owned(),
            target_schema_revision: "0001_initial.sql".to_owned(),
            supported_schema_min: "0001_initial.sql".to_owned(),
            supported_schema_max: "0001_initial.sql".to_owned(),
            migration_history_digest: SHA.to_owned(),
            compatibility_policy_digest: SHA.to_owned(),
        }
    }

    fn draft(profiles: Vec<String>) -> Result<ReleaseSetDraft, super::ReleaseModelError> {
        let mut components = BTreeMap::new();
        for (id, path) in [
            ("control_plane", "components/control.tar"),
            ("secret_resolver", "components/resolver.tar"),
            ("runtime_bundle", "components/runtime.tar"),
        ] {
            components.insert(
                id.to_owned(),
                ReleaseComponentIdentity {
                    component_id: id.to_owned(),
                    release_id: format!("{id}-v1"),
                    source_commit_sha: GIT.to_owned(),
                    artifact_path: path.to_owned(),
                    artifact_sha256: SHA.to_owned(),
                    artifact_size_bytes: 1,
                    component_manifest_sha256: SHA.to_owned(),
                },
            );
        }
        ReleaseSetDraft::new(
            ReleaseSetSource {
                repository: super::EXPECTED_REPOSITORY.to_owned(),
                commit_sha: GIT.to_owned(),
                accepted_main: true,
                accepted_main_evidence_sha256: SHA.to_owned(),
            },
            components,
            ContractsIdentity {
                files: vec![ProvenanceFileIdentity {
                    path: "openapi/v1/openapi.json".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                }],
                sha256: SHA.to_owned(),
            },
            ProtocolIdentity {
                public_api_contract_sha256: SHA.to_owned(),
                camouhost_ipc_version: 1,
                profile_bridge_protocol_version: 1,
                resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
            },
            SchemaIdentity {
                d1_repository_identity_sha256: SHA.to_owned(),
                catalog: schema("catalog"),
                resolver: schema("resolver"),
            },
            RuntimeCompatibilityIdentity {
                runtime_lock_sha256: SHA.to_owned(),
                runtime_role: "real_camoufox".to_owned(),
                profile_format: "v1".to_owned(),
                browser_identity_policy: "v1".to_owned(),
            },
            profiles,
            BuildProvenanceIdentity {
                cargo_lock_sha256: SHA.to_owned(),
                rust_toolchain_sha256: SHA.to_owned(),
                frontend_lock_sha256: SHA.to_owned(),
                release_architecture_sha256: SHA.to_owned(),
            },
            vec![
                ArtifactIdentity {
                    path: "components/runtime.tar".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
                ArtifactIdentity {
                    path: "components/control.tar".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
                ArtifactIdentity {
                    path: "components/resolver.tar".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
            ],
        )
    }

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

    #[test]
    fn draft_normalizes_representation_independent_identity_ordering() -> Result<(), super::ReleaseModelError> {
        let value = draft(vec![
            "rehearsal-core-v1".to_owned(),
            "production-core-v1".to_owned(),
        ])?;
        assert_eq!(
            value.capability_profile_compatibility,
            ["production-core-v1", "rehearsal-core-v1"]
        );
        assert_eq!(value.artifact_inventory[0].path, "components/control.tar");
        Ok(())
    }

    #[test]
    fn duplicate_profile_fails_closed() {
        let result = draft(vec!["rehearsal-core-v1".to_owned(), "rehearsal-core-v1".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn semantic_identity_binds_only_schema_owned_v3_release_id() -> Result<(), super::ReleaseModelError> {
        let value = draft(vec!["rehearsal-core-v1".to_owned()])?;
        assert!(value
            .clone()
            .into_manifest(format!("{RELEASE_SET_ID_PREFIX_V3}{SHA}"))
            .is_ok());
        assert!(value
            .into_manifest(format!("release-set-v2-sha256-{SHA}"))
            .is_err());
        Ok(())
    }
}