//! Public installation routing, shared by pairing and launch. This is not identity or health evidence.

use crate::windows_delivery_handoff::{DeliveryHandoffCoordinator, DeliveryHandoffError};
use bridge_domain::BridgePortError;
use serde::{Deserialize, Serialize};
#[cfg(any(windows, test))]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::Read;
#[cfg(any(windows, test))]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(any(windows, test))]
use std::sync::atomic::{AtomicU64, Ordering};

const SETTINGS_FILE: &str = "connection.json";
const BINDING_FILE: &str = "device-application-binding.json";
const MAX_SETTINGS_BYTES: u64 = 4_096;
#[cfg(any(windows, test))]
static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlPlaneOrigin(String);

impl ControlPlaneOrigin {
    pub fn parse(value: impl Into<String>) -> Result<Self, BridgePortError> {
        let value = value.into();
        let host = value
            .strip_prefix("https://")
            .ok_or(BridgePortError::InvalidResponse)?;
        if host.is_empty()
            || host.len() > 253
            || host.contains(['/', '?', '#', '@', '\\'])
            || host.chars().any(char::is_whitespace)
            || !host.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
            })
        {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConnectionDocument {
    schema_version: u32,
    control_plane_origin: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledConnection {
    root: PathBuf,
    origin: ControlPlaneOrigin,
}

impl InstalledConnection {
    pub fn open_current() -> Result<Self, InstalledConnectionError> {
        let executable = std::env::current_exe().map_err(|_| InstalledConnectionError::Storage)?;
        let coordinator = DeliveryHandoffCoordinator::from_current_executable(executable)
            .map_err(InstalledConnectionError::Layout)?;
        Self::open(
            coordinator
                .installed_root()
                .map_err(InstalledConnectionError::Layout)?,
        )
    }

    #[cfg(windows)]
    pub(crate) fn prepare(
        coordinator: &DeliveryHandoffCoordinator,
        origin: ControlPlaneOrigin,
    ) -> Result<Self, InstalledConnectionError> {
        let root = coordinator
            .installed_root()
            .map_err(InstalledConnectionError::Layout)?;
        Self::prepare_at(root, origin)
    }

    #[cfg(any(windows, test))]
    fn prepare_at(
        root: &Path,
        origin: ControlPlaneOrigin,
    ) -> Result<Self, InstalledConnectionError> {
        validate_root(root)?;
        match fs::symlink_metadata(root.join(SETTINGS_FILE)) {
            Ok(_) => {
                let installed = Self::open(root)?;
                if installed.origin != origin {
                    return Err(InstalledConnectionError::Conflict);
                }
                Ok(installed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // An existing device identity must never be adopted under an unknown origin.
                match fs::symlink_metadata(root.join(BINDING_FILE)) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    _ => return Err(InstalledConnectionError::Conflict),
                }
                Ok(Self {
                    root: root.to_path_buf(),
                    origin,
                })
            }
            Err(_) => Err(InstalledConnectionError::Storage),
        }
    }

    pub(crate) fn open(root: &Path) -> Result<Self, InstalledConnectionError> {
        validate_root(root)?;
        let path = root.join(SETTINGS_FILE);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| InstalledConnectionError::Storage)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_SETTINGS_BYTES
        {
            return Err(InstalledConnectionError::InvalidRecord);
        }
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|_| InstalledConnectionError::Storage)?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err(InstalledConnectionError::InvalidRecord);
        }
        let document: ConnectionDocument =
            serde_json::from_slice(&bytes).map_err(|_| InstalledConnectionError::InvalidRecord)?;
        if document.schema_version != 1 {
            return Err(InstalledConnectionError::InvalidRecord);
        }
        let origin = ControlPlaneOrigin::parse(document.control_plane_origin)
            .map_err(|_| InstalledConnectionError::InvalidRecord)?;
        Ok(Self {
            root: root.to_path_buf(),
            origin,
        })
    }

    /// Called only after native first activation succeeds, while its activation guard is held.
    #[cfg(any(windows, test))]
    pub(crate) fn persist(&self) -> Result<(), InstalledConnectionError> {
        Self::prepare_at(&self.root, self.origin.clone())?;
        if self
            .root
            .join(SETTINGS_FILE)
            .try_exists()
            .map_err(|_| InstalledConnectionError::Storage)?
        {
            return if Self::open(&self.root)?.origin == self.origin {
                Ok(())
            } else {
                Err(InstalledConnectionError::Conflict)
            };
        }
        let bytes = serde_json::to_vec(&ConnectionDocument {
            schema_version: 1,
            control_plane_origin: self.origin.as_str().to_owned(),
        })
        .map_err(|_| InstalledConnectionError::InvalidRecord)?;
        let temporary = self.root.join(format!(
            ".connection-{}-{}.tmp",
            std::process::id(),
            WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| InstalledConnectionError::Storage)?;
        let result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        // Publish complete bytes without replacing an existing record, even if another caller races.
        let result = result.and_then(|()| fs::hard_link(&temporary, self.root.join(SETTINGS_FILE)));
        let cleanup = fs::remove_file(temporary);
        result
            .and(cleanup)
            .map_err(|_| InstalledConnectionError::Storage)
    }

    #[must_use]
    pub fn origin(&self) -> &ControlPlaneOrigin {
        &self.origin
    }

    #[must_use]
    pub fn binding_path(&self) -> PathBuf {
        self.root.join(BINDING_FILE)
    }
}

fn validate_root(root: &Path) -> Result<(), InstalledConnectionError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| InstalledConnectionError::Storage)?;
    if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(InstalledConnectionError::InvalidRecord);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledConnectionError {
    InvalidRecord,
    Conflict,
    Storage,
    Layout(DeliveryHandoffError),
}

impl std::fmt::Display for InstalledConnectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("installed Profile Bridge connection is unavailable or conflicts with setup")
    }
}

impl std::error::Error for InstalledConnectionError {}

#[cfg(test)]
mod tests;
