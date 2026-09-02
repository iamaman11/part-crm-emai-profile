#![forbid(unsafe_code)]

use crate::windows_delivery::{
    DeliveryIdentity, VerifiedDeliveryCandidate, WindowsDeliveryComponent,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const STAGE_SCHEMA_VERSION: u32 = 1;
const STAGE_KIND: &str = "WINDOWS_PROFILE_BRIDGE_STAGED_DELIVERY";
const MARKER_NAME: &str = "delivery-stage-v1.json";
const BRIDGE_DIRECTORY: &str = "profile-bridge";
const RUNTIME_DIRECTORY: &str = "runtime";
const MAX_RELATIVE_PATH_BYTES: usize = 512;
const MAX_STAGE_FILES: usize = 500_000;
const MAX_STAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeliveryComponentKind {
    ProfileBridge,
    RuntimeBundle,
}

impl DeliveryComponentKind {
    const fn directory(self) -> &'static str {
        match self {
            Self::ProfileBridge => BRIDGE_DIRECTORY,
            Self::RuntimeBundle => RUNTIME_DIRECTORY,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryArchiveEntryKind {
    RegularFile,
    LinkOrSpecial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryArchiveEntry {
    relative_path: String,
    kind: DeliveryArchiveEntryKind,
    size_bytes: u64,
    sha256: String,
}

impl DeliveryArchiveEntry {
    pub fn regular_file(
        relative_path: impl Into<String>,
        size_bytes: u64,
        sha256: impl Into<String>,
    ) -> Self {
        Self {
            relative_path: relative_path.into(),
            kind: DeliveryArchiveEntryKind::RegularFile,
            size_bytes,
            sha256: sha256.into(),
        }
    }

    pub fn link_or_special(relative_path: impl Into<String>) -> Self {
        Self {
            relative_path: relative_path.into(),
            kind: DeliveryArchiveEntryKind::LinkOrSpecial,
            size_bytes: 0,
            sha256: String::new(),
        }
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn kind(&self) -> DeliveryArchiveEntryKind {
        self.kind
    }

    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Archive-format adapter used by the delivery filesystem owner.
///
/// The adapter must return every non-directory archive member. Regular files carry exact
/// manifest-owned size/SHA-256 identity; links, reparse-like entries and other special members
/// must be surfaced as `LinkOrSpecial` so the staging owner can reject them before extraction.
pub trait DeliveryArchiveReader {
    type Error;

    fn entries(
        &mut self,
        component: DeliveryComponentKind,
        artifact_path: &Path,
    ) -> Result<Vec<DeliveryArchiveEntry>, Self::Error>;

    fn copy_regular_file(
        &mut self,
        component: DeliveryComponentKind,
        artifact_path: &Path,
        entry_index: usize,
        writer: &mut dyn Write,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryStagingRoot {
    path: PathBuf,
}

impl DeliveryStagingRoot {
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self, DeliveryStagingError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(DeliveryStagingError::InvalidRoot);
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) => validate_directory_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or(DeliveryStagingError::InvalidRoot)?;
                let parent_metadata =
                    fs::symlink_metadata(parent).map_err(|_| DeliveryStagingError::Io)?;
                validate_directory_metadata(&parent_metadata)?;
                fs::create_dir(path).map_err(|_| DeliveryStagingError::Io)?;
                let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryStagingError::Io)?;
                validate_directory_metadata(&metadata)?;
            }
            Err(_) => return Err(DeliveryStagingError::Io),
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedDelivery {
    identity: DeliveryIdentity,
    path: PathBuf,
}

impl StagedDelivery {
    #[must_use]
    pub const fn identity(&self) -> &DeliveryIdentity {
        &self.identity
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn profile_bridge_root(&self) -> PathBuf {
        self.path.join(BRIDGE_DIRECTORY)
    }

    #[must_use]
    pub fn runtime_root(&self) -> PathBuf {
        self.path.join(RUNTIME_DIRECTORY)
    }
}

#[derive(Clone, Debug)]
struct PlannedFile {
    source_index: usize,
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug)]
struct ComponentPlan {
    kind: DeliveryComponentKind,
    artifact_path: PathBuf,
    release_id: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    component_manifest_sha256: String,
    files: Vec<PlannedFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StageMarker {
    schema_version: u32,
    kind: String,
    identity: DeliveryIdentity,
    profile_bridge: StageComponentMarker,
    runtime_bundle: StageComponentMarker,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StageComponentMarker {
    release_id: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    component_manifest_sha256: String,
    files: Vec<StageFileMarker>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StageFileMarker {
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

pub fn stage_verified_delivery<R: DeliveryArchiveReader>(
    root: &DeliveryStagingRoot,
    candidate: &VerifiedDeliveryCandidate,
    profile_bridge_artifact: impl AsRef<Path>,
    runtime_artifact: impl AsRef<Path>,
    reader: &mut R,
) -> Result<StagedDelivery, DeliveryStagingError> {
    validate_directory_path(root.path())?;
    let manifest = candidate.manifest();
    let bridge_plan = build_component_plan(
        DeliveryComponentKind::ProfileBridge,
        profile_bridge_artifact.as_ref(),
        &manifest.components.profile_bridge,
        reader,
    )?;
    let runtime_plan = build_component_plan(
        DeliveryComponentKind::RuntimeBundle,
        runtime_artifact.as_ref(),
        &manifest.components.runtime_bundle,
        reader,
    )?;
    let marker = StageMarker {
        schema_version: STAGE_SCHEMA_VERSION,
        kind: STAGE_KIND.to_owned(),
        identity: candidate.identity(),
        profile_bridge: marker_component(&bridge_plan),
        runtime_bundle: marker_component(&runtime_plan),
    };
    let marker_bytes =
        serde_json::to_vec(&marker).map_err(|_| DeliveryStagingError::Serialization)?;
    let directory_name = release_directory_name(candidate);
    let final_path = root.path().join(&directory_name);
    let pending_path = root.path().join(format!(".pending-{directory_name}"));

    let final_exists = safe_directory_exists(&final_path)?;
    let pending_exists = safe_directory_exists(&pending_path)?;
    if final_exists && pending_exists {
        return Err(DeliveryStagingError::AmbiguousStage);
    }
    if final_exists {
        verify_materialized_stage(&final_path, &marker_bytes, [&bridge_plan, &runtime_plan])?;
        return Ok(staged_delivery(candidate, final_path));
    }
    if pending_exists {
        match verify_materialized_stage(&pending_path, &marker_bytes, [&bridge_plan, &runtime_plan])
        {
            Ok(()) => {
                fs::rename(&pending_path, &final_path).map_err(|_| DeliveryStagingError::Io)?;
                verify_materialized_stage(
                    &final_path,
                    &marker_bytes,
                    [&bridge_plan, &runtime_plan],
                )?;
                return Ok(staged_delivery(candidate, final_path));
            }
            Err(DeliveryStagingError::CorruptStage) => {
                remove_owned_tree_if_safe(&pending_path)?;
            }
            Err(error) => return Err(error),
        }
    }

    fs::create_dir(&pending_path).map_err(|_| DeliveryStagingError::Io)?;
    validate_directory_path(&pending_path)?;
    let materialize_result = materialize_stage(
        &pending_path,
        &marker_bytes,
        [&bridge_plan, &runtime_plan],
        reader,
    );
    if let Err(error) = materialize_result {
        if !matches!(error, DeliveryStagingError::UnsafeFilesystem) {
            let _ = remove_owned_tree_if_safe(&pending_path);
        }
        return Err(error);
    }
    verify_materialized_stage(&pending_path, &marker_bytes, [&bridge_plan, &runtime_plan])?;
    if safe_directory_exists(&final_path)? {
        return Err(DeliveryStagingError::AmbiguousStage);
    }
    fs::rename(&pending_path, &final_path).map_err(|_| DeliveryStagingError::Io)?;
    verify_materialized_stage(&final_path, &marker_bytes, [&bridge_plan, &runtime_plan])?;
    Ok(staged_delivery(candidate, final_path))
}

pub fn reopen_staged_delivery(
    root: &DeliveryStagingRoot,
    identity: &DeliveryIdentity,
) -> Result<StagedDelivery, DeliveryStagingError> {
    validate_directory_path(root.path())?;
    let directory_name = release_directory_name_for_identity(identity)?;
    let final_path = root.path().join(&directory_name);
    let pending_path = root.path().join(format!(".pending-{directory_name}"));
    let final_exists = safe_directory_exists(&final_path)?;
    let pending_exists = safe_directory_exists(&pending_path)?;
    if final_exists && pending_exists {
        return Err(DeliveryStagingError::AmbiguousStage);
    }
    if pending_exists || !final_exists {
        return Err(DeliveryStagingError::CorruptStage);
    }

    let marker_path = final_path.join(MARKER_NAME);
    let marker_metadata =
        fs::symlink_metadata(&marker_path).map_err(|_| DeliveryStagingError::CorruptStage)?;
    if metadata_is_link_or_reparse(&marker_metadata) || !marker_metadata.is_file() {
        return Err(DeliveryStagingError::UnsafeFilesystem);
    }
    let marker_bytes = fs::read(&marker_path).map_err(|_| DeliveryStagingError::Io)?;
    let marker: StageMarker =
        serde_json::from_slice(&marker_bytes).map_err(|_| DeliveryStagingError::CorruptStage)?;
    let canonical_marker =
        serde_json::to_vec(&marker).map_err(|_| DeliveryStagingError::Serialization)?;
    if canonical_marker != marker_bytes
        || marker.schema_version != STAGE_SCHEMA_VERSION
        || marker.kind != STAGE_KIND
        || marker.identity != *identity
    {
        return Err(DeliveryStagingError::CorruptStage);
    }

    let bridge_plan = component_plan_from_marker(
        DeliveryComponentKind::ProfileBridge,
        &marker.profile_bridge,
        &identity.profile_bridge_release_id,
    )?;
    let runtime_plan = component_plan_from_marker(
        DeliveryComponentKind::RuntimeBundle,
        &marker.runtime_bundle,
        &identity.runtime_bundle_release_id,
    )?;
    verify_materialized_stage(&final_path, &marker_bytes, [&bridge_plan, &runtime_plan])?;
    Ok(StagedDelivery {
        identity: identity.clone(),
        path: final_path,
    })
}

fn build_component_plan<R: DeliveryArchiveReader>(
    kind: DeliveryComponentKind,
    artifact_path: &Path,
    expected: &WindowsDeliveryComponent,
    reader: &mut R,
) -> Result<ComponentPlan, DeliveryStagingError> {
    verify_artifact(artifact_path, expected)?;
    let entries = reader
        .entries(kind, artifact_path)
        .map_err(|_| DeliveryStagingError::ArchiveDecode)?;
    if entries.len() > MAX_STAGE_FILES {
        return Err(DeliveryStagingError::StageLimitExceeded);
    }
    let mut files = Vec::with_capacity(entries.len());
    let mut casefolded = HashSet::with_capacity(entries.len());
    let mut decoded_bytes = 0_u64;
    for (source_index, entry) in entries.into_iter().enumerate() {
        validate_windows_relative_path(entry.relative_path())?;
        if entry.kind() != DeliveryArchiveEntryKind::RegularFile {
            return Err(DeliveryStagingError::UnsupportedArchiveEntry);
        }
        if !is_lower_hex(entry.sha256(), 64) {
            return Err(DeliveryStagingError::InvalidEntryIdentity);
        }
        decoded_bytes = decoded_bytes
            .checked_add(entry.size_bytes())
            .ok_or(DeliveryStagingError::StageLimitExceeded)?;
        if decoded_bytes > MAX_STAGE_BYTES {
            return Err(DeliveryStagingError::StageLimitExceeded);
        }
        let folded = entry.relative_path().to_lowercase();
        if !casefolded.insert(folded) {
            return Err(DeliveryStagingError::DuplicateEntry);
        }
        files.push(PlannedFile {
            source_index,
            relative_path: entry.relative_path,
            size_bytes: entry.size_bytes,
            sha256: entry.sha256,
        });
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    validate_file_tree(&files)?;
    Ok(ComponentPlan {
        kind,
        artifact_path: artifact_path.to_path_buf(),
        release_id: expected.release_id.clone(),
        artifact_sha256: expected.artifact_sha256.clone(),
        artifact_size_bytes: expected.artifact_size_bytes,
        component_manifest_sha256: expected.component_manifest_sha256.clone(),
        files,
    })
}

fn component_plan_from_marker(
    kind: DeliveryComponentKind,
    marker: &StageComponentMarker,
    expected_release_id: &str,
) -> Result<ComponentPlan, DeliveryStagingError> {
    if marker.release_id != expected_release_id
        || marker.artifact_size_bytes == 0
        || !is_lower_hex(&marker.artifact_sha256, 64)
        || !is_lower_hex(&marker.component_manifest_sha256, 64)
        || marker.files.len() > MAX_STAGE_FILES
    {
        return Err(DeliveryStagingError::CorruptStage);
    }
    let mut files = Vec::with_capacity(marker.files.len());
    let mut casefolded = HashSet::with_capacity(marker.files.len());
    let mut decoded_bytes = 0_u64;
    for (source_index, file) in marker.files.iter().enumerate() {
        validate_windows_relative_path(&file.relative_path)
            .map_err(|_| DeliveryStagingError::CorruptStage)?;
        if !is_lower_hex(&file.sha256, 64) {
            return Err(DeliveryStagingError::CorruptStage);
        }
        decoded_bytes = decoded_bytes
            .checked_add(file.size_bytes)
            .ok_or(DeliveryStagingError::CorruptStage)?;
        if decoded_bytes > MAX_STAGE_BYTES || !casefolded.insert(file.relative_path.to_lowercase())
        {
            return Err(DeliveryStagingError::CorruptStage);
        }
        files.push(PlannedFile {
            source_index,
            relative_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
            sha256: file.sha256.clone(),
        });
    }
    if files
        .windows(2)
        .any(|pair| pair[0].relative_path >= pair[1].relative_path)
    {
        return Err(DeliveryStagingError::CorruptStage);
    }
    validate_file_tree(&files).map_err(|_| DeliveryStagingError::CorruptStage)?;
    Ok(ComponentPlan {
        kind,
        artifact_path: PathBuf::new(),
        release_id: marker.release_id.clone(),
        artifact_sha256: marker.artifact_sha256.clone(),
        artifact_size_bytes: marker.artifact_size_bytes,
        component_manifest_sha256: marker.component_manifest_sha256.clone(),
        files,
    })
}

fn verify_artifact(
    artifact_path: &Path,
    expected: &WindowsDeliveryComponent,
) -> Result<(), DeliveryStagingError> {
    if !artifact_path.is_absolute() {
        return Err(DeliveryStagingError::InvalidArtifact);
    }
    let metadata = fs::symlink_metadata(artifact_path).map_err(|_| DeliveryStagingError::Io)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(DeliveryStagingError::InvalidArtifact);
    }
    if metadata.len() != expected.artifact_size_bytes {
        return Err(DeliveryStagingError::ArtifactSizeMismatch);
    }
    let digest = sha256_file(artifact_path)?;
    if digest != expected.artifact_sha256 {
        return Err(DeliveryStagingError::ArtifactDigestMismatch);
    }
    Ok(())
}

fn validate_file_tree(files: &[PlannedFile]) -> Result<(), DeliveryStagingError> {
    let folded_files: HashSet<String> = files
        .iter()
        .map(|file| file.relative_path.to_lowercase())
        .collect();
    for file in files {
        let mut ancestor = String::new();
        let parts: Vec<&str> = file.relative_path.split('/').collect();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            if !ancestor.is_empty() {
                ancestor.push('/');
            }
            ancestor.push_str(part);
            if folded_files.contains(&ancestor.to_lowercase()) {
                return Err(DeliveryStagingError::UnsafeEntry);
            }
        }
    }
    Ok(())
}

fn marker_component(plan: &ComponentPlan) -> StageComponentMarker {
    StageComponentMarker {
        release_id: plan.release_id.clone(),
        artifact_sha256: plan.artifact_sha256.clone(),
        artifact_size_bytes: plan.artifact_size_bytes,
        component_manifest_sha256: plan.component_manifest_sha256.clone(),
        files: plan
            .files
            .iter()
            .map(|file| StageFileMarker {
                relative_path: file.relative_path.clone(),
                size_bytes: file.size_bytes,
                sha256: file.sha256.clone(),
            })
            .collect(),
    }
}

fn materialize_stage<R: DeliveryArchiveReader>(
    pending_path: &Path,
    marker_bytes: &[u8],
    plans: [&ComponentPlan; 2],
    reader: &mut R,
) -> Result<(), DeliveryStagingError> {
    for plan in plans {
        let component_root = pending_path.join(plan.kind.directory());
        fs::create_dir(&component_root).map_err(|_| DeliveryStagingError::Io)?;
        validate_directory_path(&component_root)?;
        for file in &plan.files {
            let target = component_root.join(path_from_relative(&file.relative_path)?);
            let parent = target.parent().ok_or(DeliveryStagingError::UnsafeEntry)?;
            ensure_descendant_directories(&component_root, parent)?;
            let output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| DeliveryStagingError::Io)?;
            let mut hashing = HashingWriter::new(output);
            reader
                .copy_regular_file(
                    plan.kind,
                    &plan.artifact_path,
                    file.source_index,
                    &mut hashing,
                )
                .map_err(|_| DeliveryStagingError::ArchiveDecode)?;
            let (output, bytes_written, digest) = hashing.finish();
            output.sync_all().map_err(|_| DeliveryStagingError::Io)?;
            if bytes_written != file.size_bytes || digest != file.sha256 {
                return Err(DeliveryStagingError::ExtractedFileMismatch);
            }
            let metadata = fs::symlink_metadata(&target).map_err(|_| DeliveryStagingError::Io)?;
            if metadata_is_link_or_reparse(&metadata)
                || !metadata.is_file()
                || metadata.len() != file.size_bytes
            {
                return Err(DeliveryStagingError::UnsafeFilesystem);
            }
        }
    }
    let marker_path = pending_path.join(MARKER_NAME);
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
        .map_err(|_| DeliveryStagingError::Io)?;
    marker
        .write_all(marker_bytes)
        .and_then(|()| marker.sync_all())
        .map_err(|_| DeliveryStagingError::Io)?;
    Ok(())
}

fn verify_materialized_stage(
    stage_path: &Path,
    marker_bytes: &[u8],
    plans: [&ComponentPlan; 2],
) -> Result<(), DeliveryStagingError> {
    validate_directory_path(stage_path)?;
    let marker_path = stage_path.join(MARKER_NAME);
    let marker_metadata =
        fs::symlink_metadata(&marker_path).map_err(|_| DeliveryStagingError::CorruptStage)?;
    if metadata_is_link_or_reparse(&marker_metadata) || !marker_metadata.is_file() {
        return Err(DeliveryStagingError::UnsafeFilesystem);
    }
    let actual_marker = fs::read(&marker_path).map_err(|_| DeliveryStagingError::Io)?;
    if actual_marker != marker_bytes {
        return Err(DeliveryStagingError::CorruptStage);
    }

    let mut expected_files = HashMap::new();
    let mut expected_directories = HashSet::new();
    expected_files.insert(
        MARKER_NAME.to_owned(),
        (marker_bytes.len() as u64, sha256_hex(marker_bytes)),
    );
    for plan in plans {
        let prefix = plan.kind.directory();
        expected_directories.insert(prefix.to_owned());
        for file in &plan.files {
            let path = format!("{prefix}/{}", file.relative_path);
            expected_files.insert(path.clone(), (file.size_bytes, file.sha256.clone()));
            let mut current = prefix.to_owned();
            let parts: Vec<&str> = file.relative_path.split('/').collect();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                current.push('/');
                current.push_str(part);
                expected_directories.insert(current.clone());
            }
        }
    }

    let mut actual_files = HashSet::new();
    let mut actual_directories = HashSet::new();
    collect_tree(
        stage_path,
        stage_path,
        &expected_files,
        &mut actual_files,
        &mut actual_directories,
    )?;
    if actual_files.len() != expected_files.len()
        || actual_directories != expected_directories
        || !expected_files
            .keys()
            .all(|path| actual_files.contains(path))
    {
        return Err(DeliveryStagingError::CorruptStage);
    }
    Ok(())
}

fn collect_tree(
    root: &Path,
    current: &Path,
    expected_files: &HashMap<String, (u64, String)>,
    actual_files: &mut HashSet<String>,
    actual_directories: &mut HashSet<String>,
) -> Result<(), DeliveryStagingError> {
    for entry in fs::read_dir(current).map_err(|_| DeliveryStagingError::Io)? {
        let entry = entry.map_err(|_| DeliveryStagingError::Io)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| DeliveryStagingError::Io)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(DeliveryStagingError::UnsafeFilesystem);
        }
        let relative = normalized_descendant(root, &path)?;
        if metadata.is_dir() {
            actual_directories.insert(relative);
            collect_tree(
                root,
                &path,
                expected_files,
                actual_files,
                actual_directories,
            )?;
        } else if metadata.is_file() {
            let Some((expected_size, expected_digest)) = expected_files.get(&relative) else {
                return Err(DeliveryStagingError::CorruptStage);
            };
            if metadata.len() != *expected_size || sha256_file(&path)? != *expected_digest {
                return Err(DeliveryStagingError::CorruptStage);
            }
            actual_files.insert(relative);
        } else {
            return Err(DeliveryStagingError::UnsafeFilesystem);
        }
    }
    Ok(())
}

fn ensure_descendant_directories(root: &Path, parent: &Path) -> Result<(), DeliveryStagingError> {
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| DeliveryStagingError::UnsafeEntry)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(DeliveryStagingError::UnsafeEntry);
        };
        current.push(value);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| DeliveryStagingError::Io)?;
                let metadata =
                    fs::symlink_metadata(&current).map_err(|_| DeliveryStagingError::Io)?;
                validate_directory_metadata(&metadata)?;
            }
            Err(_) => return Err(DeliveryStagingError::Io),
        }
    }
    Ok(())
}

fn validate_windows_relative_path(path: &str) -> Result<(), DeliveryStagingError> {
    if path.is_empty()
        || path.len() > MAX_RELATIVE_PATH_BYTES
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains("//")
    {
        return Err(DeliveryStagingError::UnsafeEntry);
    }
    for component in path.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(['.', ' '])
            || component.bytes().any(|byte| {
                byte.is_ascii_control()
                    || matches!(byte, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*')
            })
            || windows_reserved_component(component)
        {
            return Err(DeliveryStagingError::UnsafeEntry);
        }
    }
    Ok(())
}

fn windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn path_from_relative(path: &str) -> Result<PathBuf, DeliveryStagingError> {
    validate_windows_relative_path(path)?;
    let mut result = PathBuf::new();
    for component in path.split('/') {
        result.push(component);
    }
    Ok(result)
}

fn normalized_descendant(root: &Path, path: &Path) -> Result<String, DeliveryStagingError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DeliveryStagingError::UnsafeFilesystem)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(DeliveryStagingError::UnsafeFilesystem);
        };
        parts.push(
            value
                .to_str()
                .ok_or(DeliveryStagingError::UnsafeFilesystem)?,
        );
    }
    if parts.is_empty() {
        return Err(DeliveryStagingError::UnsafeFilesystem);
    }
    Ok(parts.join("/"))
}

fn validate_directory_path(path: &Path) -> Result<(), DeliveryStagingError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryStagingError::Io)?;
    validate_directory_metadata(&metadata)
}

fn validate_directory_metadata(metadata: &fs::Metadata) -> Result<(), DeliveryStagingError> {
    if metadata_is_link_or_reparse(metadata) || !metadata.is_dir() {
        return Err(DeliveryStagingError::UnsafeFilesystem);
    }
    Ok(())
}

fn safe_directory_exists(path: &Path) -> Result<bool, DeliveryStagingError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_directory_metadata(&metadata)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(DeliveryStagingError::Io),
    }
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = FILE_ATTRIBUTE_REPARSE_POINT;
        false
    }
}

fn remove_owned_tree_if_safe(path: &Path) -> Result<(), DeliveryStagingError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DeliveryStagingError::Io)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(DeliveryStagingError::UnsafeFilesystem);
    }
    remove_safe_directory_contents(path)?;
    fs::remove_dir(path).map_err(|_| DeliveryStagingError::Io)
}

fn remove_safe_directory_contents(path: &Path) -> Result<(), DeliveryStagingError> {
    for entry in fs::read_dir(path).map_err(|_| DeliveryStagingError::Io)? {
        let entry = entry.map_err(|_| DeliveryStagingError::Io)?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child).map_err(|_| DeliveryStagingError::Io)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(DeliveryStagingError::UnsafeFilesystem);
        }
        if metadata.is_dir() {
            remove_safe_directory_contents(&child)?;
            fs::remove_dir(&child).map_err(|_| DeliveryStagingError::Io)?;
        } else if metadata.is_file() {
            fs::remove_file(&child).map_err(|_| DeliveryStagingError::Io)?;
        } else {
            return Err(DeliveryStagingError::UnsafeFilesystem);
        }
    }
    Ok(())
}

fn release_directory_name(candidate: &VerifiedDeliveryCandidate) -> String {
    release_directory_name_from_parts(candidate.manifest().sequence, candidate.manifest_sha256())
}

fn release_directory_name_for_identity(
    identity: &DeliveryIdentity,
) -> Result<String, DeliveryStagingError> {
    if identity.sequence == 0 || !is_lower_hex(&identity.manifest_sha256, 64) {
        return Err(DeliveryStagingError::CorruptStage);
    }
    Ok(release_directory_name_from_parts(
        identity.sequence,
        &identity.manifest_sha256,
    ))
}

fn release_directory_name_from_parts(sequence: u64, manifest_sha256: &str) -> String {
    format!("release-{sequence:020}-{manifest_sha256}")
}

fn staged_delivery(candidate: &VerifiedDeliveryCandidate, path: PathBuf) -> StagedDelivery {
    StagedDelivery {
        identity: candidate.identity(),
        path,
    }
}

fn sha256_file(path: &Path) -> Result<String, DeliveryStagingError> {
    let mut file = File::open(path).map_err(|_| DeliveryStagingError::Io)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| DeliveryStagingError::Io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(encode_digest(digest.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    encode_digest(Sha256::digest(bytes))
}

fn encode_digest(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

struct HashingWriter<W> {
    writer: W,
    digest: Sha256,
    bytes_written: u64,
}

impl<W> HashingWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            digest: Sha256::new(),
            bytes_written: 0,
        }
    }

    fn finish(self) -> (W, u64, String) {
        (
            self.writer,
            self.bytes_written,
            encode_digest(self.digest.finalize()),
        )
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.writer.write(buffer)?;
        self.digest.update(&buffer[..written]);
        self.bytes_written = self
            .bytes_written
            .checked_add(u64::try_from(written).map_err(|_| io::ErrorKind::InvalidData)?)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryStagingError {
    InvalidRoot,
    InvalidArtifact,
    ArtifactSizeMismatch,
    ArtifactDigestMismatch,
    ArchiveDecode,
    UnsafeEntry,
    UnsupportedArchiveEntry,
    InvalidEntryIdentity,
    DuplicateEntry,
    StageLimitExceeded,
    ExtractedFileMismatch,
    UnsafeFilesystem,
    AmbiguousStage,
    CorruptStage,
    Serialization,
    Io,
}

impl fmt::Display for DeliveryStagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoot => "Windows delivery staging root is invalid",
            Self::InvalidArtifact => "Windows delivery artifact source is invalid",
            Self::ArtifactSizeMismatch => "Windows delivery artifact size does not match candidate",
            Self::ArtifactDigestMismatch => {
                "Windows delivery artifact digest does not match candidate"
            }
            Self::ArchiveDecode => "Windows delivery component archive cannot be decoded",
            Self::UnsafeEntry => "Windows delivery archive contains an unsafe path",
            Self::UnsupportedArchiveEntry => {
                "Windows delivery archive contains a link or special entry"
            }
            Self::InvalidEntryIdentity => "Windows delivery file identity is invalid",
            Self::DuplicateEntry => "Windows delivery archive contains an ambiguous duplicate path",
            Self::StageLimitExceeded => "Windows delivery extracted file set exceeds policy limits",
            Self::ExtractedFileMismatch => {
                "Windows delivery extracted file does not match component manifest identity"
            }
            Self::UnsafeFilesystem => {
                "Windows delivery staging filesystem contains a link or reparse ambiguity"
            }
            Self::AmbiguousStage => "Windows delivery has ambiguous staged release state",
            Self::CorruptStage => "Windows delivery staged release is corrupt or incomplete",
            Self::Serialization => "Windows delivery staging marker serialization failed",
            Self::Io => "Windows delivery staging filesystem operation failed",
        })
    }
}

impl std::error::Error for DeliveryStagingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_delivery::{
        DetachedSignatureVerifier, TrustedSigner, TrustedSignerSet, TrustedSignerStatus,
        WindowsDeliveryCompatibility, WindowsDeliveryComponents, WindowsDeliveryEvidence,
        WindowsDeliveryManifest, verify_delivery_candidate,
    };
    use bridge_domain::CAMOUHOST_IPC_VERSION;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
    const CERTIFICATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-stage-{label}-{}-{counter}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Default)]
    struct FakeArchiveReader {
        entries: HashMap<DeliveryComponentKind, Vec<(DeliveryArchiveEntry, Vec<u8>)>>,
    }

    impl FakeArchiveReader {
        fn insert_file(&mut self, component: DeliveryComponentKind, path: &str, content: &[u8]) {
            self.entries.entry(component).or_default().push((
                DeliveryArchiveEntry::regular_file(
                    path,
                    u64::try_from(content.len()).unwrap_or(u64::MAX),
                    sha256_hex(content),
                ),
                content.to_vec(),
            ));
        }

        fn insert_special(&mut self, component: DeliveryComponentKind, path: &str) {
            self.entries
                .entry(component)
                .or_default()
                .push((DeliveryArchiveEntry::link_or_special(path), Vec::new()));
        }
    }

    impl DeliveryArchiveReader for FakeArchiveReader {
        type Error = ();

        fn entries(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
        ) -> Result<Vec<DeliveryArchiveEntry>, Self::Error> {
            Ok(self
                .entries
                .get(&component)
                .map(|entries| entries.iter().map(|(entry, _)| entry.clone()).collect())
                .unwrap_or_default())
        }

        fn copy_regular_file(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
            entry_index: usize,
            writer: &mut dyn Write,
        ) -> Result<(), Self::Error> {
            let content = self
                .entries
                .get(&component)
                .and_then(|entries| entries.get(entry_index))
                .map(|(_, content)| content.as_slice())
                .ok_or(())?;
            writer.write_all(content).map_err(|_| ())
        }
    }

    struct DigestVerifier;

    impl DetachedSignatureVerifier for DigestVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            manifest_bytes: &[u8],
            cms_der: &[u8],
            expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(cms_der == Sha256::digest(manifest_bytes).as_slice()
                && expected_certificate_sha256 == CERTIFICATE)
        }
    }

    fn artifact(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
        fs::write(path, bytes)
    }

    fn component(bytes: &[u8], release_prefix: &str) -> WindowsDeliveryComponent {
        WindowsDeliveryComponent {
            release_id: format!("{release_prefix}{}", "b".repeat(64)),
            artifact_sha256: sha256_hex(bytes),
            artifact_size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            component_manifest_sha256: "c".repeat(64),
        }
    }

    fn candidate(
        bridge_bytes: &[u8],
        runtime_bytes: &[u8],
        sequence: u64,
    ) -> Result<VerifiedDeliveryCandidate, Box<dyn std::error::Error>> {
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("release-set-v3-sha256-{}", "d".repeat(64)),
            sequence,
            source_commit_sha: "1".repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: component(bridge_bytes, "profile-bridge-v2-sha256-"),
                runtime_bundle: component(runtime_bytes, "runtime-bundle-v2-sha256-"),
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: "e".repeat(64),
                provenance_sha256: "f".repeat(64),
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: 1,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: "2.0.0".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let cms = Sha256::digest(&manifest_bytes);
        let signature_bytes = format!(
            "{{\"schema_version\":1,\"kind\":\"WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS\",\"key_id\":\"test\",\"cms_der_hex\":\"{}\"}}",
            encode_digest(cms)
        )
        .into_bytes();
        let signer = TrustedSigner::new("test", CERTIFICATE, TrustedSignerStatus::Active)?;
        let trust = TrustedSignerSet::new([signer])?;
        Ok(verify_delivery_candidate(
            &manifest_bytes,
            &signature_bytes,
            &trust,
            None,
            &mut DigestVerifier,
        )?)
    }

    #[test]
    fn exact_candidate_stages_side_by_side_and_replay_is_idempotent() -> TestResult {
        let directory = TestDirectory::create("exact")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let candidate = candidate(bridge_bytes, runtime_bytes, 7)?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let mut reader = FakeArchiveReader::default();
        reader.insert_file(
            DeliveryComponentKind::ProfileBridge,
            "profile-bridge.exe",
            b"bridge",
        );
        reader.insert_file(
            DeliveryComponentKind::RuntimeBundle,
            "python/python.exe",
            b"python",
        );
        reader.insert_file(
            DeliveryComponentKind::RuntimeBundle,
            "runtime/real.py",
            b"runtime",
        );

        let first = stage_verified_delivery(
            &root,
            &candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut reader,
        )?;
        let replay = stage_verified_delivery(
            &root,
            &candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut reader,
        )?;
        assert_eq!(first, replay);
        let reopened = reopen_staged_delivery(&root, &candidate.identity())?;
        assert_eq!(reopened, first);
        assert_eq!(
            fs::read(first.profile_bridge_root().join("profile-bridge.exe"))?,
            b"bridge"
        );
        assert_eq!(
            fs::read(first.runtime_root().join("python/python.exe"))?,
            b"python"
        );
        fs::write(first.runtime_root().join("python/python.exe"), b"tampered")?;
        assert_eq!(
            reopen_staged_delivery(&root, &candidate.identity()),
            Err(DeliveryStagingError::CorruptStage)
        );
        Ok(())
    }

    #[test]
    fn reopen_rejects_component_identity_substitution() -> TestResult {
        let directory = TestDirectory::create("reopen-identity")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let candidate = candidate(bridge_bytes, runtime_bytes, 13)?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let mut reader = FakeArchiveReader::default();
        reader.insert_file(
            DeliveryComponentKind::ProfileBridge,
            "profile-bridge.exe",
            b"bridge",
        );
        reader.insert_file(
            DeliveryComponentKind::RuntimeBundle,
            "python/python.exe",
            b"python",
        );
        let staged = stage_verified_delivery(
            &root,
            &candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut reader,
        )?;
        let marker_path = staged.path().join(MARKER_NAME);
        let mut marker: StageMarker = serde_json::from_slice(&fs::read(&marker_path)?)?;
        marker.runtime_bundle.release_id = format!("runtime-bundle-v2-sha256-{}", "9".repeat(64));
        fs::write(&marker_path, serde_json::to_vec(&marker)?)?;
        assert_eq!(
            reopen_staged_delivery(&root, &candidate.identity()),
            Err(DeliveryStagingError::CorruptStage)
        );
        Ok(())
    }

    #[test]
    fn modified_artifact_is_rejected_before_extraction() -> TestResult {
        let directory = TestDirectory::create("artifact")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let candidate = candidate(bridge_bytes, runtime_bytes, 8)?;
        fs::write(&runtime_artifact, b"tampered-runtime")?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let mut reader = FakeArchiveReader::default();
        assert_eq!(
            stage_verified_delivery(
                &root,
                &candidate,
                &bridge_artifact,
                &runtime_artifact,
                &mut reader,
            ),
            Err(DeliveryStagingError::ArtifactSizeMismatch)
        );
        Ok(())
    }

    #[test]
    fn traversal_reserved_names_and_special_entries_fail_closed() -> TestResult {
        let directory = TestDirectory::create("unsafe")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let candidate = candidate(bridge_bytes, runtime_bytes, 9)?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;

        for path in [
            "../escape",
            "C:/absolute",
            "dir\\escape",
            "NUL.txt",
            "file:ads",
        ] {
            let mut reader = FakeArchiveReader::default();
            reader.insert_file(DeliveryComponentKind::ProfileBridge, path, b"bad");
            assert_eq!(
                stage_verified_delivery(
                    &root,
                    &candidate,
                    &bridge_artifact,
                    &runtime_artifact,
                    &mut reader,
                ),
                Err(DeliveryStagingError::UnsafeEntry)
            );
        }

        let mut special = FakeArchiveReader::default();
        special.insert_special(DeliveryComponentKind::RuntimeBundle, "runtime/link");
        assert_eq!(
            stage_verified_delivery(
                &root,
                &candidate,
                &bridge_artifact,
                &runtime_artifact,
                &mut special,
            ),
            Err(DeliveryStagingError::UnsupportedArchiveEntry)
        );
        Ok(())
    }

    #[test]
    fn complete_interrupted_stage_is_finalized_on_retry() -> TestResult {
        let directory = TestDirectory::create("recover")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let candidate = candidate(bridge_bytes, runtime_bytes, 10)?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let mut reader = FakeArchiveReader::default();
        reader.insert_file(
            DeliveryComponentKind::ProfileBridge,
            "profile-bridge.exe",
            b"bridge",
        );
        reader.insert_file(
            DeliveryComponentKind::RuntimeBundle,
            "runtime/real.py",
            b"runtime",
        );
        let staged = stage_verified_delivery(
            &root,
            &candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut reader,
        )?;
        let name = staged
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(DeliveryStagingError::CorruptStage)?;
        let pending = root.path().join(format!(".pending-{name}"));
        fs::rename(staged.path(), &pending)?;

        assert_eq!(
            reopen_staged_delivery(&root, &candidate.identity()),
            Err(DeliveryStagingError::CorruptStage)
        );
        let recovered = stage_verified_delivery(
            &root,
            &candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut reader,
        )?;
        assert!(recovered.path().is_dir());
        assert!(!pending.exists());
        Ok(())
    }

    #[test]
    fn failed_new_candidate_does_not_mutate_previous_staged_release() -> TestResult {
        let directory = TestDirectory::create("side-by-side")?;
        let bridge_artifact = directory.0.join("bridge.zip");
        let runtime_artifact = directory.0.join("runtime.zip");
        let bridge_bytes = b"bridge-archive";
        let runtime_bytes = b"runtime-archive";
        artifact(&bridge_artifact, bridge_bytes)?;
        artifact(&runtime_artifact, runtime_bytes)?;
        let first_candidate = candidate(bridge_bytes, runtime_bytes, 11)?;
        let second_candidate = candidate(bridge_bytes, runtime_bytes, 12)?;
        let root = DeliveryStagingRoot::open_or_create(directory.0.join("releases"))?;
        let mut good = FakeArchiveReader::default();
        good.insert_file(
            DeliveryComponentKind::ProfileBridge,
            "profile-bridge.exe",
            b"bridge",
        );
        good.insert_file(
            DeliveryComponentKind::RuntimeBundle,
            "runtime/real.py",
            b"runtime",
        );
        let first = stage_verified_delivery(
            &root,
            &first_candidate,
            &bridge_artifact,
            &runtime_artifact,
            &mut good,
        )?;
        let first_marker = fs::read(first.path().join(MARKER_NAME))?;

        let mut bad = FakeArchiveReader::default();
        bad.insert_file(DeliveryComponentKind::RuntimeBundle, "../escape", b"bad");
        assert_eq!(
            stage_verified_delivery(
                &root,
                &second_candidate,
                &bridge_artifact,
                &runtime_artifact,
                &mut bad,
            ),
            Err(DeliveryStagingError::UnsafeEntry)
        );
        assert_eq!(fs::read(first.path().join(MARKER_NAME))?, first_marker);
        assert!(first.path().is_dir());
        Ok(())
    }
}
