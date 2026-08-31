use browser_execution_domain::{
    BrowserExecutionError, BrowserIdentityManifest, BrowserOsIdentity, DisplayIdentity, FontIdentity,
    GraphicsIdentity, HardwareCapabilityIdentity, LocaleIdentity, OriginDeterminismMode,
    OriginDeterministicIdentity, ProfileStableIdentity,
};
use std::path::Path;

pub fn browser_identity_fixture(
    runtime_version: &str,
    runtime_inventory_sha256: impl Into<String>,
    fingerprint_source: &str,
    fingerprint_config_sha256: impl Into<String>,
) -> Result<BrowserIdentityManifest, BrowserExecutionError> {
    BrowserIdentityManifest::new(
        2,
        "profile-stability-v1",
        runtime_version,
        runtime_inventory_sha256,
        fingerprint_source,
        fingerprint_config_sha256,
        ProfileStableIdentity::new(
            1,
            BrowserOsIdentity::new(
                "Mozilla/5.0 Firefox/152.0",
                152,
                "Win32",
                "Windows NT 10.0; Win64; x64",
            )?,
            HardwareCapabilityIdentity::new(8, 8, 0)?,
            DisplayIdentity::new(1920, 1080, 1920, 1040, 0, 0, 24, 24, 1000)?,
            GraphicsIdentity::new(
                "Google Inc. (NVIDIA)",
                "ANGLE (NVIDIA GeForce)",
                digest('1'),
                digest('2'),
                digest('3'),
                digest('4'),
                digest('5'),
            )?,
            FontIdentity::new(digest('6'), digest('7'))?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                digest('8'),
                digest('9'),
            )?,
            LocaleIdentity::new("en-US", digest('a'), Some(digest('b')))?,
        )?,
    )
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

pub fn remove_test_root(path: &Path) -> Result<(), std::io::Error> {
    std::fs::remove_dir_all(path)
}
