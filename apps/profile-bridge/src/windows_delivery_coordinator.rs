#![forbid(unsafe_code)]

use crate::windows_delivery::{
    AcceptedDeliveryFloor, DeliveryIdentity, DeliveryPolicyError, DeliveryState, DeliveryStateError,
    DetachedSignatureVerifier, TrustedSignerSet, VerifiedDeliveryCandidate, verify_delivery_candidate,
};
use crate::windows_delivery_download::{
    DeliveryAssetFetcher, DeliveryDownloadError, DeliveryDownloadRoot, download_verified_delivery,
};
use crate::windows_delivery_staging::{
    DeliveryArchiveReader, DeliveryStagingError, DeliveryStagingRoot, StagedDelivery,
    reopen_staged_delivery, stage_verified_delivery,
};
use crate::windows_delivery_store::{DeliveryStateStore, DeliveryStateStoreError};
use std::fmt;

/// Composition boundary for one verified Windows delivery preparation transaction.
///
/// Policy, trust, download identity, archive safety, staging layout and durable state remain owned by
/// their existing modules. This coordinator only composes those owners in the required fail-closed
/// order and deliberately does not decide quiescence, activation, health or rollback.
pub struct WindowsDeliveryCoordinator<'a> {
    download_root: &'a DeliveryDownloadRoot,
    staging_root: &'a DeliveryStagingRoot,
    state_store: &'a mut DeliveryStateStore,
}

impl<'a> WindowsDeliveryCoordinator<'a> {
    #[must_use]
    pub const fn new(
        download_root: &'a DeliveryDownloadRoot,
        staging_root: &'a DeliveryStagingRoot,
        state_store: &'a mut DeliveryStateStore,
    ) -> Self {
        Self {
            download_root,
            staging_root,
            state_store,
        }
    }

    /// Verify one signed candidate against the highest already accepted local identity, download
    /// its exact immutable assets, materialize and reopen the side-by-side stage, then publish that
    /// exact identity into the canonical state journal.
    ///
    /// A failed verification/download/stage never mutates durable delivery state. Re-preparing the
    /// already-staged exact candidate is state-idempotent and does not append a redundant snapshot.
    pub fn prepare<V, F, R>(
        &mut self,
        manifest_bytes: &[u8],
        signature_bytes: &[u8],
        trust: &TrustedSignerSet,
        verifier: &mut V,
        fetcher: &mut F,
        archive_reader: &mut R,
    ) -> Result<PreparedWindowsDelivery, WindowsDeliveryCoordinatorError>
    where
        V: DetachedSignatureVerifier,
        F: DeliveryAssetFetcher,
        R: DeliveryArchiveReader,
    {
        self.state_store
            .state()
            .validate_persisted()
            .map_err(WindowsDeliveryCoordinatorError::State)?;
        let floor = accepted_floor(self.state_store.state());
        let candidate = verify_delivery_candidate(
            manifest_bytes,
            signature_bytes,
            trust,
            floor.as_ref(),
            verifier,
        )
        .map_err(WindowsDeliveryCoordinatorError::Policy)?;

        let artifacts = download_verified_delivery(self.download_root, &candidate, fetcher)
            .map_err(WindowsDeliveryCoordinatorError::Download)?;
        let staged = stage_verified_delivery(
            self.staging_root,
            &candidate,
            artifacts.profile_bridge(),
            artifacts.runtime_bundle(),
            archive_reader,
        )
        .map_err(WindowsDeliveryCoordinatorError::Staging)?;

        let reopened = reopen_staged_delivery(self.staging_root, staged.identity())
            .map_err(WindowsDeliveryCoordinatorError::Staging)?;
        if reopened.identity() != &candidate.identity() || reopened.path() != staged.path() {
            return Err(WindowsDeliveryCoordinatorError::StageIdentityMismatch);
        }

        let mut next_state = self.state_store.state().clone();
        next_state
            .stage(&candidate)
            .map_err(WindowsDeliveryCoordinatorError::State)?;
        if &next_state != self.state_store.state() {
            self.state_store
                .persist(&next_state)
                .map_err(WindowsDeliveryCoordinatorError::Store)?;
        }

        Ok(PreparedWindowsDelivery { candidate, staged })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedWindowsDelivery {
    candidate: VerifiedDeliveryCandidate,
    staged: StagedDelivery,
}

impl PreparedWindowsDelivery {
    #[must_use]
    pub const fn candidate(&self) -> &VerifiedDeliveryCandidate {
        &self.candidate
    }

    #[must_use]
    pub const fn staged(&self) -> &StagedDelivery {
        &self.staged
    }
}

fn accepted_floor(state: &DeliveryState) -> Option<AcceptedDeliveryFloor> {
    highest_accepted_identity(state).map(AcceptedDeliveryFloor::from_identity)
}

fn highest_accepted_identity(state: &DeliveryState) -> Option<&DeliveryIdentity> {
    match (state.active(), state.staged()) {
        (Some(active), Some(staged)) if staged.sequence >= active.sequence => Some(staged),
        (Some(active), Some(_)) => Some(active),
        (Some(active), None) => Some(active),
        (None, Some(staged)) => Some(staged),
        (None, None) => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDeliveryCoordinatorError {
    Policy(DeliveryPolicyError),
    Download(DeliveryDownloadError),
    Staging(DeliveryStagingError),
    State(DeliveryStateError),
    Store(DeliveryStateStoreError),
    StageIdentityMismatch,
}

impl fmt::Display for WindowsDeliveryCoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(error) => write!(formatter, "Windows delivery candidate rejected: {error}"),
            Self::Download(error) => write!(formatter, "Windows delivery download failed: {error}"),
            Self::Staging(error) => write!(formatter, "Windows delivery staging failed: {error}"),
            Self::State(error) => write!(formatter, "Windows delivery state transition failed: {error}"),
            Self::Store(error) => write!(formatter, "Windows delivery state persistence failed: {error}"),
            Self::StageIdentityMismatch => formatter.write_str(
                "Windows delivery reopened stage does not match the verified candidate identity",
            ),
        }
    }
}

impl std::error::Error for WindowsDeliveryCoordinatorError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_delivery::{
        DetachedSignatureEnvelope, TrustedSigner, TrustedSignerStatus, WindowsDeliveryCompatibility,
        WindowsDeliveryComponent, WindowsDeliveryComponents, WindowsDeliveryEvidence,
        WindowsDeliveryManifest,
    };
    use crate::windows_delivery_staging::{
        DeliveryArchiveEntry, DeliveryArchiveEntryKind, DeliveryComponentKind,
    };
    use bridge_domain::CAMOUHOST_IPC_VERSION;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
    const CERTIFICATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BRIDGE_ARCHIVE: &[u8] = b"bridge-archive-v1";
    const RUNTIME_ARCHIVE: &[u8] = b"runtime-archive-v1";
    const BRIDGE_FILE: &[u8] = b"profile-bridge-executable-v1";
    const PYTHON_FILE: &[u8] = b"embedded-python-v1";
    const CAMOUFOX_FILE: &[u8] = b"embedded-camoufox-v1";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-coordinator-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }

        fn join(&self, child: &str) -> PathBuf {
            self.0.join(child)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct AcceptVerifier;

    impl DetachedSignatureVerifier for AcceptVerifier {
        type Error = ();

        fn verify_cms(
            &mut self,
            _manifest_bytes: &[u8],
            _cms_der: &[u8],
            _expected_certificate_sha256: &str,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FakeFetcher {
        calls: Vec<String>,
    }

    impl DeliveryAssetFetcher for FakeFetcher {
        type Error = ();

        fn fetch_release_asset(
            &mut self,
            _release_set_id: &str,
            asset_name: &str,
            destination: &Path,
            _expected_size_bytes: u64,
        ) -> Result<(), Self::Error> {
            self.calls.push(asset_name.to_owned());
            let bytes = match asset_name {
                "profile-bridge.zip" => BRIDGE_ARCHIVE,
                "runtime-bundle.tar" => RUNTIME_ARCHIVE,
                _ => return Err(()),
            };
            fs::write(destination, bytes).map_err(|_| ())
        }
    }

    struct FakeArchiveReader {
        reject_runtime: bool,
    }

    impl FakeArchiveReader {
        const fn healthy() -> Self {
            Self {
                reject_runtime: false,
            }
        }

        const fn reject_runtime() -> Self {
            Self {
                reject_runtime: true,
            }
        }
    }

    impl DeliveryArchiveReader for FakeArchiveReader {
        type Error = ();

        fn entries(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
        ) -> Result<Vec<DeliveryArchiveEntry>, Self::Error> {
            Ok(match component {
                DeliveryComponentKind::ProfileBridge => vec![DeliveryArchiveEntry::regular_file(
                    "profile-bridge.exe",
                    BRIDGE_FILE.len() as u64,
                    sha256_hex(BRIDGE_FILE),
                )],
                DeliveryComponentKind::RuntimeBundle if self.reject_runtime => {
                    vec![DeliveryArchiveEntry::link_or_special("browser/camoufox.exe")]
                }
                DeliveryComponentKind::RuntimeBundle => vec![
                    DeliveryArchiveEntry::regular_file(
                        "browser/camoufox.exe",
                        CAMOUFOX_FILE.len() as u64,
                        sha256_hex(CAMOUFOX_FILE),
                    ),
                    DeliveryArchiveEntry::regular_file(
                        "python/python.exe",
                        PYTHON_FILE.len() as u64,
                        sha256_hex(PYTHON_FILE),
                    ),
                ],
            })
        }

        fn copy_regular_file(
            &mut self,
            component: DeliveryComponentKind,
            _artifact_path: &Path,
            entry_index: usize,
            writer: &mut dyn Write,
        ) -> Result<(), Self::Error> {
            let bytes: &[u8] = match (component, entry_index) {
                (DeliveryComponentKind::ProfileBridge, 0) => BRIDGE_FILE,
                (DeliveryComponentKind::RuntimeBundle, 0) => CAMOUFOX_FILE,
                (DeliveryComponentKind::RuntimeBundle, 1) => PYTHON_FILE,
                _ => return Err(()),
            };
            writer.write_all(bytes).map_err(|_| ())
        }
    }

    fn roots(
        directory: &TestDirectory,
    ) -> Result<(DeliveryDownloadRoot, DeliveryStagingRoot, DeliveryStateStore), Box<dyn std::error::Error>> {
        let downloads = DeliveryDownloadRoot::open_or_create(directory.join("downloads"))?;
        let releases = DeliveryStagingRoot::open_or_create(directory.join("releases"))?;
        let state = DeliveryStateStore::initialize(directory.join("state"))?;
        Ok((downloads, releases, state))
    }

    fn trust() -> Result<TrustedSignerSet, DeliveryPolicyError> {
        TrustedSignerSet::new([TrustedSigner::new(
            "fixture-active",
            CERTIFICATE,
            TrustedSignerStatus::Active,
        )?])
    }

    fn signed_candidate(sequence: u64) -> Result<(Vec<u8>, Vec<u8>), serde_json::Error> {
        let manifest = WindowsDeliveryManifest {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY".to_owned(),
            release_set_id: format!("release-set-v3-sha256-{}", "1".repeat(64)),
            sequence,
            source_commit_sha: "2".repeat(40),
            components: WindowsDeliveryComponents {
                profile_bridge: WindowsDeliveryComponent {
                    release_id: format!("profile-bridge-v2-sha256-{}", "3".repeat(64)),
                    artifact_sha256: sha256_hex(BRIDGE_ARCHIVE),
                    artifact_size_bytes: BRIDGE_ARCHIVE.len() as u64,
                    component_manifest_sha256: "4".repeat(64),
                },
                runtime_bundle: WindowsDeliveryComponent {
                    release_id: format!("runtime-bundle-v2-sha256-{}", "5".repeat(64)),
                    artifact_sha256: sha256_hex(RUNTIME_ARCHIVE),
                    artifact_size_bytes: RUNTIME_ARCHIVE.len() as u64,
                    component_manifest_sha256: "6".repeat(64),
                },
            },
            evidence: WindowsDeliveryEvidence {
                sbom_sha256: "7".repeat(64),
                provenance_sha256: "8".repeat(64),
            },
            compatibility: WindowsDeliveryCompatibility {
                profile_bridge_protocol_version: 1,
                camouhost_ipc_version: CAMOUHOST_IPC_VERSION,
                runtime_bundle_version: "2.0.0".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let signature_bytes = serde_json::to_vec(&DetachedSignatureEnvelope {
            schema_version: 1,
            kind: "WINDOWS_PROFILE_BRIDGE_DELIVERY_CMS".to_owned(),
            key_id: "fixture-active".to_owned(),
            cms_der_hex: "00".to_owned(),
        })?;
        Ok((manifest_bytes, signature_bytes))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut encoded, "{byte:02x}");
        }
        encoded
    }

    #[test]
    fn prepare_composes_exact_stage_and_durable_identity_without_activation() -> TestResult {
        let directory = TestDirectory::create("prepare")?;
        let (downloads, releases, mut store) = roots(&directory)?;
        let (manifest, signature) = signed_candidate(1)?;
        let trust = trust()?;
        let mut verifier = AcceptVerifier;
        let mut fetcher = FakeFetcher::default();
        let mut reader = FakeArchiveReader::healthy();

        let prepared = WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &manifest,
            &signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        )?;

        assert_eq!(store.revision(), 2);
        assert_eq!(store.state().staged(), Some(prepared.staged().identity()));
        assert_eq!(store.state().active(), None);
        assert_eq!(
            fs::read(prepared.staged().profile_bridge_root().join("profile-bridge.exe"))?,
            BRIDGE_FILE
        );
        assert_eq!(
            fs::read(prepared.staged().runtime_root().join("python/python.exe"))?,
            PYTHON_FILE
        );
        assert_eq!(fetcher.calls, ["profile-bridge.zip", "runtime-bundle.tar"]);
        Ok(())
    }

    #[test]
    fn exact_candidate_reprepare_is_identity_and_state_journal_idempotent() -> TestResult {
        let directory = TestDirectory::create("idempotent")?;
        let (downloads, releases, mut store) = roots(&directory)?;
        let (manifest, signature) = signed_candidate(1)?;
        let trust = trust()?;
        let mut verifier = AcceptVerifier;
        let mut fetcher = FakeFetcher::default();
        let mut reader = FakeArchiveReader::healthy();

        let first = WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &manifest,
            &signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        )?;
        let revision = store.revision();
        let snapshot = store.snapshot_sha256().to_owned();
        let first_path = first.staged().path().to_path_buf();

        let second = WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &manifest,
            &signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        )?;

        assert_eq!(store.revision(), revision);
        assert_eq!(store.snapshot_sha256(), snapshot);
        assert_eq!(second.staged().path(), first_path);
        assert_eq!(fetcher.calls.len(), 2);
        assert_eq!(second.candidate().identity(), first.candidate().identity());
        Ok(())
    }

    #[test]
    fn highest_staged_floor_rejects_downgrade_before_network_fetch() -> TestResult {
        let directory = TestDirectory::create("downgrade")?;
        let (downloads, releases, mut store) = roots(&directory)?;
        let trust = trust()?;
        let mut verifier = AcceptVerifier;
        let mut fetcher = FakeFetcher::default();
        let mut reader = FakeArchiveReader::healthy();
        let (newer_manifest, newer_signature) = signed_candidate(2)?;
        WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &newer_manifest,
            &newer_signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        )?;
        let calls_after_newer = fetcher.calls.len();
        let revision_after_newer = store.revision();

        let (older_manifest, older_signature) = signed_candidate(1)?;
        let result = WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &older_manifest,
            &older_signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        );

        assert_eq!(
            result,
            Err(WindowsDeliveryCoordinatorError::Policy(
                DeliveryPolicyError::DowngradeRejected
            ))
        );
        assert_eq!(fetcher.calls.len(), calls_after_newer);
        assert_eq!(store.revision(), revision_after_newer);
        Ok(())
    }

    #[test]
    fn staging_failure_never_publishes_candidate_to_state() -> TestResult {
        let directory = TestDirectory::create("stage-failure")?;
        let (downloads, releases, mut store) = roots(&directory)?;
        let (manifest, signature) = signed_candidate(1)?;
        let trust = trust()?;
        let mut verifier = AcceptVerifier;
        let mut fetcher = FakeFetcher::default();
        let mut reader = FakeArchiveReader::reject_runtime();

        let result = WindowsDeliveryCoordinator::new(&downloads, &releases, &mut store).prepare(
            &manifest,
            &signature,
            &trust,
            &mut verifier,
            &mut fetcher,
            &mut reader,
        );

        assert!(matches!(
            result,
            Err(WindowsDeliveryCoordinatorError::Staging(
                DeliveryStagingError::UnsupportedArchiveEntry
            ))
        ));
        assert_eq!(store.revision(), 1);
        assert_eq!(store.state().staged(), None);
        assert_eq!(store.state().active(), None);
        Ok(())
    }

    #[test]
    fn archive_reader_contract_surfaces_special_entries_before_copy() {
        let entry = DeliveryArchiveEntry::link_or_special("link");
        assert_eq!(entry.kind(), DeliveryArchiveEntryKind::LinkOrSpecial);
    }
}
