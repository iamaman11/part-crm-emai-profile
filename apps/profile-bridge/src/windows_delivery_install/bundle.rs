use super::NativeInstallerError;
use crate::installed_connection::ControlPlaneOrigin;
use crate::windows_delivery::{TrustedSigner, TrustedSignerSet, TrustedSignerStatus};
use serde::Deserialize;
use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::Path,
};

const MAX_BOOTSTRAP_BYTES: u64 = 4_096;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 2 * 1024 * 1024 + 4_096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapDocument {
    schema_version: u32,
    kind: String,
    control_plane_origin: String,
    signer_key_id: String,
    signer_certificate_sha256: String,
}

pub(super) struct InstallBundle {
    pub origin: ControlPlaneOrigin,
    pub trust: TrustedSignerSet,
    pub manifest: Vec<u8>,
    pub signature: Vec<u8>,
}

impl InstallBundle {
    pub fn open(directory: &Path) -> Result<Self, NativeInstallerError> {
        validate_directory_tree(directory, false)?;
        let document: BootstrapDocument = serde_json::from_slice(&read_bounded(
            &directory.join("installation.json"),
            MAX_BOOTSTRAP_BYTES,
        )?)
        .map_err(|_| NativeInstallerError::InvalidBundle)?;
        if document.schema_version != 1 || document.kind != "WINDOWS_PROFILE_BRIDGE_INSTALLATION" {
            return Err(NativeInstallerError::InvalidBundle);
        }
        let origin = ControlPlaneOrigin::parse(document.control_plane_origin)
            .map_err(|_| NativeInstallerError::InvalidBundle)?;
        let signer = TrustedSigner::new(
            document.signer_key_id,
            document.signer_certificate_sha256,
            TrustedSignerStatus::Active,
        )
        .map_err(|_| NativeInstallerError::InvalidBundle)?;
        let trust =
            TrustedSignerSet::new([signer]).map_err(|_| NativeInstallerError::InvalidBundle)?;
        Ok(Self {
            origin,
            trust,
            manifest: read_bounded(
                &directory.join("windows-delivery-manifest.json"),
                MAX_MANIFEST_BYTES,
            )?,
            signature: read_bounded(
                &directory.join("windows-delivery-signature.json"),
                MAX_SIGNATURE_BYTES,
            )?,
        })
    }
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, NativeInstallerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| NativeInstallerError::InputIo)?;
    if !metadata.is_file() || linked(&metadata) || metadata.len() == 0 || metadata.len() > limit {
        return Err(NativeInstallerError::InvalidBundle);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|_| NativeInstallerError::InputIo)?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(NativeInstallerError::InvalidBundle);
    }
    Ok(bytes)
}

pub(super) fn validate_directory_tree(
    path: &Path,
    allow_missing: bool,
) -> Result<(), NativeInstallerError> {
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(NativeInstallerError::InvalidBundle);
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() && !linked(&metadata) => {}
            Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(NativeInstallerError::InvalidBundle),
        }
    }
    Ok(())
}

fn linked(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
