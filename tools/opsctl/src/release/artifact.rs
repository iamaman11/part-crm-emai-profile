use crate::release::digest::{sha256_hex, sha256_reader_hex};
use crate::release::model::{ArtifactIdentity, ReleaseModelError, ReleaseSetManifest};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactVerification {
    pub verified_files: usize,
    pub verified_bytes: u64,
    pub verified_components: Vec<String>,
}

pub fn verify_artifacts(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<ArtifactVerification, ReleaseModelError> {
    let root_metadata = fs::symlink_metadata(artifact_root).map_err(|error| {
        ReleaseModelError::new(format!(
            "ARTIFACT_ROOT_UNAVAILABLE: {}: {error}",
            artifact_root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ReleaseModelError::new(
            "ARTIFACT_ROOT_INVALID: artifact root must be a real directory",
        ));
    }

    let expected = manifest
        .artifact_inventory
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    let mut observed = collect_files(artifact_root)?;
    remove_verified_control_manifest(manifest, &mut observed)?;
    let expected_paths = expected.keys().copied().collect::<BTreeSet<_>>();
    let observed_paths = observed.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if expected_paths != observed_paths {
        let missing = expected_paths.difference(&observed_paths).copied().collect::<Vec<_>>();
        let unexpected = observed_paths.difference(&expected_paths).copied().collect::<Vec<_>>();
        return Err(ReleaseModelError::new(format!(
            "ARTIFACT_INVENTORY_MISMATCH: missing={missing:?} unexpected={unexpected:?}"
        )));
    }

    let mut verified_bytes = 0_u64;
    for (relative, path) in &observed {
        let artifact = expected.get(relative.as_str()).ok_or_else(|| {
            ReleaseModelError::new(format!("unexpected artifact after inventory comparison: {relative}"))
        })?;
        verify_one(path.as_path(), artifact)?;
        verified_bytes = verified_bytes
            .checked_add(artifact.size_bytes)
            .ok_or_else(|| ReleaseModelError::new("artifact byte total overflow"))?;
    }

    let verified_components = verify_component_manifests(manifest, artifact_root)?;
    Ok(ArtifactVerification {
        verified_files: manifest.artifact_inventory.len(),
        verified_bytes,
        verified_components,
    })
}

fn remove_verified_control_manifest(
    manifest: &ReleaseSetManifest,
    observed: &mut BTreeMap<String, PathBuf>,
) -> Result<(), ReleaseModelError> {
    let Some(path) = observed.get("release-set.json").cloned() else {
        return Ok(());
    };
    let input = fs::read_to_string(&path).map_err(|error| {
        ReleaseModelError::new(format!(
            "RELEASE_SET_CONTROL_DOCUMENT_READ_FAILED: {}: {error}",
            path.display()
        ))
    })?;
    let control = ReleaseSetManifest::parse_json(&input).map_err(|error| {
        ReleaseModelError::new(format!(
            "RELEASE_SET_CONTROL_DOCUMENT_INVALID: {}: {error}",
            path.display()
        ))
    })?;
    if control != *manifest {
        return Err(ReleaseModelError::new(
            "RELEASE_SET_CONTROL_DOCUMENT_MISMATCH: artifact root release-set.json differs from verified manifest",
        ));
    }
    observed.remove("release-set.json");
    Ok(())
}

fn verify_one(path: &Path, artifact: &ArtifactIdentity) -> Result<(), ReleaseModelError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ReleaseModelError::new(format!("ARTIFACT_MISSING: {}: {error}", artifact.path))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ReleaseModelError::new(format!(
            "ARTIFACT_TYPE_INVALID: {} must be a regular file",
            artifact.path
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Err(ReleaseModelError::new(format!(
            "ARTIFACT_SIZE_MISMATCH: {} expected={} observed={}",
            artifact.path,
            artifact.size_bytes,
            metadata.len()
        )));
    }
    let mut file = File::open(path).map_err(|error| {
        ReleaseModelError::new(format!("ARTIFACT_READ_FAILED: {}: {error}", artifact.path))
    })?;
    let digest = sha256_reader_hex(&mut file).map_err(|error| {
        ReleaseModelError::new(format!("ARTIFACT_READ_FAILED: {}: {error}", artifact.path))
    })?;
    if digest != artifact.sha256 {
        return Err(ReleaseModelError::new(format!(
            "ARTIFACT_DIGEST_MISMATCH: {} expected={} observed={digest}",
            artifact.path, artifact.sha256
        )));
    }
    Ok(())
}

fn verify_component_manifests(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<Vec<String>, ReleaseModelError> {
    let mut verified = Vec::with_capacity(manifest.components.len());
    for component in manifest.components.values() {
        let path = artifact_root.join(&component.component_manifest_path);
        let bytes = fs::read(&path).map_err(|error| {
            ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: cannot read {} for {}: {error}",
                component.component_manifest_path, component.component_id
            ))
        })?;
        let digest = sha256_hex(&bytes);
        if digest != component.component_manifest_sha256 {
            return Err(ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest digest expected={} observed={digest}",
                component.component_id, component.component_manifest_sha256
            )));
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest is invalid JSON: {error}",
                component.component_id
            ))
        })?;
        let object = value.as_object().ok_or_else(|| {
            ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest root must be an object",
                component.component_id
            ))
        })?;
        let release_id = object.get("release_id").and_then(Value::as_str).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} manifest lacks release_id",
                component.component_id
            ))
        })?;
        if release_id != component.release_id {
            return Err(ReleaseModelError::new(format!(
                "COMPONENT_MANIFEST_MISMATCH: component {} release_id differs from durable manifest",
                component.component_id
            )));
        }
        let source_sha = object
            .get("source")
            .and_then(Value::as_object)
            .and_then(|source| source.get("commit_sha"))
            .and_then(Value::as_str)
            .or_else(|| object.get("source_commit_sha").and_then(Value::as_str))
            .ok_or_else(|| {
                ReleaseModelError::new(format!(
                    "COMPONENT_MANIFEST_MISMATCH: component {} manifest lacks source SHA",
                    component.component_id
                ))
            })?;
        if source_sha != manifest.source.commit_sha || source_sha != component.source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {} durable manifest source differs from Release Set",
                component.component_id
            )));
        }
        verified.push(component.component_id.clone());
    }
    verified.sort();
    Ok(verified)
}

fn collect_files(root: &Path) -> Result<BTreeMap<String, PathBuf>, ReleaseModelError> {
    let mut output = BTreeMap::new();
    visit_directory(root, root, &mut output)?;
    Ok(output)
}

fn visit_directory(
    root: &Path,
    current: &Path,
    output: &mut BTreeMap<String, PathBuf>,
) -> Result<(), ReleaseModelError> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            ReleaseModelError::new(format!(
                "ARTIFACT_DIRECTORY_READ_FAILED: {}: {error}",
                current.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ReleaseModelError::new(format!("ARTIFACT_DIRECTORY_READ_FAILED: {error}")))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            ReleaseModelError::new(format!("ARTIFACT_METADATA_FAILED: {}: {error}", path.display()))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ReleaseModelError::new(format!("ARTIFACT_SYMLINK_FORBIDDEN: {}", path.display())));
        }
        if metadata.is_dir() {
            visit_directory(root, &path, output)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(ReleaseModelError::new(format!("ARTIFACT_TYPE_INVALID: {}", path.display())));
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            ReleaseModelError::new(format!("artifact path escaped root: {error}"))
        })?;
        let mut components = Vec::new();
        for component in relative.components() {
            let text = component.as_os_str().to_str().ok_or_else(|| {
                ReleaseModelError::new("artifact path must be valid UTF-8")
            })?;
            components.push(text);
        }
        let canonical = components.join("/");
        if output.insert(canonical.clone(), path).is_some() {
            return Err(ReleaseModelError::new(format!("duplicate observed artifact path: {canonical}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::verify_artifacts;
    use crate::release::digest::{canonical_json, sha256_hex};
    use crate::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
    use serde_json::{Value, json};
    use std::fs;
    use std::path::{Path, PathBuf};

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
        let path = std::env::temp_dir().join(format!("opsctl-ar11-v2-{label}-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(path.join("components"))?;
        Ok(path)
    }

    fn accepted_main_evidence() -> Result<String, String> {
        let identity = json!({"authority":"accepted-main","commit_sha":GIT_SHA,"repository":REPOSITORY});
        Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
    }

    fn schema(component: &str) -> Value {
        json!({
            "database_component":component,
            "target_schema_revision":"0001_initial.sql",
            "supported_schema_min":"0001_initial.sql",
            "supported_schema_max":"0001_initial.sql",
            "migration_history_digest":SHA_A,
            "compatibility_policy_digest":SHA_A
        })
    }

    fn manifest_value(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
        let files = [
            ("control-plane.tar", b"control".as_slice()),
            ("resolver.tar", b"resolver".as_slice()),
            ("runtime.tar", b"runtime".as_slice()),
        ];
        for (name, bytes) in files {
            fs::write(root.join("components").join(name), bytes)?;
        }
        let manifests = [
            ("control-plane-manifest.json", "cp"),
            ("secret-resolver-manifest.json", "rs"),
            ("runtime-bundle-manifest.json", "rt"),
        ];
        let mut manifest_digests = std::collections::BTreeMap::new();
        let mut manifest_sizes = std::collections::BTreeMap::new();
        for (name, release_id) in manifests {
            let bytes = serde_json::to_vec(&json!({"release_id":release_id,"source_commit_sha":GIT_SHA}))?;
            fs::write(root.join(name), &bytes)?;
            manifest_digests.insert(name, sha256_hex(&bytes));
            manifest_sizes.insert(name, bytes.len() as u64);
        }
        let sha_a = sha256_hex(b"control");
        let sha_b = sha256_hex(b"resolver");
        let sha_c = sha256_hex(b"runtime");
        let evidence = accepted_main_evidence()?;
        let component = |release_id: &str, artifact: &str, artifact_sha: &str, artifact_size: u64, manifest: &str| json!({
            "release_id":release_id,"source_commit_sha":GIT_SHA,"artifact_path":artifact,"artifact_sha256":artifact_sha,"artifact_size_bytes":artifact_size,
            "component_manifest_path":manifest,"component_manifest_sha256":manifest_digests[manifest]
        });
        let mut value = json!({
          "schema_version":2,
          "release_set_id":format!("{RELEASE_SET_ID_PREFIX}{evidence}"),
          "source":{"repository":REPOSITORY,"commit_sha":GIT_SHA,"accepted_main":true,"accepted_main_evidence_sha256":evidence},
          "components":{
            "control_plane":component("cp","components/control-plane.tar",&sha_a,7,"control-plane-manifest.json"),
            "secret_resolver":component("rs","components/resolver.tar",&sha_b,8,"secret-resolver-manifest.json"),
            "runtime_bundle":component("rt","components/runtime.tar",&sha_c,7,"runtime-bundle-manifest.json")
          },
          "contracts":{"files":[{"path":"openapi/v1/openapi.json","sha256":SHA_A,"size_bytes":1}],"sha256":SHA_A},
          "protocols":{"public_api_contract_sha256":SHA_A,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
          "schemas":{"d1_evolution_authority_sha256":SHA_A,"catalog":schema("catalog"),"resolver":schema("resolver")},
          "runtime_compatibility":{"runtime_lock_sha256":SHA_A,"runtime_role":"real_camoufox","profile_format":"v1","browser_identity_policy":"v1"},
          "capability_profile_compatibility":["rehearsal-core-v1"],
          "build_provenance":{"cargo_lock_sha256":SHA_A,"rust_toolchain_sha256":SHA_A,"frontend_lock_sha256":SHA_A,"release_architecture_sha256":SHA_A},
          "artifact_inventory":[
            {"path":"components/control-plane.tar","sha256":sha_a,"size_bytes":7,"kind":"component"},
            {"path":"components/resolver.tar","sha256":sha_b,"size_bytes":8,"kind":"component"},
            {"path":"components/runtime.tar","sha256":sha_c,"size_bytes":7,"kind":"component"},
            {"path":"control-plane-manifest.json","sha256":manifest_digests["control-plane-manifest.json"],"size_bytes":manifest_sizes["control-plane-manifest.json"],"kind":"manifest"},
            {"path":"secret-resolver-manifest.json","sha256":manifest_digests["secret-resolver-manifest.json"],"size_bytes":manifest_sizes["secret-resolver-manifest.json"],"kind":"manifest"},
            {"path":"runtime-bundle-manifest.json","sha256":manifest_digests["runtime-bundle-manifest.json"],"size_bytes":manifest_sizes["runtime-bundle-manifest.json"],"kind":"manifest"}
          ]
        });
        let mut identity = value.clone();
        identity.as_object_mut().ok_or("identity must be object")?.remove("release_set_id");
        let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
        value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
        Ok(value)
    }

    fn manifest(root: &Path) -> Result<ReleaseSetManifest, Box<dyn std::error::Error>> {
        Ok(ReleaseSetManifest::parse_json(&serde_json::to_string(&manifest_value(root)?)?)?)
    }

    #[test]
    fn verifies_exact_bounded_tree_and_durable_manifests() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("exact")?;
        let manifest = manifest(&root)?;
        let result = verify_artifacts(&manifest, &root)?;
        assert_eq!(result.verified_files, 6);
        assert_eq!(result.verified_components, vec!["control_plane", "runtime_bundle", "secret_resolver"]);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_manifest_release_identity_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("manifest-mismatch")?;
        let manifest = manifest(&root)?;
        fs::write(root.join("control-plane-manifest.json"), serde_json::to_vec(&json!({"release_id":"other","source_commit_sha":GIT_SHA}))?)?;
        assert!(verify_artifacts(&manifest, &root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_unexpected_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("unexpected")?;
        let manifest = manifest(&root)?;
        fs::write(root.join("secret.txt"), b"secret")?;
        assert!(verify_artifacts(&manifest, &root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
