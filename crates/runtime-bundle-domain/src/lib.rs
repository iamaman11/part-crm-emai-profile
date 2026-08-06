#![forbid(unsafe_code)]

use core::fmt;
use std::collections::BTreeMap;

pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const RUNTIME_IPC_VERSION: u16 = 1;
const MAX_PATH_LENGTH: usize = 240;
const MAX_SEGMENT_LENGTH: usize = 80;
const SHA256_HEX_LENGTH: usize = 64;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BundleRelativePath(String);

impl BundleRelativePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, BundlePathError> {
        let value = value.into();
        validate_path(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn case_folded(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl fmt::Display for BundleRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundlePathError {
    Empty,
    TooLong,
    AbsoluteOrDrivePath,
    InvalidSeparator,
    InvalidSegment,
    ReservedSegment,
}

impl fmt::Display for BundlePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "bundle path is empty",
            Self::TooLong => "bundle path exceeds the bounded length",
            Self::AbsoluteOrDrivePath => "bundle path must be relative and drive-free",
            Self::InvalidSeparator => "bundle path must use canonical forward slashes",
            Self::InvalidSegment => "bundle path contains an invalid segment",
            Self::ReservedSegment => "bundle path contains a Windows-reserved segment",
        })
    }
}

impl std::error::Error for BundlePathError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, DigestError> {
        let value = value.into();
        if value.len() != SHA256_HEX_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(DigestError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigestError;

impl fmt::Display for DigestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SHA-256 digest must be 64 lowercase hexadecimal characters")
    }
}

impl std::error::Error for DigestError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePlatform {
    WindowsX86_64,
}

impl RuntimePlatform {
    #[must_use]
    pub const fn manifest_value(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => "windows-x86_64",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeManifest {
    schema_version: u16,
    runtime_version: String,
    python_version: String,
    ipc_version: u16,
    platform: RuntimePlatform,
    entrypoint: BundleRelativePath,
    inventory_sha256: Sha256Digest,
}

impl RuntimeManifest {
    pub fn new(
        runtime_version: impl Into<String>,
        python_version: impl Into<String>,
        platform: RuntimePlatform,
        entrypoint: BundleRelativePath,
        inventory_sha256: Sha256Digest,
    ) -> Result<Self, RuntimeManifestError> {
        let runtime_version = runtime_version.into();
        let python_version = python_version.into();
        if !valid_version(&runtime_version) {
            return Err(RuntimeManifestError::InvalidRuntimeVersion);
        }
        if !valid_python_version(&python_version) {
            return Err(RuntimeManifestError::InvalidPythonVersion);
        }
        if !entrypoint.as_str().ends_with(".py") {
            return Err(RuntimeManifestError::InvalidEntrypoint);
        }
        Ok(Self {
            schema_version: RUNTIME_MANIFEST_SCHEMA_VERSION,
            runtime_version,
            python_version,
            ipc_version: RUNTIME_IPC_VERSION,
            platform,
            entrypoint,
            inventory_sha256,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    #[must_use]
    pub fn python_version(&self) -> &str {
        &self.python_version
    }

    #[must_use]
    pub const fn ipc_version(&self) -> u16 {
        self.ipc_version
    }

    #[must_use]
    pub const fn platform(&self) -> RuntimePlatform {
        self.platform
    }

    #[must_use]
    pub const fn entrypoint(&self) -> &BundleRelativePath {
        &self.entrypoint
    }

    #[must_use]
    pub const fn inventory_sha256(&self) -> &Sha256Digest {
        &self.inventory_sha256
    }

    pub fn validate_inventory_digest(
        &self,
        calculated: &Sha256Digest,
    ) -> Result<(), RuntimeManifestError> {
        if &self.inventory_sha256 != calculated {
            return Err(RuntimeManifestError::InventoryDigestMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeManifestError {
    InvalidRuntimeVersion,
    InvalidPythonVersion,
    InvalidEntrypoint,
    InventoryDigestMismatch,
}

impl fmt::Display for RuntimeManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRuntimeVersion => "runtime version is invalid",
            Self::InvalidPythonVersion => "Python version is invalid",
            Self::InvalidEntrypoint => "runtime entrypoint must be a safe Python file",
            Self::InventoryDigestMismatch => "runtime inventory digest does not match",
        })
    }
}

impl std::error::Error for RuntimeManifestError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    path: BundleRelativePath,
    length: u64,
    sha256: Sha256Digest,
}

impl InventoryEntry {
    #[must_use]
    pub const fn new(path: BundleRelativePath, length: u64, sha256: Sha256Digest) -> Self {
        Self {
            path,
            length,
            sha256,
        }
    }

    #[must_use]
    pub const fn path(&self) -> &BundleRelativePath {
        &self.path
    }

    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }

    #[must_use]
    pub const fn sha256(&self) -> &Sha256Digest {
        &self.sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeInventory {
    entries: Vec<InventoryEntry>,
}

impl RuntimeInventory {
    pub fn new(entries: impl IntoIterator<Item = InventoryEntry>) -> Result<Self, InventoryError> {
        let mut by_case_folded = BTreeMap::new();
        for entry in entries {
            let key = entry.path.case_folded();
            if by_case_folded.insert(key, entry).is_some() {
                return Err(InventoryError::DuplicateOrCaseCollision);
            }
        }
        if by_case_folded.is_empty() {
            return Err(InventoryError::Empty);
        }
        Ok(Self {
            entries: by_case_folded.into_values().collect(),
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[InventoryEntry] {
        &self.entries
    }

    #[must_use]
    pub fn contains(&self, path: &BundleRelativePath) -> bool {
        let folded = path.case_folded();
        self.entries
            .binary_search_by(|entry| entry.path.case_folded().cmp(&folded))
            .is_ok()
    }

    pub fn validate_entrypoint(&self, manifest: &RuntimeManifest) -> Result<(), InventoryError> {
        if !self.contains(manifest.entrypoint()) {
            return Err(InventoryError::EntrypointMissing);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryError {
    Empty,
    DuplicateOrCaseCollision,
    EntrypointMissing,
}

impl fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "runtime inventory is empty",
            Self::DuplicateOrCaseCollision => {
                "runtime inventory contains duplicate or case-colliding paths"
            }
            Self::EntrypointMissing => "runtime entrypoint is absent from inventory",
        })
    }
}

impl std::error::Error for InventoryError {}

fn validate_path(value: &str) -> Result<(), BundlePathError> {
    if value.is_empty() {
        return Err(BundlePathError::Empty);
    }
    if value.len() > MAX_PATH_LENGTH {
        return Err(BundlePathError::TooLong);
    }
    if value.starts_with('/') || value.contains(':') {
        return Err(BundlePathError::AbsoluteOrDrivePath);
    }
    if value.contains('\\') || value.contains("//") || value.ends_with('/') {
        return Err(BundlePathError::InvalidSeparator);
    }
    for segment in value.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.len() > MAX_SEGMENT_LENGTH
            || segment.ends_with(['.', ' '])
            || !segment.bytes().all(valid_path_byte)
        {
            return Err(BundlePathError::InvalidSegment);
        }
        let stem = segment
            .split_once('.')
            .map_or(segment, |(candidate, _)| candidate);
        if windows_reserved(stem) {
            return Err(BundlePathError::ReservedSegment);
        }
    }
    Ok(())
}

fn valid_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

fn windows_reserved(value: &str) -> bool {
    let folded = value.to_ascii_uppercase();
    matches!(folded.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || numbered_reserved(&folded, "COM")
        || numbered_reserved(&folded, "LPT")
}

fn numbered_reserved(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

fn valid_version(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(parts.next(), Some(part) if numeric_component(part))
        && matches!(parts.next(), Some(part) if numeric_component(part))
        && matches!(parts.next(), Some(part) if numeric_component(part))
        && parts.next().is_none()
}

fn valid_python_version(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(parts.next(), Some(part) if numeric_component(part))
        && matches!(parts.next(), Some(part) if numeric_component(part))
        && parts.next().is_none()
}

fn numeric_component(value: &str) -> bool {
    !value.is_empty() && value.len() <= 5 && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{
        BundlePathError, BundleRelativePath, DigestError, InventoryEntry, InventoryError,
        RuntimeInventory, RuntimeManifest, RuntimeManifestError, RuntimePlatform, Sha256Digest,
    };

    fn digest(character: char) -> Result<Sha256Digest, DigestError> {
        Sha256Digest::parse(character.to_string().repeat(64))
    }

    #[test]
    fn safe_relative_paths_are_canonical() -> Result<(), Box<dyn std::error::Error>> {
        let path = BundleRelativePath::parse("camouhost/main.py")?;
        assert_eq!(path.as_str(), "camouhost/main.py");
        assert_eq!(path.case_folded(), "camouhost/main.py");
        Ok(())
    }

    #[test]
    fn unsafe_and_windows_reserved_paths_fail_closed() {
        let invalid = [
            "",
            "/absolute.py",
            "C:/runtime.py",
            "../runtime.py",
            "camouhost/../runtime.py",
            "camouhost\\main.py",
            "camouhost//main.py",
            "camouhost/CON.py",
            "camouhost/com1.txt",
            "camouhost/trailing.",
            "camouhost/trailing ",
        ];
        for value in invalid {
            assert!(
                BundleRelativePath::parse(value).is_err(),
                "unexpected valid path: {value}"
            );
        }
    }

    #[test]
    fn digest_requires_lowercase_sha256_shape() {
        assert_eq!(Sha256Digest::parse("A".repeat(64)), Err(DigestError));
        assert_eq!(Sha256Digest::parse("a".repeat(63)), Err(DigestError));
    }

    #[test]
    fn case_colliding_inventory_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let result = RuntimeInventory::new([
            InventoryEntry::new(
                BundleRelativePath::parse("camouhost/main.py")?,
                10,
                digest('a')?,
            ),
            InventoryEntry::new(
                BundleRelativePath::parse("Camouhost/Main.py")?,
                10,
                digest('b')?,
            ),
        ]);
        assert_eq!(result, Err(InventoryError::DuplicateOrCaseCollision));
        Ok(())
    }

    #[test]
    fn manifest_requires_entrypoint_and_matching_inventory_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let inventory_digest = digest('c')?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/main.py")?,
            inventory_digest.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(
            BundleRelativePath::parse("camouhost/main.py")?,
            10,
            digest('a')?,
        )])?;
        inventory.validate_entrypoint(&manifest)?;
        manifest.validate_inventory_digest(&inventory_digest)?;
        assert_eq!(
            manifest.validate_inventory_digest(&digest('d')?),
            Err(RuntimeManifestError::InventoryDigestMismatch)
        );
        Ok(())
    }

    #[test]
    fn manifest_rejects_non_python_entrypoint() -> Result<(), BundlePathError> {
        let result = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/runtime.exe")?,
            digest('a').map_err(|_| BundlePathError::InvalidSegment)?,
        );
        assert_eq!(result, Err(RuntimeManifestError::InvalidEntrypoint));
        Ok(())
    }
}
