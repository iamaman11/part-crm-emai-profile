use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const EXPECTED_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";

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
pub struct ReleaseSetV3Parts {
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

/// Pure representation-independent Release Set v3 aggregate semantics.
///
/// External DTOs, JSON, canonicalization, hashing, filesystem observations and provider/GitHub
/// evidence stay in outer adapters. This type owns only Release Set cross-section invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetV3 {
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

impl ReleaseSetV3 {
    pub fn new(mut parts: ReleaseSetV3Parts) -> Result<Self, ReleaseModelError> {
        parts
            .contracts
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        parts.capability_profile_compatibility.sort();
        parts
            .artifact_inventory
            .sort_by(|left, right| left.path.cmp(&right.path));

        let release_set = Self {
            schema_version: ReleaseSetSchemaVersion::CURRENT,
            source: parts.source,
            components: parts.components,
            contracts: parts.contracts,
            protocols: parts.protocols,
            schemas: parts.schemas,
            runtime_compatibility: parts.runtime_compatibility,
            capability_profile_compatibility: parts.capability_profile_compatibility,
            build_provenance: parts.build_provenance,
            artifact_inventory: parts.artifact_inventory,
        };
        release_set.validate()?;
        Ok(release_set)
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
        validate_component_artifacts(&self.components, &self.artifact_inventory)
    }

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
    #[must_use]
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
    validate_sha256(
        &source.accepted_main_evidence_sha256,
        "source.accepted_main_evidence_sha256",
    )
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
            return Err(ReleaseModelError::new(format!(
                "missing required component: {required}"
            )));
        }
    }
    for (component_id, component) in components {
        if !ALLOWED_COMPONENTS.contains(&component_id.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown component: {component_id}"
            )));
        }
        if component.component_id != *component_id {
            return Err(ReleaseModelError::new(format!(
                "component identity key mismatch: expected {component_id}, observed {}",
                component.component_id
            )));
        }
        non_empty(
            &component.release_id,
            &format!("components.{component_id}.release_id"),
        )?;
        validate_git_sha(
            &component.source_commit_sha,
            &format!("components.{component_id}.source_commit_sha"),
        )?;
        if component.source_commit_sha != source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {component_id} source SHA differs from release source"
            )));
        }
        validate_relative_path(&component.artifact_path, "component artifact path")?;
        validate_sha256(
            &component.artifact_sha256,
            &format!("components.{component_id}.artifact_sha256"),
        )?;
        positive(component.artifact_size_bytes, "component artifact size")?;
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
            return Err(ReleaseModelError::new(format!(
                "duplicate contract path: {}",
                file.path
            )));
        }
        if previous.is_some_and(|value| value > file.path.as_str()) {
            return Err(ReleaseModelError::new(
                "contracts.files must use canonical path ordering",
            ));
        }
        previous = Some(file.path.as_str());
        validate_sha256(&file.sha256, "contracts.files.sha256")?;
        positive(file.size_bytes, "contracts.files.size_bytes")?;
    }
    validate_sha256(&contracts.sha256, "contracts.sha256")
}

fn validate_protocols(protocols: &ProtocolIdentity) -> Result<(), ReleaseModelError> {
    validate_sha256(
        &protocols.public_api_contract_sha256,
        "protocols.public_api_contract_sha256",
    )?;
    positive(
        protocols.camouhost_ipc_version,
        "protocols.camouhost_ipc_version",
    )?;
    positive(
        protocols.profile_bridge_protocol_version,
        "protocols.profile_bridge_protocol_version",
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
    non_empty(&window.target_schema_revision, "target_schema_revision")?;
    non_empty(&window.supported_schema_min, "supported_schema_min")?;
    non_empty(&window.supported_schema_max, "supported_schema_max")?;
    if window.supported_schema_min > window.supported_schema_max
        || window.target_schema_revision < window.supported_schema_min
        || window.target_schema_revision > window.supported_schema_max
    {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: invalid compatibility window for {expected_component}"
        )));
    }
    validate_sha256(
        &window.migration_history_digest,
        "schemas.migration_history_digest",
    )?;
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
    non_empty(
        &runtime.profile_format,
        "runtime_compatibility.profile_format",
    )?;
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
        non_empty(profile, "capability_profile_compatibility entry")?;
        if !seen.insert(profile.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "duplicate capability profile: {profile}"
            )));
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

fn validate_build_provenance(
    provenance: &BuildProvenanceIdentity,
) -> Result<(), ReleaseModelError> {
    for (value, label) in [
        (
            &provenance.cargo_lock_sha256,
            "build_provenance.cargo_lock_sha256",
        ),
        (
            &provenance.rust_toolchain_sha256,
            "build_provenance.rust_toolchain_sha256",
        ),
        (
            &provenance.frontend_lock_sha256,
            "build_provenance.frontend_lock_sha256",
        ),
        (
            &provenance.release_architecture_sha256,
            "build_provenance.release_architecture_sha256",
        ),
    ] {
        validate_sha256(value, label)?;
    }
    Ok(())
}

fn validate_artifacts(artifacts: &[ArtifactIdentity]) -> Result<(), ReleaseModelError> {
    if artifacts.is_empty() {
        return Err(ReleaseModelError::new(
            "artifact_inventory must not be empty",
        ));
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
        validate_sha256(&artifact.sha256, "artifact sha256")?;
        positive(artifact.size_bytes, "artifact size")?;
        non_empty(&artifact.kind, "artifact kind")?;
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
    for (component_id, component) in components {
        let artifact = by_path
            .get(component.artifact_path.as_str())
            .ok_or_else(|| {
                ReleaseModelError::new(format!(
                    "ARTIFACT_INVENTORY_MISMATCH: component {component_id} artifact is absent"
                ))
            })?;
        if artifact.sha256 != component.artifact_sha256
            || artifact.size_bytes != component.artifact_size_bytes
        {
            return Err(ReleaseModelError::new(format!(
                "ARTIFACT_INVENTORY_MISMATCH: component {component_id} artifact identity differs"
            )));
        }
    }
    Ok(())
}

fn validate_git_sha(value: &str, label: &str) -> Result<(), ReleaseModelError> {
    if !lowercase_hex(value, 40) {
        return Err(ReleaseModelError::new(format!(
            "{label} must be 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), ReleaseModelError> {
    if !lowercase_hex(value, 64) {
        return Err(ReleaseModelError::new(format!(
            "{label} must be 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn lowercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_relative_path(value: &str, label: &str) -> Result<(), ReleaseModelError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ReleaseModelError::new(format!(
            "{label} must be a normalized repository-relative POSIX path"
        )));
    }
    Ok(())
}

fn positive(value: u64, label: &str) -> Result<(), ReleaseModelError> {
    if value == 0 {
        return Err(ReleaseModelError::new(format!("{label} must be positive")));
    }
    Ok(())
}

fn non_empty(value: &str, label: &str) -> Result<(), ReleaseModelError> {
    if value.is_empty() {
        return Err(ReleaseModelError::new(format!("{label} must not be empty")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactIdentity, BuildProvenanceIdentity, ContractsIdentity, EXPECTED_REPOSITORY,
        ProtocolIdentity, ProvenanceFileIdentity, ReleaseComponentIdentity, ReleaseModelError,
        ReleaseSetSchemaVersion, ReleaseSetSource, ReleaseSetV3, ReleaseSetV3Parts,
        RuntimeCompatibilityIdentity, SchemaCompatibilityWindow, SchemaIdentity,
    };
    use std::collections::BTreeMap;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn component(id: &str, path: &str) -> ReleaseComponentIdentity {
        ReleaseComponentIdentity {
            component_id: id.to_owned(),
            release_id: format!("{id}-v1"),
            source_commit_sha: GIT.to_owned(),
            artifact_path: path.to_owned(),
            artifact_sha256: SHA.to_owned(),
            artifact_size_bytes: 1,
            component_manifest_sha256: SHA.to_owned(),
        }
    }

    fn artifact(path: &str) -> ArtifactIdentity {
        ArtifactIdentity {
            path: path.to_owned(),
            sha256: SHA.to_owned(),
            size_bytes: 1,
            kind: "component".to_owned(),
        }
    }

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

    fn valid_parts() -> ReleaseSetV3Parts {
        let mut components = BTreeMap::new();
        components.insert(
            "control_plane".to_owned(),
            component("control_plane", "components/control-plane.tar"),
        );
        components.insert(
            "secret_resolver".to_owned(),
            component("secret_resolver", "components/secret-resolver.tar"),
        );
        components.insert(
            "runtime_bundle".to_owned(),
            component("runtime_bundle", "components/runtime-bundle.tar"),
        );
        ReleaseSetV3Parts {
            source: ReleaseSetSource {
                repository: EXPECTED_REPOSITORY.to_owned(),
                commit_sha: GIT.to_owned(),
                accepted_main: true,
                accepted_main_evidence_sha256: SHA.to_owned(),
            },
            components,
            contracts: ContractsIdentity {
                files: vec![ProvenanceFileIdentity {
                    path: "openapi/v1/openapi.json".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                }],
                sha256: SHA.to_owned(),
            },
            protocols: ProtocolIdentity {
                public_api_contract_sha256: SHA.to_owned(),
                camouhost_ipc_version: 1,
                profile_bridge_protocol_version: 1,
                resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
            },
            schemas: SchemaIdentity {
                d1_repository_identity_sha256: SHA.to_owned(),
                catalog: schema("catalog"),
                resolver: schema("resolver"),
            },
            runtime_compatibility: RuntimeCompatibilityIdentity {
                runtime_lock_sha256: SHA.to_owned(),
                runtime_role: "camouhost".to_owned(),
                profile_format: "camoufox-fingerprint-v1".to_owned(),
                browser_identity_policy: "browser-identity-v1".to_owned(),
            },
            capability_profile_compatibility: vec!["production-core-v1".to_owned()],
            build_provenance: BuildProvenanceIdentity {
                cargo_lock_sha256: SHA.to_owned(),
                rust_toolchain_sha256: SHA.to_owned(),
                frontend_lock_sha256: SHA.to_owned(),
                release_architecture_sha256: SHA.to_owned(),
            },
            artifact_inventory: vec![
                artifact("components/runtime-bundle.tar"),
                artifact("components/control-plane.tar"),
                artifact("components/secret-resolver.tar"),
            ],
        }
    }

    #[test]
    fn constructs_normalized_v3_semantics_in_memory() -> Result<(), Box<dyn std::error::Error>> {
        let release_set = ReleaseSetV3::new(valid_parts())?;
        assert_eq!(release_set.schema_version, ReleaseSetSchemaVersion::V3);
        assert_eq!(release_set.schema_version.number(), 3);
        assert_eq!(
            release_set
                .artifact_inventory
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "components/control-plane.tar",
                "components/runtime-bundle.tar",
                "components/secret-resolver.tar",
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_cross_section_source_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let mut parts = valid_parts();
        let component = parts
            .components
            .get_mut("control_plane")
            .ok_or_else(|| ReleaseModelError::new("fixture missing control_plane"))?;
        component.source_commit_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
        assert!(ReleaseSetV3::new(parts).is_err());
        Ok(())
    }

    #[test]
    fn rejects_component_artifact_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let mut parts = valid_parts();
        let component = parts
            .components
            .get_mut("runtime_bundle")
            .ok_or_else(|| ReleaseModelError::new("fixture missing runtime_bundle"))?;
        component.artifact_size_bytes = 2;
        assert!(ReleaseSetV3::new(parts).is_err());
        Ok(())
    }

    #[test]
    fn rejects_invalid_schema_window_without_reimplementing_d1_source_policy() {
        let mut parts = valid_parts();
        parts.schemas.catalog.supported_schema_min = "0002_next.sql".to_owned();
        assert!(ReleaseSetV3::new(parts).is_err());
    }

    #[test]
    fn pure_semantics_have_no_external_release_id_or_display_version()
    -> Result<(), Box<dyn std::error::Error>> {
        let release_set = ReleaseSetV3::new(valid_parts())?;
        assert_eq!(release_set.component_ids().len(), 3);
        Ok(())
    }
}
