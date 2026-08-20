use crate::release::digest::{canonical_json, sha256_hex};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_SCHEMA_VERSION: u64 = 2;
pub const RELEASE_SET_ID_PREFIX: &str = "release-set-v2-sha256-";
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
    pub component_manifest_path: String,
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
        revision >= self.supported_schema_min.as_str() && revision <= self.supported_schema_max.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIdentity {
    pub d1_evolution_authority_sha256: String,
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
    pub schema_version: u64,
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
    identity_payload: Value,
}

impl ReleaseSetManifest {
    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!("invalid release-set JSON: {error}"))
        })?;
        let root = object(&value, "release-set root")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "release_set_id",
                "display_version",
                "source",
                "components",
                "contracts",
                "protocols",
                "schemas",
                "runtime_compatibility",
                "capability_profile_compatibility",
                "build_provenance",
                "artifact_inventory",
            ],
            "release-set root",
        )?;

        let schema_version = required_u64(root, "schema_version")?;
        if schema_version != RELEASE_SET_SCHEMA_VERSION {
            return Err(ReleaseModelError::new(format!(
                "unsupported release-set schema_version: {schema_version}; only v2 is accepted before first production release"
            )));
        }
        let release_set_id = required_string(root, "release_set_id")?;
        validate_release_set_id(&release_set_id)?;
        let display_version = optional_string(root, "display_version")?;
        let source = parse_source(required(root, "source")?)?;
        let components = parse_components(required(root, "components")?, &source.commit_sha)?;
        let artifact_inventory = parse_artifact_inventory(required(root, "artifact_inventory")?)?;
        validate_component_artifacts(&components, &artifact_inventory)?;
        let contracts = parse_contracts(required(root, "contracts")?)?;
        let protocols = parse_protocols(required(root, "protocols")?)?;
        let schemas = parse_schemas(required(root, "schemas")?)?;
        let runtime_compatibility = parse_runtime(required(root, "runtime_compatibility")?)?;
        let capability_profile_compatibility = required_string_array(root, "capability_profile_compatibility")?;
        if capability_profile_compatibility.is_empty() {
            return Err(ReleaseModelError::new(
                "capability_profile_compatibility must not be empty",
            ));
        }
        let build_provenance = parse_build_provenance(required(root, "build_provenance")?)?;

        let mut identity_payload = value.clone();
        let payload = identity_payload
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("release-set root must remain an object"))?;
        payload.remove("release_set_id");
        payload.remove("display_version");

        let manifest = Self {
            schema_version,
            release_set_id,
            display_version,
            source,
            components,
            contracts,
            protocols,
            schemas,
            runtime_compatibility,
            capability_profile_compatibility,
            build_provenance,
            artifact_inventory,
            identity_payload,
        };
        manifest.verify_content_address()?;
        Ok(manifest)
    }

    pub fn verify_content_address(&self) -> Result<(), ReleaseModelError> {
        let canonical = canonical_json(&self.identity_payload).map_err(ReleaseModelError::new)?;
        let expected = format!(
            "{RELEASE_SET_ID_PREFIX}{}",
            sha256_hex(canonical.as_bytes())
        );
        if self.release_set_id != expected {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_IDENTITY_MISMATCH: expected {expected}, observed {}",
                self.release_set_id
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn component_ids(&self) -> Vec<&str> {
        self.components.keys().map(String::as_str).collect()
    }
}

fn parse_source(value: &Value) -> Result<ReleaseSetSource, ReleaseModelError> {
    let source = object(value, "source")?;
    reject_unknown_fields(
        source,
        &[
            "repository",
            "commit_sha",
            "accepted_main",
            "accepted_main_evidence_sha256",
        ],
        "source",
    )?;
    let repository = required_string(source, "repository")?;
    if repository != EXPECTED_REPOSITORY {
        return Err(ReleaseModelError::new(format!(
            "SOURCE_NOT_ACCEPTED: repository must be {EXPECTED_REPOSITORY}"
        )));
    }
    let commit_sha = required_string(source, "commit_sha")?;
    validate_git_sha(&commit_sha, "source.commit_sha")?;
    let accepted_main = required_bool(source, "accepted_main")?;
    if !accepted_main {
        return Err(ReleaseModelError::new(
            "SOURCE_NOT_ACCEPTED: accepted_main must be true",
        ));
    }
    let accepted_main_evidence_sha256 = required_string(source, "accepted_main_evidence_sha256")?;
    validate_sha256_like(
        &accepted_main_evidence_sha256,
        "source.accepted_main_evidence_sha256",
    )?;
    let accepted_main_identity = serde_json::json!({
        "authority": "accepted-main",
        "commit_sha": commit_sha,
        "repository": repository,
    });
    let canonical = canonical_json(&accepted_main_identity).map_err(ReleaseModelError::new)?;
    let expected_evidence = sha256_hex(canonical.as_bytes());
    if accepted_main_evidence_sha256 != expected_evidence {
        return Err(ReleaseModelError::new(
            "SOURCE_NOT_ACCEPTED: accepted-main identity binding is invalid",
        ));
    }
    Ok(ReleaseSetSource {
        repository,
        commit_sha,
        accepted_main,
        accepted_main_evidence_sha256,
    })
}

fn parse_components(
    value: &Value,
    source_commit_sha: &str,
) -> Result<BTreeMap<String, ReleaseComponentIdentity>, ReleaseModelError> {
    let components = object(value, "components")?;
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
    let mut result = BTreeMap::new();
    for (component_id, value) in components {
        if !ALLOWED_COMPONENTS.contains(&component_id.as_str()) {
            return Err(ReleaseModelError::new(format!("unknown component: {component_id}")));
        }
        let component = object(value, &format!("components.{component_id}"))?;
        reject_unknown_fields(
            component,
            &[
                "release_id",
                "source_commit_sha",
                "artifact_path",
                "artifact_sha256",
                "artifact_size_bytes",
                "component_manifest_path",
                "component_manifest_sha256",
            ],
            &format!("components.{component_id}"),
        )?;
        let release_id = required_string(component, "release_id")?;
        if release_id.trim().is_empty() {
            return Err(ReleaseModelError::new(format!(
                "components.{component_id}.release_id must not be empty"
            )));
        }
        let component_source = required_string(component, "source_commit_sha")?;
        validate_git_sha(&component_source, &format!("components.{component_id}.source_commit_sha"))?;
        if component_source != source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {component_id} source SHA differs from release source"
            )));
        }
        let artifact_path = required_string(component, "artifact_path")?;
        validate_artifact_path(&artifact_path)?;
        let artifact_sha256 = required_string(component, "artifact_sha256")?;
        validate_sha256_like(&artifact_sha256, &format!("components.{component_id}.artifact_sha256"))?;
        let artifact_size_bytes = required_u64(component, "artifact_size_bytes")?;
        if artifact_size_bytes == 0 {
            return Err(ReleaseModelError::new(format!(
                "components.{component_id}.artifact_size_bytes must be positive"
            )));
        }
        let component_manifest_path = required_string(component, "component_manifest_path")?;
        validate_artifact_path(&component_manifest_path)?;
        let component_manifest_sha256 = required_string(component, "component_manifest_sha256")?;
        validate_sha256_like(
            &component_manifest_sha256,
            &format!("components.{component_id}.component_manifest_sha256"),
        )?;
        result.insert(
            component_id.clone(),
            ReleaseComponentIdentity {
                component_id: component_id.clone(),
                release_id,
                source_commit_sha: component_source,
                artifact_path,
                artifact_sha256,
                artifact_size_bytes,
                component_manifest_path,
                component_manifest_sha256,
            },
        );
    }
    Ok(result)
}

fn parse_contracts(value: &Value) -> Result<ContractsIdentity, ReleaseModelError> {
    let root = object(value, "contracts")?;
    reject_unknown_fields(root, &["files", "sha256"], "contracts")?;
    let files = array(required(root, "files")?, "contracts.files")?;
    if files.is_empty() {
        return Err(ReleaseModelError::new("contracts.files must not be empty"));
    }
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::with_capacity(files.len());
    for value in files {
        let entry = object(value, "contracts.files entry")?;
        reject_unknown_fields(entry, &["path", "sha256", "size_bytes"], "contracts.files entry")?;
        let path = required_string(entry, "path")?;
        validate_artifact_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err(ReleaseModelError::new(format!("duplicate contracts path: {path}")));
        }
        let sha256 = required_string(entry, "sha256")?;
        validate_sha256_like(&sha256, "contracts.files.sha256")?;
        let size_bytes = required_u64(entry, "size_bytes")?;
        if size_bytes == 0 {
            return Err(ReleaseModelError::new("contracts file size must be positive"));
        }
        parsed.push(ProvenanceFileIdentity { path, sha256, size_bytes });
    }
    parsed.sort_by(|left, right| left.path.cmp(&right.path));
    let sha256 = required_string(root, "sha256")?;
    validate_sha256_like(&sha256, "contracts.sha256")?;
    Ok(ContractsIdentity { files: parsed, sha256 })
}

fn parse_protocols(value: &Value) -> Result<ProtocolIdentity, ReleaseModelError> {
    let root = object(value, "protocols")?;
    reject_unknown_fields(
        root,
        &[
            "public_api_contract_sha256",
            "camouhost_ipc_version",
            "profile_bridge_protocol_version",
            "resolver_protocol",
        ],
        "protocols",
    )?;
    let public_api_contract_sha256 = required_string(root, "public_api_contract_sha256")?;
    validate_sha256_like(&public_api_contract_sha256, "protocols.public_api_contract_sha256")?;
    let camouhost_ipc_version = required_u64(root, "camouhost_ipc_version")?;
    let profile_bridge_protocol_version = required_u64(root, "profile_bridge_protocol_version")?;
    if camouhost_ipc_version == 0 || profile_bridge_protocol_version == 0 {
        return Err(ReleaseModelError::new("protocol versions must be positive"));
    }
    let resolver_protocol = required_string(root, "resolver_protocol")?;
    if resolver_protocol.trim().is_empty() {
        return Err(ReleaseModelError::new("resolver_protocol must not be empty"));
    }
    Ok(ProtocolIdentity {
        public_api_contract_sha256,
        camouhost_ipc_version,
        profile_bridge_protocol_version,
        resolver_protocol,
    })
}

fn parse_schema_window(value: &Value, expected_component: &str) -> Result<SchemaCompatibilityWindow, ReleaseModelError> {
    let root = object(value, &format!("schemas.{expected_component}"))?;
    reject_unknown_fields(
        root,
        &[
            "database_component",
            "target_schema_revision",
            "supported_schema_min",
            "supported_schema_max",
            "migration_history_digest",
            "compatibility_policy_digest",
        ],
        &format!("schemas.{expected_component}"),
    )?;
    let database_component = required_string(root, "database_component")?;
    if database_component != expected_component {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: expected {expected_component}, observed {database_component}"
        )));
    }
    let target_schema_revision = non_empty_string(root, "target_schema_revision")?;
    let supported_schema_min = non_empty_string(root, "supported_schema_min")?;
    let supported_schema_max = non_empty_string(root, "supported_schema_max")?;
    if supported_schema_min > supported_schema_max || target_schema_revision < supported_schema_min || target_schema_revision > supported_schema_max {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: invalid compatibility window for {expected_component}"
        )));
    }
    let migration_history_digest = required_string(root, "migration_history_digest")?;
    validate_sha256_like(&migration_history_digest, "schemas.migration_history_digest")?;
    let compatibility_policy_digest = required_string(root, "compatibility_policy_digest")?;
    validate_sha256_like(&compatibility_policy_digest, "schemas.compatibility_policy_digest")?;
    Ok(SchemaCompatibilityWindow {
        database_component,
        target_schema_revision,
        supported_schema_min,
        supported_schema_max,
        migration_history_digest,
        compatibility_policy_digest,
    })
}

fn parse_schemas(value: &Value) -> Result<SchemaIdentity, ReleaseModelError> {
    let root = object(value, "schemas")?;
    reject_unknown_fields(root, &["d1_evolution_authority_sha256", "catalog", "resolver"], "schemas")?;
    let d1_evolution_authority_sha256 = required_string(root, "d1_evolution_authority_sha256")?;
    validate_sha256_like(&d1_evolution_authority_sha256, "schemas.d1_evolution_authority_sha256")?;
    let catalog = parse_schema_window(required(root, "catalog")?, "catalog")?;
    let resolver = parse_schema_window(required(root, "resolver")?, "resolver")?;
    Ok(SchemaIdentity { d1_evolution_authority_sha256, catalog, resolver })
}

fn parse_runtime(value: &Value) -> Result<RuntimeCompatibilityIdentity, ReleaseModelError> {
    let root = object(value, "runtime_compatibility")?;
    reject_unknown_fields(
        root,
        &["runtime_lock_sha256", "runtime_role", "profile_format", "browser_identity_policy"],
        "runtime_compatibility",
    )?;
    let runtime_lock_sha256 = required_string(root, "runtime_lock_sha256")?;
    validate_sha256_like(&runtime_lock_sha256, "runtime_compatibility.runtime_lock_sha256")?;
    Ok(RuntimeCompatibilityIdentity {
        runtime_lock_sha256,
        runtime_role: non_empty_string(root, "runtime_role")?,
        profile_format: non_empty_string(root, "profile_format")?,
        browser_identity_policy: non_empty_string(root, "browser_identity_policy")?,
    })
}

fn parse_build_provenance(value: &Value) -> Result<BuildProvenanceIdentity, ReleaseModelError> {
    let root = object(value, "build_provenance")?;
    reject_unknown_fields(
        root,
        &["cargo_lock_sha256", "rust_toolchain_sha256", "frontend_lock_sha256", "release_architecture_sha256"],
        "build_provenance",
    )?;
    let cargo_lock_sha256 = required_string(root, "cargo_lock_sha256")?;
    let rust_toolchain_sha256 = required_string(root, "rust_toolchain_sha256")?;
    let frontend_lock_sha256 = required_string(root, "frontend_lock_sha256")?;
    let release_architecture_sha256 = required_string(root, "release_architecture_sha256")?;
    for (value, field) in [
        (&cargo_lock_sha256, "build_provenance.cargo_lock_sha256"),
        (&rust_toolchain_sha256, "build_provenance.rust_toolchain_sha256"),
        (&frontend_lock_sha256, "build_provenance.frontend_lock_sha256"),
        (&release_architecture_sha256, "build_provenance.release_architecture_sha256"),
    ] {
        validate_sha256_like(value, field)?;
    }
    Ok(BuildProvenanceIdentity {
        cargo_lock_sha256,
        rust_toolchain_sha256,
        frontend_lock_sha256,
        release_architecture_sha256,
    })
}

fn parse_artifact_inventory(value: &Value) -> Result<Vec<ArtifactIdentity>, ReleaseModelError> {
    let values = array(value, "artifact_inventory")?;
    if values.is_empty() {
        return Err(ReleaseModelError::new("artifact_inventory must not be empty"));
    }
    let mut paths = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let item = object(value, "artifact_inventory entry")?;
        reject_unknown_fields(item, &["path", "sha256", "size_bytes", "kind"], "artifact_inventory entry")?;
        let path = required_string(item, "path")?;
        validate_artifact_path(&path)?;
        if !paths.insert(path.clone()) {
            return Err(ReleaseModelError::new(format!("duplicate artifact path: {path}")));
        }
        let sha256 = required_string(item, "sha256")?;
        validate_sha256_like(&sha256, &format!("artifact_inventory.{path}.sha256"))?;
        let size_bytes = required_u64(item, "size_bytes")?;
        if size_bytes == 0 {
            return Err(ReleaseModelError::new(format!("artifact {path} has zero size")));
        }
        let kind = required_string(item, "kind")?;
        if !matches!(kind.as_str(), "component" | "contract" | "runtime" | "manifest" | "sbom") {
            return Err(ReleaseModelError::new(format!("unknown artifact kind for {path}: {kind}")));
        }
        result.push(ArtifactIdentity { path, sha256, size_bytes, kind });
    }
    result.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(result)
}

fn validate_component_artifacts(
    components: &BTreeMap<String, ReleaseComponentIdentity>,
    inventory: &[ArtifactIdentity],
) -> Result<(), ReleaseModelError> {
    let by_path = inventory
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
        if artifact.kind != "component"
            || artifact.sha256 != component.artifact_sha256
            || artifact.size_bytes != component.artifact_size_bytes
        {
            return Err(ReleaseModelError::new(format!(
                "component {} artifact identity disagrees with artifact_inventory",
                component.component_id
            )));
        }
        let component_manifest = by_path.get(component.component_manifest_path.as_str()).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest is absent from artifact_inventory",
                component.component_id
            ))
        })?;
        if component_manifest.kind != "manifest"
            || component_manifest.sha256 != component.component_manifest_sha256
        {
            return Err(ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest identity disagrees with artifact_inventory",
                component.component_id
            )));
        }
    }
    Ok(())
}

pub fn validate_artifact_path(path: &str) -> Result<(), ReleaseModelError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.split('/').any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path.contains(':')
    {
        return Err(ReleaseModelError::new(format!("unsafe artifact path: {path:?}")));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseModelError {
    message: String,
}

impl ReleaseModelError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for ReleaseModelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseModelError {}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object.get(key).ok_or_else(|| ReleaseModelError::new(format!("missing required field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?.as_str().map(ToOwned::to_owned).ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
}

fn non_empty_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    let value = required_string(object, key)?;
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!("field {key} must not be empty")));
    }
    Ok(value)
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, ReleaseModelError> {
    object.get(key).map(|value| value.as_str().map(ToOwned::to_owned).ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))).transpose()
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?.as_bool().ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a boolean")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?.as_u64().ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn required_string_array(object: &Map<String, Value>, key: &str) -> Result<Vec<String>, ReleaseModelError> {
    let values = array(required(object, key)?, key)?;
    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let text = value.as_str().ok_or_else(|| ReleaseModelError::new(format!("field {key} must contain strings")))?;
        if text.trim().is_empty() || !seen.insert(text.to_owned()) {
            return Err(ReleaseModelError::new(format!("field {key} contains empty/duplicate {text:?}")));
        }
        output.push(text.to_owned());
    }
    output.sort();
    Ok(output)
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value.as_object().ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON object")))
}

fn array<'a>(value: &'a Value, context: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value.as_array().ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON array")))
}

fn reject_unknown_fields(object: &Map<String, Value>, allowed: &[&str], context: &str) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!("unknown field in {context}: {key}")));
        }
    }
    Ok(())
}

fn validate_release_set_id(value: &str) -> Result<(), ReleaseModelError> {
    let digest = value.strip_prefix(RELEASE_SET_ID_PREFIX).ok_or_else(|| {
        ReleaseModelError::new("release_set_id must use the only supported release-set-v2-sha256 prefix")
    })?;
    validate_sha256_like(digest, "release_set_id digest")
}

fn validate_git_sha(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(ReleaseModelError::new(format!("{field} must be exactly 40 lowercase hexadecimal characters")));
    }
    Ok(())
}

fn validate_sha256_like(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(ReleaseModelError::new(format!("{field} must be exactly 64 lowercase hexadecimal characters")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityDecision, RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
    use crate::release::digest::{canonical_json, sha256_hex};
    use serde_json::{Value, json};

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn accepted_main_evidence() -> Result<String, String> {
        let identity = json!({"authority":"accepted-main","commit_sha":GIT_SHA,"repository":REPOSITORY});
        Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
    }

    fn component(release_id: &str, artifact: &str, digest: &str, size: u64, manifest: &str) -> Value {
        json!({
            "release_id": release_id,
            "source_commit_sha": GIT_SHA,
            "artifact_path": artifact,
            "artifact_sha256": digest,
            "artifact_size_bytes": size,
            "component_manifest_path": manifest,
            "component_manifest_sha256": SHA_A
        })
    }

    fn schema(component: &str) -> Value {
        json!({
            "database_component": component,
            "target_schema_revision": "0001_initial.sql",
            "supported_schema_min": "0001_initial.sql",
            "supported_schema_max": "0001_initial.sql",
            "migration_history_digest": SHA_A,
            "compatibility_policy_digest": SHA_B
        })
    }

    fn fixture() -> Result<Value, String> {
        Ok(json!({
          "schema_version": 2,
          "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
          "display_version": "test",
          "source": {"repository":REPOSITORY,"commit_sha":GIT_SHA,"accepted_main":true,"accepted_main_evidence_sha256":accepted_main_evidence()?},
          "components": {
            "control_plane": component("control-plane-v1", "components/control-plane.tar", SHA_A, 10, "control-plane-manifest.json"),
            "secret_resolver": component("resolver-v1", "components/resolver.tar", SHA_B, 11, "secret-resolver-manifest.json"),
            "runtime_bundle": component("runtime-v1", "components/runtime.tar", SHA_C, 12, "runtime-bundle-manifest.json")
          },
          "contracts": {"files":[{"path":"openapi/v1/openapi.json","sha256":SHA_A,"size_bytes":10}],"sha256":SHA_B},
          "protocols": {"public_api_contract_sha256":SHA_B,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
          "schemas": {"d1_evolution_authority_sha256":SHA_A,"catalog":schema("catalog"),"resolver":schema("resolver")},
          "runtime_compatibility": {"runtime_lock_sha256":SHA_A,"runtime_role":"real_camoufox","profile_format":"v1","browser_identity_policy":"v1"},
          "capability_profile_compatibility": ["rehearsal-core-v1"],
          "build_provenance": {"cargo_lock_sha256":SHA_A,"rust_toolchain_sha256":SHA_A,"frontend_lock_sha256":SHA_A,"release_architecture_sha256":SHA_A},
          "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":SHA_A,"size_bytes":10,"kind":"component"},
            {"path":"components/resolver.tar","sha256":SHA_B,"size_bytes":11,"kind":"component"},
            {"path":"components/runtime.tar","sha256":SHA_C,"size_bytes":12,"kind":"component"},
            {"path":"control-plane-manifest.json","sha256":SHA_A,"size_bytes":20,"kind":"manifest"},
            {"path":"secret-resolver-manifest.json","sha256":SHA_A,"size_bytes":20,"kind":"manifest"},
            {"path":"runtime-bundle-manifest.json","sha256":SHA_A,"size_bytes":20,"kind":"manifest"}
          ]
        }))
    }

    fn signed_fixture() -> Result<String, String> {
        let mut complete = fixture()?;
        let mut identity = complete.clone();
        identity.as_object_mut().ok_or_else(|| "fixture root must be object".to_owned())?.remove("release_set_id");
        identity.as_object_mut().ok_or_else(|| "fixture root must be object".to_owned())?.remove("display_version");
        let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
        complete["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
        serde_json::to_string(&complete).map_err(|error| error.to_string())
    }

    #[test]
    fn parses_v2_and_rejects_v1() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = ReleaseSetManifest::parse_json(&signed_fixture()?)?;
        assert_eq!(parsed.schema_version, 2);
        let v1 = signed_fixture()?.replace("\"schema_version\":2", "\"schema_version\":1").replace("release-set-v2-sha256-", "release-set-v1-sha256-");
        assert!(ReleaseSetManifest::parse_json(&v1).is_err());
        Ok(())
    }

    #[test]
    fn rejects_component_from_different_source_sha() -> Result<(), String> {
        let mut value: Value = serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["components"]["control_plane"]["source_commit_sha"] = Value::String("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned());
        assert!(ReleaseSetManifest::parse_json(&serde_json::to_string(&value).map_err(|error| error.to_string())?).is_err());
        Ok(())
    }

    #[test]
    fn rejects_missing_manifest_inventory_binding() -> Result<(), String> {
        let mut value: Value = serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["components"]["control_plane"]["component_manifest_path"] = Value::String("missing.json".to_owned());
        assert!(ReleaseSetManifest::parse_json(&serde_json::to_string(&value).map_err(|error| error.to_string())?).is_err());
        Ok(())
    }

    #[test]
    fn unknown_compatibility_is_not_compatible() -> Result<(), Box<dyn std::error::Error>> {
        assert!(!CompatibilityDecision::parse("UNKNOWN")?.is_compatible());
        Ok(())
    }
}
