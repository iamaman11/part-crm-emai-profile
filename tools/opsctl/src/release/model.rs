use crate::canonical::parse_strict_json;
use crate::release::digest::{canonical_json, sha256_hex};
pub use opsctl_core::release::{
    ArtifactIdentity, BuildProvenanceIdentity, CompatibilityDecision, ContractsIdentity,
    EXPECTED_REPOSITORY, ProtocolIdentity, ProvenanceFileIdentity, RELEASE_SET_ID_PREFIX_V3,
    ReleaseComponentIdentity, ReleaseModelError, ReleaseSetManifest, ReleaseSetSchemaVersion,
    ReleaseSetSource, RuntimeCompatibilityIdentity, SchemaCompatibilityWindow, SchemaIdentity,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const RELEASE_SET_SCHEMA_VERSION: u64 = ReleaseSetSchemaVersion::CURRENT.number();
pub const RELEASE_SET_ID_PREFIX: &str = RELEASE_SET_ID_PREFIX_V3;
const REQUIRED_COMPONENTS: [&str; 3] = ["control_plane", "secret_resolver", "runtime_bundle"];
const ALLOWED_COMPONENTS: [&str; 5] = [
    "control_plane",
    "frontend",
    "secret_resolver",
    "runtime_bundle",
    "profile_bridge",
];
const MAX_JCS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn parse_json(input: &str) -> Result<ReleaseSetManifest, ReleaseModelError> {
    let value = parse_strict_json(input)
        .map_err(|error| ReleaseModelError::new(format!("invalid release-set JSON: {error}")))?;
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
    let schema_version =
        ReleaseSetSchemaVersion::from_number(required_u64(root, "schema_version")?)?;
    let release_set_id = required_string(root, "release_set_id")?;
    validate_release_set_id(&release_set_id, schema_version)?;
    let display_version = optional_string(root, "display_version")?;
    let source = parse_source(required(root, "source")?)?;
    let components = parse_components(required(root, "components")?, &source.commit_sha)?;
    let artifact_inventory = parse_artifact_inventory(required(root, "artifact_inventory")?)?;
    validate_component_artifacts(&components, &artifact_inventory)?;
    let contracts = parse_contracts(required(root, "contracts")?)?;
    let protocols = parse_protocols(required(root, "protocols")?)?;
    let schemas = parse_schemas(required(root, "schemas")?)?;
    let runtime_compatibility = parse_runtime(required(root, "runtime_compatibility")?)?;
    let capability_profile_compatibility =
        required_string_array(root, "capability_profile_compatibility")?;
    if capability_profile_compatibility.is_empty() {
        return Err(ReleaseModelError::new(
            "capability_profile_compatibility must not be empty",
        ));
    }
    let build_provenance = parse_build_provenance(required(root, "build_provenance")?)?;
    let manifest = ReleaseSetManifest {
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
    };
    verify_content_address(&manifest)?;
    Ok(manifest)
}

pub fn verify_content_address(manifest: &ReleaseSetManifest) -> Result<(), ReleaseModelError> {
    let identity = ReleaseSetIdentityV3::from_manifest(manifest);
    let value = serde_json::to_value(identity).map_err(|error| {
        ReleaseModelError::new(format!(
            "cannot encode typed Release Set v3 identity: {error}"
        ))
    })?;
    let canonical = canonical_json(&value).map_err(ReleaseModelError::new)?;
    let expected = format!(
        "{}{}",
        manifest.schema_version.id_prefix(),
        sha256_hex(canonical.as_bytes())
    );
    if manifest.release_set_id != expected {
        return Err(ReleaseModelError::new(format!(
            "RELEASE_IDENTITY_MISMATCH: expected {expected}, observed {}",
            manifest.release_set_id
        )));
    }
    Ok(())
}

#[derive(Serialize)]
struct ReleaseSetIdentityV3<'a> {
    schema_version: u64,
    source: SourceIdentityV3<'a>,
    components: BTreeMap<&'a str, ComponentIdentityV3<'a>>,
    contracts: ContractsIdentityV3<'a>,
    protocols: ProtocolIdentityV3<'a>,
    schemas: SchemasIdentityV3<'a>,
    runtime_compatibility: RuntimeIdentityV3<'a>,
    capability_profile_compatibility: Vec<&'a str>,
    build_provenance: BuildIdentityV3<'a>,
    artifact_inventory: Vec<ArtifactIdentityV3<'a>>,
}

impl<'a> ReleaseSetIdentityV3<'a> {
    fn from_manifest(manifest: &'a ReleaseSetManifest) -> Self {
        Self {
            schema_version: manifest.schema_version.number(),
            source: SourceIdentityV3 {
                repository: &manifest.source.repository,
                commit_sha: &manifest.source.commit_sha,
                accepted_main: manifest.source.accepted_main,
                accepted_main_evidence_sha256: &manifest.source.accepted_main_evidence_sha256,
            },
            components: manifest
                .components
                .iter()
                .map(|(id, component)| {
                    (
                        id.as_str(),
                        ComponentIdentityV3 {
                            release_id: &component.release_id,
                            source_commit_sha: &component.source_commit_sha,
                            artifact_path: &component.artifact_path,
                            artifact_sha256: &component.artifact_sha256,
                            artifact_size_bytes: component.artifact_size_bytes,
                            component_manifest_sha256: &component.component_manifest_sha256,
                        },
                    )
                })
                .collect(),
            contracts: ContractsIdentityV3 {
                files: manifest
                    .contracts
                    .files
                    .iter()
                    .map(|entry| ProvenanceFileIdentityV3 {
                        path: &entry.path,
                        sha256: &entry.sha256,
                        size_bytes: entry.size_bytes,
                    })
                    .collect(),
                sha256: &manifest.contracts.sha256,
            },
            protocols: ProtocolIdentityV3 {
                public_api_contract_sha256: &manifest.protocols.public_api_contract_sha256,
                camouhost_ipc_version: manifest.protocols.camouhost_ipc_version,
                profile_bridge_protocol_version: manifest.protocols.profile_bridge_protocol_version,
                resolver_protocol: &manifest.protocols.resolver_protocol,
            },
            schemas: SchemasIdentityV3 {
                d1_repository_identity_sha256: &manifest.schemas.d1_repository_identity_sha256,
                catalog: SchemaWindowV3::from_window(&manifest.schemas.catalog),
                resolver: SchemaWindowV3::from_window(&manifest.schemas.resolver),
            },
            runtime_compatibility: RuntimeIdentityV3 {
                runtime_lock_sha256: &manifest.runtime_compatibility.runtime_lock_sha256,
                runtime_role: &manifest.runtime_compatibility.runtime_role,
                profile_format: &manifest.runtime_compatibility.profile_format,
                browser_identity_policy: &manifest.runtime_compatibility.browser_identity_policy,
            },
            capability_profile_compatibility: manifest
                .capability_profile_compatibility
                .iter()
                .map(String::as_str)
                .collect(),
            build_provenance: BuildIdentityV3 {
                cargo_lock_sha256: &manifest.build_provenance.cargo_lock_sha256,
                rust_toolchain_sha256: &manifest.build_provenance.rust_toolchain_sha256,
                frontend_lock_sha256: &manifest.build_provenance.frontend_lock_sha256,
                release_architecture_sha256: &manifest.build_provenance.release_architecture_sha256,
            },
            artifact_inventory: manifest
                .artifact_inventory
                .iter()
                .map(|entry| ArtifactIdentityV3 {
                    path: &entry.path,
                    sha256: &entry.sha256,
                    size_bytes: entry.size_bytes,
                    kind: &entry.kind,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct SourceIdentityV3<'a> {
    repository: &'a str,
    commit_sha: &'a str,
    accepted_main: bool,
    accepted_main_evidence_sha256: &'a str,
}

#[derive(Serialize)]
struct ComponentIdentityV3<'a> {
    release_id: &'a str,
    source_commit_sha: &'a str,
    artifact_path: &'a str,
    artifact_sha256: &'a str,
    artifact_size_bytes: u64,
    component_manifest_sha256: &'a str,
}

#[derive(Serialize)]
struct ProvenanceFileIdentityV3<'a> {
    path: &'a str,
    sha256: &'a str,
    size_bytes: u64,
}

#[derive(Serialize)]
struct ContractsIdentityV3<'a> {
    files: Vec<ProvenanceFileIdentityV3<'a>>,
    sha256: &'a str,
}

#[derive(Serialize)]
struct ProtocolIdentityV3<'a> {
    public_api_contract_sha256: &'a str,
    camouhost_ipc_version: u64,
    profile_bridge_protocol_version: u64,
    resolver_protocol: &'a str,
}

#[derive(Serialize)]
struct SchemaWindowV3<'a> {
    database_component: &'a str,
    target_schema_revision: &'a str,
    supported_schema_min: &'a str,
    supported_schema_max: &'a str,
    migration_history_digest: &'a str,
    compatibility_policy_digest: &'a str,
}

impl<'a> SchemaWindowV3<'a> {
    fn from_window(window: &'a SchemaCompatibilityWindow) -> Self {
        Self {
            database_component: &window.database_component,
            target_schema_revision: &window.target_schema_revision,
            supported_schema_min: &window.supported_schema_min,
            supported_schema_max: &window.supported_schema_max,
            migration_history_digest: &window.migration_history_digest,
            compatibility_policy_digest: &window.compatibility_policy_digest,
        }
    }
}

#[derive(Serialize)]
struct SchemasIdentityV3<'a> {
    d1_repository_identity_sha256: &'a str,
    catalog: SchemaWindowV3<'a>,
    resolver: SchemaWindowV3<'a>,
}

#[derive(Serialize)]
struct RuntimeIdentityV3<'a> {
    runtime_lock_sha256: &'a str,
    runtime_role: &'a str,
    profile_format: &'a str,
    browser_identity_policy: &'a str,
}

#[derive(Serialize)]
struct BuildIdentityV3<'a> {
    cargo_lock_sha256: &'a str,
    rust_toolchain_sha256: &'a str,
    frontend_lock_sha256: &'a str,
    release_architecture_sha256: &'a str,
}

#[derive(Serialize)]
struct ArtifactIdentityV3<'a> {
    path: &'a str,
    sha256: &'a str,
    size_bytes: u64,
    kind: &'a str,
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
    let identity = serde_json::json!({
        "authority": "accepted-main",
        "commit_sha": commit_sha,
        "repository": repository,
    });
    let canonical = canonical_json(&identity).map_err(ReleaseModelError::new)?;
    if accepted_main_evidence_sha256 != sha256_hex(canonical.as_bytes()) {
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
            return Err(ReleaseModelError::new(format!(
                "unknown component: {component_id}"
            )));
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
                "component_manifest_sha256",
            ],
            &format!("components.{component_id}"),
        )?;
        let release_id = non_empty_string(component, "release_id")?;
        let component_source = required_string(component, "source_commit_sha")?;
        validate_git_sha(
            &component_source,
            &format!("components.{component_id}.source_commit_sha"),
        )?;
        if component_source != source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {component_id} source SHA differs from release source"
            )));
        }
        let artifact_path = required_string(component, "artifact_path")?;
        validate_artifact_path(&artifact_path)?;
        let artifact_sha256 = required_string(component, "artifact_sha256")?;
        validate_sha256_like(
            &artifact_sha256,
            &format!("components.{component_id}.artifact_sha256"),
        )?;
        let artifact_size_bytes = required_jcs_u64(component, "artifact_size_bytes")?;
        if artifact_size_bytes == 0 {
            return Err(ReleaseModelError::new(format!(
                "components.{component_id}.artifact_size_bytes must be positive"
            )));
        }
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
        reject_unknown_fields(
            entry,
            &["path", "sha256", "size_bytes"],
            "contracts.files entry",
        )?;
        let path = required_string(entry, "path")?;
        validate_artifact_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err(ReleaseModelError::new(format!(
                "duplicate contracts path: {path}"
            )));
        }
        let sha256 = required_string(entry, "sha256")?;
        validate_sha256_like(&sha256, "contracts.files.sha256")?;
        let size_bytes = required_jcs_u64(entry, "size_bytes")?;
        if size_bytes == 0 {
            return Err(ReleaseModelError::new(
                "contracts file size must be positive",
            ));
        }
        parsed.push(ProvenanceFileIdentity {
            path,
            sha256,
            size_bytes,
        });
    }
    parsed.sort_by(|left, right| left.path.cmp(&right.path));
    let sha256 = required_string(root, "sha256")?;
    validate_sha256_like(&sha256, "contracts.sha256")?;
    Ok(ContractsIdentity {
        files: parsed,
        sha256,
    })
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
    validate_sha256_like(
        &public_api_contract_sha256,
        "protocols.public_api_contract_sha256",
    )?;
    let camouhost_ipc_version = required_jcs_u64(root, "camouhost_ipc_version")?;
    let profile_bridge_protocol_version =
        required_jcs_u64(root, "profile_bridge_protocol_version")?;
    if camouhost_ipc_version == 0 || profile_bridge_protocol_version == 0 {
        return Err(ReleaseModelError::new("protocol versions must be positive"));
    }
    let resolver_protocol = non_empty_string(root, "resolver_protocol")?;
    Ok(ProtocolIdentity {
        public_api_contract_sha256,
        camouhost_ipc_version,
        profile_bridge_protocol_version,
        resolver_protocol,
    })
}

fn parse_schema_window(
    value: &Value,
    expected_component: &str,
) -> Result<SchemaCompatibilityWindow, ReleaseModelError> {
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
    if supported_schema_min > supported_schema_max
        || target_schema_revision < supported_schema_min
        || target_schema_revision > supported_schema_max
    {
        return Err(ReleaseModelError::new(format!(
            "SCHEMA_IDENTITY_MISMATCH: invalid compatibility window for {expected_component}"
        )));
    }
    let migration_history_digest = required_string(root, "migration_history_digest")?;
    validate_sha256_like(
        &migration_history_digest,
        "schemas.migration_history_digest",
    )?;
    let compatibility_policy_digest = required_string(root, "compatibility_policy_digest")?;
    validate_sha256_like(
        &compatibility_policy_digest,
        "schemas.compatibility_policy_digest",
    )?;
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
    reject_unknown_fields(
        root,
        &["d1_repository_identity_sha256", "catalog", "resolver"],
        "schemas",
    )?;
    let d1_repository_identity_sha256 = required_string(root, "d1_repository_identity_sha256")?;
    validate_sha256_like(
        &d1_repository_identity_sha256,
        "schemas.d1_repository_identity_sha256",
    )?;
    Ok(SchemaIdentity {
        d1_repository_identity_sha256,
        catalog: parse_schema_window(required(root, "catalog")?, "catalog")?,
        resolver: parse_schema_window(required(root, "resolver")?, "resolver")?,
    })
}

fn parse_runtime(value: &Value) -> Result<RuntimeCompatibilityIdentity, ReleaseModelError> {
    let root = object(value, "runtime_compatibility")?;
    reject_unknown_fields(
        root,
        &[
            "runtime_lock_sha256",
            "runtime_role",
            "profile_format",
            "browser_identity_policy",
        ],
        "runtime_compatibility",
    )?;
    let runtime_lock_sha256 = required_string(root, "runtime_lock_sha256")?;
    validate_sha256_like(
        &runtime_lock_sha256,
        "runtime_compatibility.runtime_lock_sha256",
    )?;
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
        &[
            "cargo_lock_sha256",
            "rust_toolchain_sha256",
            "frontend_lock_sha256",
            "release_architecture_sha256",
        ],
        "build_provenance",
    )?;
    let cargo_lock_sha256 = required_string(root, "cargo_lock_sha256")?;
    let rust_toolchain_sha256 = required_string(root, "rust_toolchain_sha256")?;
    let frontend_lock_sha256 = required_string(root, "frontend_lock_sha256")?;
    let release_architecture_sha256 = required_string(root, "release_architecture_sha256")?;
    for (value, field) in [
        (&cargo_lock_sha256, "build_provenance.cargo_lock_sha256"),
        (
            &rust_toolchain_sha256,
            "build_provenance.rust_toolchain_sha256",
        ),
        (
            &frontend_lock_sha256,
            "build_provenance.frontend_lock_sha256",
        ),
        (
            &release_architecture_sha256,
            "build_provenance.release_architecture_sha256",
        ),
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
        return Err(ReleaseModelError::new(
            "artifact_inventory must not be empty",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let item = object(value, "artifact_inventory entry")?;
        reject_unknown_fields(
            item,
            &["path", "sha256", "size_bytes", "kind"],
            "artifact_inventory entry",
        )?;
        let path = required_string(item, "path")?;
        validate_artifact_path(&path)?;
        if !paths.insert(path.clone()) {
            return Err(ReleaseModelError::new(format!(
                "duplicate artifact path: {path}"
            )));
        }
        let sha256 = required_string(item, "sha256")?;
        validate_sha256_like(&sha256, &format!("artifact_inventory.{path}.sha256"))?;
        let size_bytes = required_jcs_u64(item, "size_bytes")?;
        if size_bytes == 0 {
            return Err(ReleaseModelError::new(format!(
                "artifact {path} has zero size"
            )));
        }
        let kind = required_string(item, "kind")?;
        if kind != "component" {
            return Err(ReleaseModelError::new(format!(
                "Release Set v3 artifact {path} must be a component archive, observed kind={kind}"
            )));
        }
        result.push(ArtifactIdentity {
            path,
            sha256,
            size_bytes,
            kind,
        });
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
        let artifact = by_path
            .get(component.artifact_path.as_str())
            .ok_or_else(|| {
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

pub fn validate_artifact_path(path: &str) -> Result<(), ReleaseModelError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path.contains(':')
    {
        return Err(ReleaseModelError::new(format!(
            "unsafe artifact path: {path:?}"
        )));
    }
    Ok(())
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("missing required field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
}

fn non_empty_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    let value = required_string(object, key)?;
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!(
            "field {key} must not be empty"
        )));
    }
    Ok(value)
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ReleaseModelError> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
        })
        .transpose()
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?
        .as_bool()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a boolean")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn required_jcs_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    let value = required_u64(object, key)?;
    if value > MAX_JCS_SAFE_INTEGER {
        return Err(ReleaseModelError::new(format!(
            "field {key} exceeds RFC 8785/I-JSON safe integer range"
        )));
    }
    Ok(value)
}

fn required_string_array(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ReleaseModelError> {
    let values = array(required(object, key)?, key)?;
    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| ReleaseModelError::new(format!("field {key} must contain strings")))?;
        if text.trim().is_empty() || !seen.insert(text.to_owned()) {
            return Err(ReleaseModelError::new(format!(
                "field {key} contains an empty or duplicate value"
            )));
        }
        output.push(text.to_owned());
    }
    output.sort();
    Ok(output)
}

fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON object")))
}

fn array<'a>(value: &'a Value, context: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON array")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown field in {context}: {key}"
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
    validate_sha256_like(digest, "release_set_id digest")
}

fn validate_git_sha(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256_like(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RELEASE_SET_ID_PREFIX, ReleaseSetSchemaVersion, parse_json};
    use crate::release::digest::{canonical_json, sha256_hex};
    use serde_json::{Value, json};

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REPO: &str = "iamaman11/part-crm-emai-profile";

    fn schema(component: &str) -> Value {
        json!({
            "database_component":component,
            "target_schema_revision":"0001_initial.sql",
            "supported_schema_min":"0001_initial.sql",
            "supported_schema_max":"0001_initial.sql",
            "migration_history_digest":SHA,
            "compatibility_policy_digest":SHA
        })
    }

    fn signed_fixture() -> Result<String, String> {
        let accepted = sha256_hex(
            canonical_json(
                &json!({"authority":"accepted-main","commit_sha":GIT,"repository":REPO}),
            )?
            .as_bytes(),
        );
        let component = |id: &str, path: &str| {
            json!({
                "release_id":id,
                "source_commit_sha":GIT,
                "artifact_path":path,
                "artifact_sha256":SHA,
                "artifact_size_bytes":1,
                "component_manifest_sha256":SHA
            })
        };
        let mut value = json!({
            "schema_version":3,
            "release_set_id":format!("{RELEASE_SET_ID_PREFIX}{SHA}"),
            "source":{"repository":REPO,"commit_sha":GIT,"accepted_main":true,"accepted_main_evidence_sha256":accepted},
            "components":{
                "control_plane":component("cp","components/cp.tar"),
                "secret_resolver":component("rs","components/rs.tar"),
                "runtime_bundle":component("rt","components/rt.tar")
            },
            "contracts":{"files":[{"path":"openapi/v1/openapi.json","sha256":SHA,"size_bytes":1}],"sha256":SHA},
            "protocols":{"public_api_contract_sha256":SHA,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
            "schemas":{"d1_repository_identity_sha256":SHA,"catalog":schema("catalog"),"resolver":schema("resolver")},
            "runtime_compatibility":{"runtime_lock_sha256":SHA,"runtime_role":"real_camoufox","profile_format":"v1","browser_identity_policy":"v1"},
            "capability_profile_compatibility":["rehearsal-core-v1"],
            "build_provenance":{"cargo_lock_sha256":SHA,"rust_toolchain_sha256":SHA,"frontend_lock_sha256":SHA,"release_architecture_sha256":SHA},
            "artifact_inventory":[
                {"path":"components/cp.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/rs.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/rt.tar","sha256":SHA,"size_bytes":1,"kind":"component"}
            ]
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or_else(|| "fixture root must be object".to_owned())?
            .remove("release_set_id");
        value["release_set_id"] = Value::String(format!(
            "{RELEASE_SET_ID_PREFIX}{}",
            sha256_hex(canonical_json(&identity)?.as_bytes())
        ));
        serde_json::to_string(&value).map_err(|error| error.to_string())
    }

    #[test]
    fn accepts_v3_and_rejects_v2() -> Result<(), Box<dyn std::error::Error>> {
        let input = signed_fixture()?;
        assert_eq!(
            parse_json(&input)?.schema_version,
            ReleaseSetSchemaVersion::V3
        );
        let v2 = input
            .replace("\"schema_version\":3", "\"schema_version\":2")
            .replace("release-set-v3-sha256-", "release-set-v2-sha256-");
        assert!(parse_json(&v2).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_members_fail_closed_before_semantic_decode()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = signed_fixture()?;
        let duplicate = input.replacen("{", "{\"schema_version\":3,", 1);
        assert!(parse_json(&duplicate).is_err());
        Ok(())
    }

    #[test]
    fn display_version_is_not_part_of_semantic_identity() -> Result<(), Box<dyn std::error::Error>>
    {
        let input = signed_fixture()?;
        let manifest = parse_json(&input)?;
        let mut with_display: Value = serde_json::from_str(&input)?;
        with_display["display_version"] = Value::String("human-label-only".to_owned());
        let rendered = serde_json::to_string(&with_display)?;
        let reparsed = parse_json(&rendered)?;
        assert_eq!(manifest.release_set_id, reparsed.release_set_id);
        Ok(())
    }
}
