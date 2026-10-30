//! Installer composition only. Delivery policy, extraction, journal and installation eligibility
//! remain in their existing owners. The wizard supplies public bootstrap data from the immutable installer payload delivered by the canonical HTTPS app.

use crate::installed_connection::InstalledConnectionError;
use crate::windows_delivery_coordinator::WindowsDeliveryCoordinatorError;
use crate::windows_delivery_store::DeliveryStateStoreError;
use std::path::Path;

#[cfg(any(windows, test))]
mod bundle;
#[cfg(windows)]
mod s1;

/// Operator-only no-effect observation. Output is JSON with its own verdict;
/// a successful command execution is NOT an installation authorization.
pub fn run_s1_observation(bundle: &Path, root: &Path, copy_root: Option<&Path>) -> String {
    #[cfg(windows)]
    {
        s1::observe(bundle, root, copy_root)
    }
    #[cfg(not(windows))]
    {
        let _ = (bundle, root, copy_root);
        String::from(
            "{\"schema_version\":1,\"kind\":\"PROFILE_BRIDGE_S1_READ_ONLY_OBSERVATION\",\"verdict\":\"UNKNOWN\",\"reason_codes\":[\"WINDOWS_REQUIRED\"]}",
        )
    }
}

pub const S1_OBSERVE_ARGUMENT: &str = "--s1-observe";

pub const PREPARE_INSTALL_ARGUMENT: &str = "--delivery-prepare";
/// Versioned installer-only locator; the native staging owner chooses the directory.
pub const PREPARED_DIRECTORY_PREFIX: &str = "PROFILE_BRIDGE_PREPARED_DIRECTORY_V1=";

pub fn run_install_preparation(
    bundle: &Path,
    installation_root: &Path,
) -> Result<String, NativeInstallerError> {
    #[cfg(windows)]
    {
        windows::prepare(bundle, installation_root)
    }
    #[cfg(not(windows))]
    {
        let _ = (bundle, installation_root);
        Err(NativeInstallerError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod windows {
    use super::{
        NativeInstallerError,
        bundle::{InstallBundle, validate_directory_tree},
    };
    use crate::installed_connection::InstalledConnection;
    use crate::local_profile::{DeliveryActivationGuard, MaterializationRoot};
    use crate::windows_delivery_archive::WindowsDeliveryArchiveReader;
    use crate::windows_delivery_coordinator::WindowsDeliveryCoordinator;
    use crate::windows_delivery_download::{DeliveryDownloadRoot, WindowsReleaseAssetFetcher};
    use crate::windows_delivery_handoff::DeliveryHandoffCoordinator;
    use crate::windows_delivery_signature::WindowsCmsSignatureVerifier;
    use crate::windows_delivery_staging::DeliveryStagingRoot;
    use crate::windows_delivery_store::DeliveryStateStore;
    use std::{fs, io, path::Path};

    pub(super) fn prepare(bundle_path: &Path, root: &Path) -> Result<String, NativeInstallerError> {
        let bundle = InstallBundle::open(bundle_path)?;
        validate_directory_tree(root, true)?;
        fs::create_dir_all(root).map_err(|_| NativeInstallerError::Filesystem)?;
        validate_directory_tree(root, false)?;
        let workspace = MaterializationRoot::open_or_create(root.join("profiles"))
            .map_err(|_| NativeInstallerError::Filesystem)?;
        let _guard = DeliveryActivationGuard::acquire(&workspace)
            .map_err(|_| NativeInstallerError::ActivationGuard)?;
        let state_root = root.join("state");
        // A corrupt, partial or ambiguous existing store is never reset by setup.
        let mut store = match fs::symlink_metadata(&state_root) {
            Ok(_) => DeliveryStateStore::open(&state_root),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                DeliveryStateStore::initialize(&state_root)
            }
            Err(_) => return Err(NativeInstallerError::Filesystem),
        }
        .map_err(NativeInstallerError::StateStore)?;
        // The canonical delivery store exists only now. Installation observations
        // may start here; a rejected/corrupt store is never modified for diagnosis.
        crate::user_diagnostics::report::record_at(
            root,
            crate::user_diagnostics::report::Stage::InstallPrepare,
            crate::user_diagnostics::report::Outcome::Started,
            crate::user_diagnostics::report::Cloud::Unknown,
            crate::user_diagnostics::report::Cause::None,
            0,
        );
        let downloads = DeliveryDownloadRoot::open_or_create(root.join("downloads"))
            .map_err(|_| NativeInstallerError::Filesystem)?;
        let staging = DeliveryStagingRoot::open_or_create(root.join("releases"))
            .map_err(|_| NativeInstallerError::Filesystem)?;
        let handoff = DeliveryHandoffCoordinator::new(staging.clone(), &state_root);
        InstalledConnection::prepare(&handoff, bundle.origin)
            .map_err(NativeInstallerError::InstalledConnection)?;
        let mut verifier = WindowsCmsSignatureVerifier;
        let mut fetcher = WindowsReleaseAssetFetcher::from_system()
            .map_err(|_| NativeInstallerError::PlatformUnavailable)?;
        let mut reader = WindowsDeliveryArchiveReader::new();
        let prepared = WindowsDeliveryCoordinator::new(&downloads, &staging, &mut store)
            .prepare_installation(
                &bundle.manifest,
                &bundle.signature,
                &bundle.trust,
                &mut verifier,
                &mut fetcher,
                &mut reader,
            )
            .map_err(NativeInstallerError::Preparation)?;
        // This command deliberately does not activate another process or claim runtime health.
        // The wizard next starts the exact staged executable with --delivery-install-staged.
        prepared
            .staged()
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or(NativeInstallerError::Filesystem)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeInstallerError {
    UnsupportedPlatform,
    InvalidBundle,
    InputIo,
    Filesystem,
    ActivationGuard,
    PlatformUnavailable,
    StateStore(DeliveryStateStoreError),
    Preparation(WindowsDeliveryCoordinatorError),
    InstalledConnection(InstalledConnectionError),
}

impl std::fmt::Display for NativeInstallerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Profile Bridge installation preparation did not complete")
    }
}
impl std::error::Error for NativeInstallerError {}

#[cfg(test)]
mod tests;
