mod request;
mod runtime_lock;
#[cfg(test)]
mod tests;

use crate::canonical::{canonical_json, sha256_hex};
use crate::d1;
use crate::release::authority::ReleaseArchitecture;
use crate::release::input_topology::{ReleaseInputTopology, ResolvedReleaseInput};
use crate::release::v3_output::render_release_set_v3;
use opsctl_core::release as core;
use request::ReleaseFinalizeRequestV1;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::Path;

const COMPONENT_ARTIFACT_KIND: &str = "component";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFinalizeError {
    message: String,
}

impl ReleaseFinalizeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReleaseFinalizeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseFinalizeError {}

#[derive(Serialize)]
struct ProvenanceDigestRow<'a> {
    path: &'a str,
    sha256: &'a str,
    size_bytes: u64,
}

/// Compose explicit packaging observations with canonical local release facts and finalize v3.
///
/// This is an outer adapter/composition boundary only. It performs local observation and typed
/// owner projection, then delegates all Release Set aggregate semantics to `opsctl-core::release`
/// and all durable representation/content addressing to the v3 output adapter. It has no
/// network/provider/process/mutation authority.
pub fn finalize_json(root: &Path, input: &str) -> Result<String, ReleaseFinalizeError> {
    let request = request::parse(input)?;
    let release_set = compose_release_set(root, request)?;
    let rendered = render_release_set_v3(&release_set).map_err(|error| {
        ReleaseFinalizeError::new(format!("Release Set v3 rendering failed: {error}"))
    })?;
    String::from_utf8(rendered.canonical_document_bytes).map_err(|error| {
        ReleaseFinalizeError::new(format!(
            "canonical Release Set v3 output is not UTF-8: {error}"
        ))
    })
}

fn compose_release_set(
    root: &Path,
    request: ReleaseFinalizeRequestV1,
) -> Result<core::ReleaseSetV3, ReleaseFinalizeError> {
    let topology = ReleaseInputTopology::load(root).map_err(|error| {
        ReleaseFinalizeError::new(format!("release input topology invalid: {error}"))
    })?;
    let resolved = topology.resolve(root).map_err(|error| {
        ReleaseFinalizeError::new(format!("release input observation failed: {error}"))
    })?;

    let contracts = contracts_identity(&resolved)?;
    let public_api_root = resolved_input(&resolved, "public_api_root")?;
    let runtime_input = resolved_input(&resolved, "camouhost_runtime_lock")?;
    let runtime = runtime_lock::load(&runtime_input.absolute_path)?;
    let architecture = ReleaseArchitecture::load(root).map_err(|error| {
        ReleaseFinalizeError::new(format!("release architecture invalid: {error}"))
    })?;
    let catalog = d1_schema_window(root, "catalog")?;
    let resolver = d1_schema_window(root, "resolver")?;
    let d1_repository_identity_sha256 = d1::repository_identity_sha256(root).map_err(|error| {
        ReleaseFinalizeError::new(format!("typed D1 repository identity failed: {error}"))
    })?;
    let (components, artifact_inventory) = component_identities(&request);

    core::ReleaseSetV3::new(core::ReleaseSetV3Parts {
        source: core::ReleaseSetSource {
            repository: request.source.repository,
            commit_sha: request.source.commit_sha,
            accepted_main: request.source.accepted_main,
            accepted_main_evidence_sha256: request.source.accepted_main_evidence_sha256,
        },
        components,
        contracts,
        protocols: core::ProtocolIdentity {
            public_api_contract_sha256: public_api_root.sha256.clone(),
            camouhost_ipc_version: runtime.camouhost_ipc_version,
            profile_bridge_protocol_version: request.protocols.profile_bridge_protocol_version,
            resolver_protocol: request.protocols.resolver_protocol,
        },
        schemas: core::SchemaIdentity {
            d1_repository_identity_sha256,
            catalog,
            resolver,
        },
        runtime_compatibility: core::RuntimeCompatibilityIdentity {
            runtime_lock_sha256: runtime_input.sha256.clone(),
            runtime_role: runtime.runtime_role,
            profile_format: runtime.profile_format,
            browser_identity_policy: runtime.browser_identity_policy,
        },
        capability_profile_compatibility: architecture.profiles.keys().cloned().collect(),
        build_provenance: core::BuildProvenanceIdentity {
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
        artifact_inventory,
    })
    .map_err(|error| {
        ReleaseFinalizeError::new(format!("Release Set v3 semantic validation failed: {error}"))
    })
}

fn contracts_identity(
    resolved: &[ResolvedReleaseInput],
) -> Result<core::ContractsIdentity, ReleaseFinalizeError> {
    let mut files = resolved
        .iter()
        .filter(|input| input.input.consumed_by("release_set.contracts"))
        .map(|input| core::ProvenanceFileIdentity {
            path: input.input.release_identity_source.clone(),
            sha256: input.sha256.clone(),
            size_bytes: input.size_bytes,
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Err(ReleaseFinalizeError::new(
            "canonical release input topology has no release_set.contracts inputs",
        ));
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let digest_rows = files
        .iter()
        .map(|file| ProvenanceDigestRow {
            path: &file.path,
            sha256: &file.sha256,
            size_bytes: file.size_bytes,
        })
        .collect::<Vec<_>>();
    let value = serde_json::to_value(&digest_rows).map_err(|error| {
        ReleaseFinalizeError::new(format!("cannot serialize contract identity scope: {error}"))
    })?;
    let canonical = canonical_json(&value).map_err(|error| {
        ReleaseFinalizeError::new(format!("cannot canonicalize contract identity scope: {error}"))
    })?;

    Ok(core::ContractsIdentity {
        files,
        sha256: sha256_hex(canonical.as_bytes()),
    })
}

fn component_identities(
    request: &ReleaseFinalizeRequestV1,
) -> (
    BTreeMap<String, core::ReleaseComponentIdentity>,
    Vec<core::ArtifactIdentity>,
) {
    let mut components = BTreeMap::new();
    let mut artifacts = BTreeMap::<String, core::ArtifactIdentity>::new();

    for (component_key, observation) in &request.components {
        components.insert(
            component_key.clone(),
            core::ReleaseComponentIdentity {
                component_id: observation.component_id.clone(),
                release_id: observation.release_id.clone(),
                source_commit_sha: observation.source_commit_sha.clone(),
                artifact_path: observation.artifact_path.clone(),
                artifact_sha256: observation.artifact_sha256.clone(),
                artifact_size_bytes: observation.artifact_size_bytes,
                component_manifest_sha256: observation.component_manifest_sha256.clone(),
            },
        );

        let candidate = core::ArtifactIdentity {
            path: observation.artifact_path.clone(),
            sha256: observation.artifact_sha256.clone(),
            size_bytes: observation.artifact_size_bytes,
            kind: COMPONENT_ARTIFACT_KIND.to_owned(),
        };
        artifacts.entry(candidate.path.clone()).or_insert(candidate);
    }

    (components, artifacts.into_values().collect())
}

fn d1_schema_window(
    root: &Path,
    component: &str,
) -> Result<core::SchemaCompatibilityWindow, ReleaseFinalizeError> {
    let identity = d1::release_schema_identity(root, component).map_err(|error| {
        ReleaseFinalizeError::new(format!("typed D1 {component} release identity failed: {error}"))
    })?;
    Ok(core::SchemaCompatibilityWindow {
        database_component: identity.database_component,
        target_schema_revision: identity.target_schema_revision,
        supported_schema_min: identity.supported_schema_min,
        supported_schema_max: identity.supported_schema_max,
        migration_history_digest: identity.migration_history_digest,
        compatibility_policy_digest: identity.compatibility_policy_digest,
    })
}

fn resolved_input<'a>(
    resolved: &'a [ResolvedReleaseInput],
    input_id: &str,
) -> Result<&'a ResolvedReleaseInput, ReleaseFinalizeError> {
    resolved
        .iter()
        .find(|input| input.input.input_id == input_id)
        .ok_or_else(|| {
            ReleaseFinalizeError::new(format!("canonical release input missing: {input_id}"))
        })
}
