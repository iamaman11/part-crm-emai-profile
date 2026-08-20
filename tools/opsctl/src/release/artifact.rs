use crate::release::digest::sha256_reader_hex;
use crate::release::model::{ArtifactIdentity, ReleaseModelError, ReleaseSetManifest};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactVerification {
    pub verified_files: usize,
    pub verified_bytes: u64,
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
        let missing = expected_paths
            .difference(&observed_paths)
            .copied()
            .collect::<Vec<_>>();
        let unexpected = observed_paths
            .difference(&expected_paths)
            .copied()
            .collect::<Vec<_>>();
        return Err(ReleaseModelError::new(format!(
            "ARTIFACT_INVENTORY_MISMATCH: missing={missing:?} unexpected={unexpected:?}"
        )));
    }

    let mut verified_bytes = 0_u64;
    for (relative, path) in observed {
        let artifact = expected.get(relative.as_str()).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "unexpected artifact after inventory comparison: {relative}"
            ))
        })?;
        verify_one(path.as_path(), artifact)?;
        verified_bytes = verified_bytes
            .checked_add(artifact.size_bytes)
            .ok_or_else(|| ReleaseModelError::new("artifact byte total overflow"))?;
    }

    Ok(ArtifactVerification {
        verified_files: manifest.artifact_inventory.len(),
        verified_bytes,
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
        .map_err(|error| {
            ReleaseModelError::new(format!("ARTIFACT_DIRECTORY_READ_FAILED: {error}"))
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            ReleaseModelError::new(format!(
                "ARTIFACT_METADATA_FAILED: {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ReleaseModelError::new(format!(
                "ARTIFACT_SYMLINK_FORBIDDEN: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            visit_directory(root, &path, output)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(ReleaseModelError::new(format!(
                "ARTIFACT_TYPE_INVALID: {}",
                path.display()
            )));
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            ReleaseModelError::new(format!("artifact path escaped root: {error}"))
        })?;
        let mut components = Vec::new();
        for component in relative.components() {
            let text = component
                .as_os_str()
                .to_str()
                .ok_or_else(|| ReleaseModelError::new("artifact path must be valid UTF-8"))?;
            components.push(text);
        }
        let canonical = components.join("/");
        if output.insert(canonical.clone(), path).is_some() {
            return Err(ReleaseModelError::new(format!(
                "duplicate observed artifact path: {canonical}"
            )));
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

    fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
        // Test thread names contain `::`, which is not a valid Windows path component.
        // Labels are unique per test, and the process id isolates concurrent cargo invocations.
        let path = std::env::temp_dir().join(format!("opsctl-ar11-{label}-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(path.join("components"))?;
        Ok(path)
    }

    fn accepted_main_evidence() -> Result<String, String> {
        let identity = json!({
            "authority": "accepted-main",
            "commit_sha": GIT_SHA,
            "repository": REPOSITORY,
        });
        Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
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
        let sha_a = sha256_hex(b"control");
        let sha_b = sha256_hex(b"resolver");
        let sha_c = sha256_hex(b"runtime");
        let evidence = accepted_main_evidence()?;
        let component_manifest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let mut value = json!({
          "schema_version": 1,
          "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{evidence}"),
          "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": evidence
          },
          "components": {
            "control_plane": {"release_id":"cp","source_commit_sha":GIT_SHA,"artifact_path":"components/control-plane.tar","artifact_sha256":sha_a,"artifact_size_bytes":7,"component_manifest_sha256":component_manifest},
            "secret_resolver": {"release_id":"rs","source_commit_sha":GIT_SHA,"artifact_path":"components/resolver.tar","artifact_sha256":sha_b,"artifact_size_bytes":8,"component_manifest_sha256":component_manifest},
            "runtime_bundle": {"release_id":"rt","source_commit_sha":GIT_SHA,"artifact_path":"components/runtime.tar","artifact_sha256":sha_c,"artifact_size_bytes":7,"component_manifest_sha256":component_manifest}
          },
          "contracts": {}, "protocols": {}, "schemas": {}, "runtime_compatibility": {},
          "capability_profile_compatibility": ["rehearsal-core-v1"],
          "build_provenance": {},
          "artifact_inventory": [
            {"path":"components/control-plane.tar","sha256":sha_a,"size_bytes":7,"kind":"component"},
            {"path":"components/resolver.tar","sha256":sha_b,"size_bytes":8,"kind":"component"},
            {"path":"components/runtime.tar","sha256":sha_c,"size_bytes":7,"kind":"component"}
          ]
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or("identity must be object")?
            .remove("release_set_id");
        let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
        value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
        Ok(value)
    }

    fn manifest(root: &Path) -> Result<ReleaseSetManifest, Box<dyn std::error::Error>> {
        let value = manifest_value(root)?;
        Ok(ReleaseSetManifest::parse_json(&serde_json::to_string(
            &value,
        )?)?)
    }

    #[test]
    fn verifies_exact_bounded_artifact_tree() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("exact")?;
        let manifest = manifest(&root)?;
        let result = verify_artifacts(&manifest, &root)?;
        assert_eq!(result.verified_files, 3);
        assert_eq!(result.verified_bytes, 22);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn verifies_colocated_release_set_control_document() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("control-document")?;
        let value = manifest_value(&root)?;
        let input = serde_json::to_string_pretty(&value)? + "\n";
        let manifest = ReleaseSetManifest::parse_json(&input)?;
        fs::write(root.join("release-set.json"), input)?;
        let result = verify_artifacts(&manifest, &root)?;
        assert_eq!(result.verified_files, 3);
        assert_eq!(result.verified_bytes, 22);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_mismatched_colocated_release_set_control_document()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("control-mismatch")?;
        let manifest = manifest(&root)?;
        fs::write(root.join("release-set.json"), "{}\n")?;
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

    #[test]
    fn rejects_digest_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("digest")?;
        let manifest = manifest(&root)?;
        fs::write(root.join("components/control-plane.tar"), b"CONTROL")?;
        assert!(verify_artifacts(&manifest, &root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
