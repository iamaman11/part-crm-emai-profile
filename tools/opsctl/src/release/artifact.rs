use crate::release::component_manifest::verify_component_manifests;
use crate::release::digest::sha256_reader_hex;
use crate::release::model::{ArtifactIdentity, ReleaseModelError, ReleaseSetManifest, parse_json};
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
    let component_manifests = verify_component_manifests(manifest, artifact_root)?;
    Ok(ArtifactVerification {
        verified_files: manifest.artifact_inventory.len(),
        verified_bytes,
        verified_components: component_manifests.verified_components,
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
    let control = parse_json(&input).map_err(|error| {
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
