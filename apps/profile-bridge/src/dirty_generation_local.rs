use crate::dirty_generation::PreparedDirtyGeneration;
use crate::dirty_generation_finalize::CommittedDirtyGeneration;
use crate::local_profile::{LocalGenerationRecord, LocalProfileError};
use profile_platform_primitives::UnixMillis;

impl CommittedDirtyGeneration {
    pub fn apply_local_successor(
        &self,
        base: &mut LocalGenerationRecord,
        prepared: &PreparedDirtyGeneration,
        now: UnixMillis,
    ) -> Result<LocalGenerationRecord, LocalProfileError> {
        let metadata = prepared.sealed().metadata();
        let Some(base_generation_id) = metadata.base_generation_id() else {
            return Err(LocalProfileError::InvalidTransition);
        };
        let published = self.published();
        let expected_container_bytes = u64::try_from(prepared.sealed().container().len())
            .map_err(|_| LocalProfileError::InventorySizeOverflow)?;
        let candidate_workspace_name = prepared
            .candidate_workspace()
            .path()
            .file_name()
            .and_then(|value| value.to_str());

        if base.generation_id() != base_generation_id
            || published.generation_id() != metadata.generation_id()
            || published.object_key() != prepared.object_key()
            || published.metadata_digest() != prepared.metadata_digest()
            || published.container_digest() != prepared.container_digest()
            || published.container_bytes() != expected_container_bytes
            || candidate_workspace_name != Some(metadata.generation_id().as_str())
        {
            return Err(LocalProfileError::InvalidTransition);
        }

        let current_inventory = prepared.candidate_workspace().inventory()?;
        if current_inventory != *prepared.candidate_inventory() {
            return Err(LocalProfileError::CloneChanged);
        }

        base.supersede_with_successor(
            metadata.generation_id().clone(),
            current_inventory.total_bytes(),
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_mail_query::BrowserMailExecutionProof;
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::dirty_generation_finalize::{
        DirtyGenerationCommitClientPort, DirtyGenerationCommitOutcome,
        publish_verify_and_commit_dirty_generation,
    };
    use crate::local_profile::{LocalGenerationState, MaterializationRoot};
    use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
    use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
    use application_ports::generation_objects::{
        GenerationObjectExactVerifyPort, GenerationObjectUploadOutcome, GenerationObjectUploadPort,
        ImmutableGenerationObject,
    };
    use application_ports::generations::GenerationPortError;
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
        TenantScope,
    };
    use session_domain::ProfileLease;
    use std::future::Future;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::task::{Context, Poll, Waker};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct Keys;

    impl GenerationSealingMaterialPort for Keys {
        type Error = ();

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _generation_id: &GenerationId,
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("key_dirty_local_commit_01").map_err(|_| ())?,
                    [9; 32],
                ),
                NoncePrefix::new([10; 16]),
                4096,
            ))
        }
    }

    struct Upload;

    impl GenerationObjectUploadPort for Upload {
        async fn put_generation_object_if_absent(
            &self,
            _scope: &TenantScope,
            _object: &ImmutableGenerationObject<'_>,
        ) -> Result<GenerationObjectUploadOutcome, GenerationPortError> {
            Ok(GenerationObjectUploadOutcome::Created)
        }
    }

    struct Verify;

    impl GenerationObjectExactVerifyPort for Verify {
        async fn verify_generation_object_exact(
            &self,
            _scope: &TenantScope,
            _object: &ImmutableGenerationObject<'_>,
        ) -> Result<bool, GenerationPortError> {
            Ok(true)
        }
    }

    #[derive(Debug)]
    struct CommitError;

    impl std::fmt::Display for CommitError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("commit failed")
        }
    }

    impl std::error::Error for CommitError {}

    struct Commit;

    impl DirtyGenerationCommitClientPort for Commit {
        type Error = CommitError;

        async fn commit_dirty_generation(
            &self,
            _scope: &TenantScope,
            _request: &crate::dirty_generation_finalize::DirtyGenerationCommitRequest,
        ) -> Result<DirtyGenerationCommitOutcome, Self::Error> {
            Ok(DirtyGenerationCommitOutcome::Activated)
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        scope: TenantScope,
        base: LocalGenerationRecord,
        prepared: PreparedDirtyGeneration,
        proof: BrowserMailExecutionProof,
        candidate_generation_id: GenerationId,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-dirty-local-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant_id = TenantId::parse(format!("tenant_01JLOCALCOMMIT{counter}"))?;
            let profile_id = ProfileId::parse(format!("profile_01JLOCALCOMMIT{counter}"))?;
            let base_generation_id =
                GenerationId::parse(format!("generation_01JLOCALBASE{counter}"))?;
            let candidate_generation_id =
                GenerationId::parse(format!("generation_01JLOCALCAND{counter}"))?;
            let workspace = root.create_generation(&tenant_id, &profile_id, &base_generation_id)?;
            std::fs::write(workspace.path().join("prefs.js"), b"local-successor")?;

            let mut base = LocalGenerationRecord::new(
                base_generation_id.clone(),
                workspace.inventory()?.total_bytes(),
                UnixMillis::new(10),
            );
            base.set_locked(true)?;
            base.begin_use(UnixMillis::new(11))?;
            base.graceful_close(UnixMillis::new(12))?;

            let mut keys = Keys;
            let prepared = prepare_dirty_generation_candidate(
                &base,
                &workspace,
                &root,
                &tenant_id,
                &profile_id,
                &candidate_generation_id,
                &mut keys,
            )?;
            let lease = ProfileLease::issue(
                tenant_id.clone(),
                profile_id.clone(),
                SessionId::parse(format!("session_01JLOCALCOMMIT{counter}"))?,
                DeviceId::parse(format!("device_01JLOCALCOMMIT{counter}"))?,
                4,
                FencingToken::parse(format!("fence_01JLOCALCOMMIT{counter}"))?,
            )?;
            let proof = BrowserMailExecutionProof::new(
                BrowserMailboxExecutionBinding::new(
                    MailboxBindingId::parse(format!("binding_01JLOCALCOMMIT{counter}"))?,
                    profile_id,
                ),
                base_generation_id,
                DeviceJobId::parse(format!("devjob_01JLOCALCOMMIT{counter}"))?,
                DeviceClaimId::parse(format!("devclaim_01JLOCALCOMMIT{counter}"))?,
                6,
                lease,
            )?;

            Ok(Self {
                root_path,
                scope: TenantScope::new(tenant_id),
                base,
                prepared,
                proof,
                candidate_generation_id,
            })
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn committed_generation_creates_exact_clean_successor_and_supersedes_base()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let committed = block_on(publish_verify_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload,
            &Verify,
            &Commit,
        ))?;

        let candidate = committed.apply_local_successor(
            &mut fixture.base,
            &fixture.prepared,
            UnixMillis::new(13),
        )?;

        assert_eq!(
            fixture.base.state(),
            LocalGenerationState::SupersededEvictable
        );
        assert!(fixture.base.is_locked());
        assert_eq!(candidate.generation_id(), &fixture.candidate_generation_id);
        assert_eq!(candidate.state(), LocalGenerationState::MaterializedClean);
        assert_eq!(
            candidate.bytes(),
            fixture.prepared.candidate_inventory().total_bytes()
        );
        assert!(!candidate.is_locked());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn changed_candidate_workspace_blocks_local_successor_transition()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let committed = block_on(publish_verify_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload,
            &Verify,
            &Commit,
        ))?;
        std::fs::write(
            fixture
                .prepared
                .candidate_workspace()
                .path()
                .join("late-change"),
            b"changed-after-commit",
        )?;

        assert_eq!(
            committed.apply_local_successor(
                &mut fixture.base,
                &fixture.prepared,
                UnixMillis::new(13),
            ),
            Err(LocalProfileError::CloneChanged)
        );
        assert_eq!(fixture.base.state(), LocalGenerationState::DirtyLocal);
        fixture.cleanup();
        Ok(())
    }
}
