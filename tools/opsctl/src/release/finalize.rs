use crate::canonical::{canonical_json, parse_strict_json, sha256_hex};
use crate::d1;
use crate::release::authority::ReleaseArchitecture;
use crate::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use crate::release::model;
use opsctl_core::release::{
    ArtifactIdentity, BuildProvenanceIdentity, ContractsIdentity, EXPECTED_REPOSITORY,
    ProtocolIdentity, ProvenanceFileIdentity, ReleaseComponentIdentity, ReleaseModelError,
    ReleaseSetDraft, ReleaseSetSchemaVersion, ReleaseSetSource, RuntimeCompatibilityIdentity,
    SchemaCompatibilityWindow, SchemaIdentity,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const TRANSPORT_SCHEMA_VERSION: u64 = 1;
const TRANSPORT_KIND: &str = "RELEASE_FINALIZE_REQUEST";
const COMPONENT_KIND: &str = "component";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseFinalizeRequestV1 {
    schema_version: u64,
    kind: String,
    source_commit_sha: String,
    profile_bridge_protocol_version: u64,
    components: ComponentObservationsV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentObservationsV1 {
    control_plane: ComponentObservationV1,
    secret_resolver: ComponentObservationV1,
    runtime_bundle: ComponentObservationV1,
    profile_bridge: ComponentObservationV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentObservationV1 {
    release_id: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    component_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeLockFactsV1 {
    camouhost_ipc_version: u64,
    runtime_role: String,
    fingerprint_config_schema: String,
    fingerprint_policy_version: String,
}

/// Finalize an ephemeral packaging observation DTO into the current canonical Release Set.
///
/// The request deliberately has its own transport version and contains no Release Set schema
/// version, id prefix, semantic identity field list, D1 policy, capability profile policy or
/// provenance policy. Those facts are composed from their natural typed/local owners here and
/// validated by the dependency-free pure Release Set core before rendering.
pub fn finalize_json(root: &Path, input: &str) -> Result<String, ReleaseModelError> {
    let request = parse_request(input)?;
    let draft = build_draft(root, request)?;
    let identity = identity_value(&draft);
    let canonical_identity = canonical_json(&identity).map_err(ReleaseModelError::new)?;
    let release_set_id = format!(
        "{}{}",
        ReleaseSetSchemaVersion::CURRENT.id_prefix(),
        sha256_hex(canonical_identity.as_bytes())
    );
    let manifest = draft.clone().into_manifest(release_set_id.clone())?;

    let mut output_value = identity;
    let output_object = output_value.as_object_mut().ok_or_else(|| {
        ReleaseModelError::new("typed Release Set identity must render as an object")
    })?;
    output_object.insert("release_set_id".to_owned(), Value::String(release_set_id));
    let output = canonical_json(&output_value).map_err(ReleaseModelError::new)?;

    // The reader is an independent fail-closed verification path over the final external bytes.
    // This catches any accidental adapter drift without making the rendered JSON a semantic input.
    let reparsed = model::parse_json(&output)?;
    if reparsed != manifest {
        return Err(ReleaseModelError::new(
            "final Release Set bytes do not round-trip to the typed semantic result",
        ));
    }
    Ok(output)
}

fn parse_request(input: &str) -> Result<ReleaseFinalizeRequestV1, ReleaseModelError> {
    let value = parse_strict_json(input).map_err(|error| {
        ReleaseModelError::new(format!("invalid ReleaseFinalizeRequestV1 JSON: {error}"))
    })?;
    let request: ReleaseFinalizeRequestV1 = serde_json::from_value(value).map_err(|error| {
        ReleaseModelError::new(format!("invalid ReleaseFinalizeRequestV1 DTO: {error}"))
    })?;
    if request.schema_version != TRANSPORT_SCHEMA_VERSION || request.kind != TRANSPORT_KIND {
        return Err(ReleaseModelError::new(format!(
            "unsupported ReleaseFinalizeRequest transport: kind={} schema_version={}",
            request.kind, request.schema_version
        )));
    }
    Ok(request)
}

fn build_draft(
    root: &Path,
    request: ReleaseFinalizeRequestV1,
) -> Result<ReleaseSetDraft, ReleaseModelError> {
    let topology = ReleaseInputTopology::load(root)?;
    let resolved = topology.resolve(root)?;
    let architecture = ReleaseArchitecture::load(root).map_err(|error| {
        ReleaseModelError::new(format!("release architecture invalid: {error}"))
    })?;

    let mut contract_files = resolved
        .iter()
        .filter(|input| input.input.consumed_by("release_set.contracts"))
        .map(|input| ProvenanceFileIdentity {
            path: input.input.release_identity_source.clone(),
            sha256: input.sha256.clone(),
            size_bytes: input.size_bytes,
        })
        .collect::<Vec<_>>();
    if contract_files.is_empty() {
        return Err(ReleaseModelError::new(
            "canonical release input topology has no release_set.contracts inputs",
        ));
    }
    contract_files.sort_by(|left, right| left.path.cmp(&right.path));
    let contracts_sha = sha256_hex(
        canonical_json(&provenance_files_value(&contract_files))
            .map_err(ReleaseModelError::new)?
            .as_bytes(),
    );
    let contracts = ContractsIdentity {
        files: contract_files,
        sha256: contracts_sha,
    };

    let runtime_input = resolved_input(&resolved, "camouhost_runtime_lock")?;
    let runtime_lock = runtime_lock_facts(&runtime_input.absolute_path)?;
    let catalog = d1_schema_window(root, "catalog")?;
    let resolver = d1_schema_window(root, "resolver")?;
    let source = ReleaseSetSource {
        repository: EXPECTED_REPOSITORY.to_owned(),
        commit_sha: request.source_commit_sha.clone(),
        accepted_main: true,
        accepted_main_evidence_sha256: accepted_main_evidence(&request.source_commit_sha)?,
    };

    let mut components = BTreeMap::new();
    components.insert(
        "control_plane".to_owned(),
        component_identity(
            "control_plane",
            "components/control-plane.tar",
            &request.source_commit_sha,
            &request.components.control_plane,
        ),
    );
    components.insert(
        "frontend".to_owned(),
        component_identity(
            "frontend",
            "components/control-plane.tar",
            &request.source_commit_sha,
            &request.components.control_plane,
        ),
    );
    components.insert(
        "secret_resolver".to_owned(),
        component_identity(
            "secret_resolver",
            "components/secret-resolver.tar",
            &request.source_commit_sha,
            &request.components.secret_resolver,
        ),
    );
    components.insert(
        "runtime_bundle".to_owned(),
        component_identity(
            "runtime_bundle",
            "components/runtime-bundle.tar",
            &request.source_commit_sha,
            &request.components.runtime_bundle,
        ),
    );
    components.insert(
        "profile_bridge".to_owned(),
        component_identity(
            "profile_bridge",
            "components/profile-bridge.zip",
            &request.source_commit_sha,
            &request.components.profile_bridge,
        ),
    );

    let artifacts = vec![
        artifact_identity(
            "components/control-plane.tar",
            &request.components.control_plane,
        ),
        artifact_identity(
            "components/secret-resolver.tar",
            &request.components.secret_resolver,
        ),
        artifact_identity(
            "components/runtime-bundle.tar",
            &request.components.runtime_bundle,
        ),
        artifact_identity(
            "components/profile-bridge.zip",
            &request.components.profile_bridge,
        ),
    ];

    ReleaseSetDraft::new(
        source,
        components,
        contracts.clone(),
        ProtocolIdentity {
            public_api_contract_sha256: contracts.sha256,
            camouhost_ipc_version: runtime_lock.camouhost_ipc_version,
            profile_bridge_protocol_version: request.profile_bridge_protocol_version,
            resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
        },
        SchemaIdentity {
            d1_repository_identity_sha256: d1::repository_identity_sha256(root).map_err(
                |error| {
                    ReleaseModelError::new(format!("typed D1 repository identity failed: {error}"))
                },
            )?,
            catalog,
            resolver,
        },
        RuntimeCompatibilityIdentity {
            runtime_lock_sha256: runtime_input.sha256.clone(),
            runtime_role: runtime_lock.runtime_role,
            profile_format: runtime_lock.fingerprint_config_schema,
            browser_identity_policy: runtime_lock.fingerprint_policy_version,
        },
        architecture.profiles.keys().cloned().collect(),
        BuildProvenanceIdentity {
            cargo_lock_sha256: resolved_input(&resolved, "cargo_lock")?.sha256.clone(),
            rust_toolchain_sha256: resolved_input(&resolved, "rust_toolchain")?.sha256.clone(),
            frontend_lock_sha256: resolved_input(&resolved, "frontend_lock")?.sha256.clone(),
            release_architecture_sha256: resolved_input(
                &resolved,
                "release_architecture_authority",
            )?
            .sha256
            .clone(),
        },
        artifacts,
    )
}

fn component_identity(
    component_id: &str,
    artifact_path: &str,
    source_commit_sha: &str,
    observation: &ComponentObservationV1,
) -> ReleaseComponentIdentity {
    ReleaseComponentIdentity {
        component_id: component_id.to_owned(),
        release_id: observation.release_id.clone(),
        source_commit_sha: source_commit_sha.to_owned(),
        artifact_path: artifact_path.to_owned(),
        artifact_sha256: observation.artifact_sha256.clone(),
        artifact_size_bytes: observation.artifact_size_bytes,
        component_manifest_sha256: observation.component_manifest_sha256.clone(),
    }
}

fn artifact_identity(path: &str, observation: &ComponentObservationV1) -> ArtifactIdentity {
    ArtifactIdentity {
        path: path.to_owned(),
        sha256: observation.artifact_sha256.clone(),
        size_bytes: observation.artifact_size_bytes,
        kind: COMPONENT_KIND.to_owned(),
    }
}

fn d1_schema_window(
    root: &Path,
    component: &str,
) -> Result<SchemaCompatibilityWindow, ReleaseModelError> {
    let identity = d1::release_schema_identity(root, component).map_err(|error| {
        ReleaseModelError::new(format!(
            "typed D1 {component} release identity failed: {error}"
        ))
    })?;
    Ok(SchemaCompatibilityWindow {
        database_component: identity.database_component,
        target_schema_revision: identity.target_schema_revision,
        supported_schema_min: identity.supported_schema_min,
        supported_schema_max: identity.supported_schema_max,
        migration_history_digest: identity.migration_history_digest,
        compatibility_policy_digest: identity.compatibility_policy_digest,
    })
}

fn accepted_main_evidence(source_sha: &str) -> Result<String, ReleaseModelError> {
    let identity = json!({
        "authority": "accepted-main",
        "commit_sha": source_sha,
        "repository": EXPECTED_REPOSITORY,
    });
    canonical_json(&identity)
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(ReleaseModelError::new)
}

fn runtime_lock_facts(path: &Path) -> Result<RuntimeLockFactsV1, ReleaseModelError> {
    let input = fs::read_to_string(path).map_err(|error| {
        ReleaseModelError::new(format!("cannot read canonical runtime lock: {error}"))
    })?;
    let value = parse_strict_json(&input)
        .map_err(|error| ReleaseModelError::new(format!("invalid runtime lock JSON: {error}")))?;
    let root = exact_object(
        &value,
        "runtime lock",
        &[
            "browser",
            "camouhost_ipc_version",
            "components",
            "fingerprint_config_schema",
            "fingerprint_policy_version",
            "python",
            "python_source",
            "runtime_role",
            "schema_version",
        ],
    )?;
    if required_u64(root, "schema_version", "runtime lock")? != 1 {
        return Err(ReleaseModelError::new(
            "unsupported runtime lock schema_version",
        ));
    }

    let browser = exact_object(
        required(root, "browser", "runtime lock")?,
        "runtime lock browser",
        &["release_commit", "repository", "version"],
    )?;
    for key in ["release_commit", "repository", "version"] {
        required_string(browser, key, "runtime lock browser")?;
    }
    let components = exact_object(
        required(root, "components", "runtime lock")?,
        "runtime lock components",
        &["browserforge", "camoufox_python", "playwright"],
    )?;
    for key in ["browserforge", "camoufox_python", "playwright"] {
        required_string(components, key, "runtime lock components")?;
    }
    required_string(root, "python", "runtime lock")?;
    let python_source = exact_object(
        required(root, "python_source", "runtime lock")?,
        "runtime lock python_source",
        &["commit", "repository"],
    )?;
    for key in ["commit", "repository"] {
        required_string(python_source, key, "runtime lock python_source")?;
    }

    Ok(RuntimeLockFactsV1 {
        camouhost_ipc_version: required_u64(root, "camouhost_ipc_version", "runtime lock")?,
        runtime_role: required_string(root, "runtime_role", "runtime lock")?.to_owned(),
        fingerprint_config_schema: required_string(
            root,
            "fingerprint_config_schema",
            "runtime lock",
        )?
        .to_owned(),
        fingerprint_policy_version: required_string(
            root,
            "fingerprint_policy_version",
            "runtime lock",
        )?
        .to_owned(),
    })
}

fn exact_object<'a>(
    value: &'a Value,
    label: &str,
    fields: &[&str],
) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON object")))?;
    let expected = fields.iter().copied().collect::<BTreeSet<_>>();
    let observed = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if observed != expected {
        return Err(ReleaseModelError::new(format!(
            "{label} field inventory drifted: expected={expected:?} observed={observed:?}"
        )));
    }
    Ok(object)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("{label} missing field: {key}")))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a str, ReleaseModelError> {
    required(object, key, label)?
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ReleaseModelError::new(format!("{label}.{key} must be a non-empty string")))
}

fn required_u64(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u64, ReleaseModelError> {
    required(object, key, label)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("{label}.{key} must be an unsigned integer")))
}

fn resolved_input<'a>(
    resolved: &'a [ResolvedReleaseInput],
    input_id: &str,
) -> Result<&'a ResolvedReleaseInput, ReleaseModelError> {
    resolved
        .iter()
        .find(|input| input.input.input_id == input_id)
        .ok_or_else(|| {
            ReleaseModelError::new(format!("canonical release input missing: {input_id}"))
        })
}

fn provenance_files_value(files: &[ProvenanceFileIdentity]) -> Value {
    Value::Array(
        files
            .iter()
            .map(|entry| {
                json!({
                    "path": entry.path,
                    "sha256": entry.sha256,
                    "size_bytes": entry.size_bytes,
                })
            })
            .collect(),
    )
}

fn schema_window_value(window: &SchemaCompatibilityWindow) -> Value {
    json!({
        "database_component": window.database_component,
        "target_schema_revision": window.target_schema_revision,
        "supported_schema_min": window.supported_schema_min,
        "supported_schema_max": window.supported_schema_max,
        "migration_history_digest": window.migration_history_digest,
        "compatibility_policy_digest": window.compatibility_policy_digest,
    })
}

fn identity_value(draft: &ReleaseSetDraft) -> Value {
    let components = draft
        .components
        .iter()
        .map(|(id, component)| {
            (
                id.clone(),
                json!({
                    "release_id": component.release_id,
                    "source_commit_sha": component.source_commit_sha,
                    "artifact_path": component.artifact_path,
                    "artifact_sha256": component.artifact_sha256,
                    "artifact_size_bytes": component.artifact_size_bytes,
                    "component_manifest_sha256": component.component_manifest_sha256,
                }),
            )
        })
        .collect::<Map<String, Value>>();
    let artifacts = draft
        .artifact_inventory
        .iter()
        .map(|artifact| {
            json!({
                "path": artifact.path,
                "sha256": artifact.sha256,
                "size_bytes": artifact.size_bytes,
                "kind": artifact.kind,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": draft.schema_version.number(),
        "source": {
            "repository": draft.source.repository,
            "commit_sha": draft.source.commit_sha,
            "accepted_main": draft.source.accepted_main,
            "accepted_main_evidence_sha256": draft.source.accepted_main_evidence_sha256,
        },
        "components": components,
        "contracts": {
            "files": provenance_files_value(&draft.contracts.files),
            "sha256": draft.contracts.sha256,
        },
        "protocols": {
            "public_api_contract_sha256": draft.protocols.public_api_contract_sha256,
            "camouhost_ipc_version": draft.protocols.camouhost_ipc_version,
            "profile_bridge_protocol_version": draft.protocols.profile_bridge_protocol_version,
            "resolver_protocol": draft.protocols.resolver_protocol,
        },
        "schemas": {
            "d1_repository_identity_sha256": draft.schemas.d1_repository_identity_sha256,
            "catalog": schema_window_value(&draft.schemas.catalog),
            "resolver": schema_window_value(&draft.schemas.resolver),
        },
        "runtime_compatibility": {
            "runtime_lock_sha256": draft.runtime_compatibility.runtime_lock_sha256,
            "runtime_role": draft.runtime_compatibility.runtime_role,
            "profile_format": draft.runtime_compatibility.profile_format,
            "browser_identity_policy": draft.runtime_compatibility.browser_identity_policy,
        },
        "capability_profile_compatibility": draft.capability_profile_compatibility,
        "build_provenance": {
            "cargo_lock_sha256": draft.build_provenance.cargo_lock_sha256,
            "rust_toolchain_sha256": draft.build_provenance.rust_toolchain_sha256,
            "frontend_lock_sha256": draft.build_provenance.frontend_lock_sha256,
            "release_architecture_sha256": draft.build_provenance.release_architecture_sha256,
        },
        "artifact_inventory": artifacts,
    })
}

#[cfg(test)]
mod tests {
    use super::finalize_json;
    use crate::release::model;
    use opsctl_core::release::MAX_JCS_SAFE_INTEGER;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn observation(release_id: &str) -> Value {
        json!({
            "release_id": release_id,
            "artifact_sha256": SHA,
            "artifact_size_bytes": 1,
            "component_manifest_sha256": SHA,
        })
    }

    fn request_value() -> Value {
        json!({
            "schema_version": 1,
            "kind": "RELEASE_FINALIZE_REQUEST",
            "source_commit_sha": GIT,
            "profile_bridge_protocol_version": 1,
            "components": {
                "control_plane": observation("control-plane-v2"),
                "secret_resolver": observation("resolver-v2"),
                "runtime_bundle": observation("runtime-v2"),
                "profile_bridge": observation("bridge-v2"),
            }
        })
    }

    #[test]
    fn finalizer_produces_reader_verified_release_set_v3() -> Result<(), Box<dyn std::error::Error>>
    {
        let input = serde_json::to_string(&request_value())?;
        let output = finalize_json(&root(), &input)?;
        let manifest = model::parse_json(&output)?;
        assert_eq!(manifest.schema_version.number(), 3);
        assert!(
            manifest
                .release_set_id
                .starts_with("release-set-v3-sha256-")
        );
        assert_eq!(
            manifest.components["control_plane"].release_id,
            "control-plane-v2"
        );
        assert_eq!(
            manifest.components["secret_resolver"].release_id,
            "resolver-v2"
        );
        assert_eq!(
            manifest.components["runtime_bundle"].release_id,
            "runtime-v2"
        );
        assert_eq!(
            manifest.components["profile_bridge"].release_id,
            "bridge-v2"
        );
        assert_eq!(
            manifest
                .capability_profile_compatibility
                .first()
                .map(String::as_str),
            Some("production-core-v1")
        );
        Ok(())
    }

    #[test]
    fn transport_version_is_independent_and_fail_closed() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut request = request_value();
        request["schema_version"] = json!(2);
        assert!(finalize_json(&root(), &serde_json::to_string(&request)?).is_err());

        let mut coupled = request_value();
        coupled["release_set_schema_version"] = json!(3);
        assert!(finalize_json(&root(), &serde_json::to_string(&coupled)?).is_err());
        Ok(())
    }

    #[test]
    fn transport_rejects_duplicate_keys_and_unsafe_integers()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = serde_json::to_string(&request_value())?;
        let duplicate = input.replacen('{', "{\"schema_version\":1,", 1);
        assert!(finalize_json(&root(), &duplicate).is_err());

        let mut unsafe_integer = request_value();
        unsafe_integer["components"]["control_plane"]["artifact_size_bytes"] =
            json!(MAX_JCS_SAFE_INTEGER + 1);
        assert!(finalize_json(&root(), &serde_json::to_string(&unsafe_integer)?).is_err());
        Ok(())
    }

    #[test]
    fn transport_whitespace_cannot_change_final_identity() -> Result<(), Box<dyn std::error::Error>>
    {
        let request = request_value();
        let compact = finalize_json(&root(), &serde_json::to_string(&request)?)?;
        let pretty = finalize_json(&root(), &serde_json::to_string_pretty(&request)?)?;
        assert_eq!(compact, pretty);
        Ok(())
    }
}
