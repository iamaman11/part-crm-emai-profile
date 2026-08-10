use crate::browser_mail_query::BrowserMailExecutionProof;
use crate::dirty_generation::PreparedDirtyGeneration;
use crate::dirty_generation_finalize::{
    DirtyGenerationCommitClientPort, DirtyGenerationFinalizeError,
    publish_verify_and_commit_dirty_generation,
};
use crate::local_profile::{
    BridgeWorkspaceLock, LocalGenerationRecord, LocalGenerationState, LocalProfileError,
};
use application_ports::ProfileCoordinatorPort;
use application_ports::generation_objects::{
    GenerationObjectExactVerifyPort, GenerationObjectUploadPort,
};
use profile_platform_primitives::{GenerationId, TenantScope, UnixMillis};
use session_domain::ProfileLease;
use std::fmt;

#[derive(Debug)]
pub struct RetainedDirtyClose {
    lease: ProfileLease,
    workspace_lock: Option<BridgeWorkspaceLock>,
    base: LocalGenerationRecord,
}

impl RetainedDirtyClose {
    pub fn begin_after_browser_close(
        lease: ProfileLease,
        workspace_lock: BridgeWorkspaceLock,
        mut base: LocalGenerationRecord,
        now: UnixMillis,
    ) -> Result<Self, LocalProfileError> {
        if !base.is_locked() || base.state() != LocalGenerationState::InUse {
            return Err(LocalProfileError::InvalidTransition);
        }
        base.graceful_close(now)?;
        Ok(Self {
            lease,
            workspace_lock: Some(workspace_lock),
            base,
        })
    }

    #[must_use]
    pub const fn lease(&self) -> &ProfileLease {
        &self.lease
    }

    #[must_use]
    pub const fn base_record(&self) -> &LocalGenerationRecord {
        &self.base
    }

    #[must_use]
    pub const fn holds_workspace_lock(&self) -> bool {
        self.workspace_lock.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finalize<U, V, M, P>(
        &mut self,
        scope: &TenantScope,
        proof: &BrowserMailExecutionProof,
        prepared: &PreparedDirtyGeneration,
        upload: &U,
        verifier: &V,
        commit: &M,
        coordinator: &mut P,
        now: UnixMillis,
    ) -> Result<DirtyCloseCompletion, RetainedDirtyCloseError<M::Error>>
    where
        U: GenerationObjectUploadPort,
        V: GenerationObjectExactVerifyPort,
        M: DirtyGenerationCommitClientPort,
        P: ProfileCoordinatorPort,
    {
        if self.workspace_lock.is_none()
            || self.base.state() != LocalGenerationState::DirtyLocal
            || !self.base.is_locked()
            || !same_lease(&self.lease, proof.coordinator_lease())
        {
            return Err(RetainedDirtyCloseError::InvalidRetainedOwnership);
        }

        let committed = publish_verify_and_commit_dirty_generation(
            scope, proof, prepared, upload, verifier, commit,
        )
        .await
        .map_err(RetainedDirtyCloseError::Finalize)?;

        let local_outcome = match committed.apply_local_successor(&mut self.base, prepared, now) {
            Ok(candidate) => DirtyCloseLocalOutcome::CandidateAccepted(candidate),
            Err(_error) if self.base.state() == LocalGenerationState::SupersededEvictable => {
                DirtyCloseLocalOutcome::RematerializeRequired(
                    committed.published().generation_id().clone(),
                )
            }
            Err(error) => return Err(RetainedDirtyCloseError::Local(error)),
        };

        let workspace_lock = self
            .workspace_lock
            .take()
            .ok_or(RetainedDirtyCloseError::InvalidRetainedOwnership)?;
        let workspace_lock_released = workspace_lock.release().is_ok();
        if workspace_lock_released {
            self.base
                .set_locked(false)
                .map_err(RetainedDirtyCloseError::Local)?;
        }
        let coordinator_lease_released =
            workspace_lock_released && coordinator.close_lease(&self.lease).is_ok();

        Ok(DirtyCloseCompletion {
            local_outcome,
            workspace_lock_released,
            coordinator_lease_released,
        })
    }
}

fn same_lease(left: &ProfileLease, right: &ProfileLease) -> bool {
    left.tenant_id() == right.tenant_id()
        && left.profile_id() == right.profile_id()
        && left.session_id() == right.session_id()
        && left.device_id() == right.device_id()
        && left.epoch() == right.epoch()
        && left.fencing_token() == right.fencing_token()
        && left.status() == right.status()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirtyCloseLocalOutcome {
    CandidateAccepted(LocalGenerationRecord),
    RematerializeRequired(GenerationId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyCloseCompletion {
    local_outcome: DirtyCloseLocalOutcome,
    workspace_lock_released: bool,
    coordinator_lease_released: bool,
}

impl DirtyCloseCompletion {
    #[must_use]
    pub const fn local_outcome(&self) -> &DirtyCloseLocalOutcome {
        &self.local_outcome
    }

    #[must_use]
    pub const fn workspace_lock_released(&self) -> bool {
        self.workspace_lock_released
    }

    #[must_use]
    pub const fn coordinator_lease_released(&self) -> bool {
        self.coordinator_lease_released
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum RetainedDirtyCloseError<C> {
    InvalidRetainedOwnership,
    Finalize(DirtyGenerationFinalizeError<C>),
    Local(LocalProfileError),
}

impl<C: fmt::Display> fmt::Display for RetainedDirtyCloseError<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRetainedOwnership => {
                formatter.write_str("dirty close retained ownership is invalid")
            }
            Self::Finalize(error) => write!(formatter, "dirty close finalization failed: {error}"),
            Self::Local(error) => write!(formatter, "dirty close local transition failed: {error}"),
        }
    }
}

impl<C: std::error::Error> std::error::Error for RetainedDirtyCloseError<C> {}

#[cfg(test)]
mod tests {
    use super::{DirtyCloseLocalOutcome, RetainedDirtyClose, RetainedDirtyCloseError};
    use crate::browser_mail_query::BrowserMailExecutionProof;
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::dirty_generation_finalize::{
        DirtyGenerationCommitClientPort, DirtyGenerationCommitOutcome,
    };
    use crate::local_profile::{
        BridgeWorkspaceLock, LocalGenerationRecord, LocalGenerationState, MaterializationRoot,
    };
    use application_ports::ProfileCoordinatorPort;
    use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
    use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
    use application_ports::generation_objects::{
        GenerationObjectExactVerifyPort, GenerationObjectUploadOutcome, GenerationObjectUploadPort,
        ImmutableGenerationObject,
    };
    use application_ports::generations::GenerationPortError;
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, GenerationId,
        MailboxBindingId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
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
                    KeyId::parse("key_retained_dirty_close_01").map_err(|_| ())?,
                    [21; 32],
                ),
                NoncePrefix::new([22; 16]),
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

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct CommitError;

    impl std::fmt::Display for CommitError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("commit failed")
        }
    }

    impl std::error::Error for CommitError {}

    struct Commit {
        fail: bool,
    }

    impl DirtyGenerationCommitClientPort for Commit {
        type Error = CommitError;

        async fn commit_dirty_generation(
            &self,
            _scope: &TenantScope,
            _request: &crate::dirty_generation_finalize::DirtyGenerationCommitRequest,
        ) -> Result<DirtyGenerationCommitOutcome, Self::Error> {
            if self.fail {
                Err(CommitError)
            } else {
                Ok(DirtyGenerationCommitOutcome::Activated)
            }
        }
    }

    #[derive(Default)]
    struct Coordinator {
        close_calls: usize,
    }

    impl ProfileCoordinatorPort for Coordinator {
        type Error = ();

        fn acquire_lease(
            &mut self,
            _actor: &ActorContext,
            _profile_id: &ProfileId,
            _device_id: &DeviceId,
        ) -> Result<ProfileLease, Self::Error> {
            Err(())
        }

        fn close_lease(&mut self, _lease: &ProfileLease) -> Result<(), Self::Error> {
            self.close_calls += 1;
            Ok(())
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        scope: TenantScope,
        root: MaterializationRoot,
        profile_id: ProfileId,
        device_id: DeviceId,
        candidate_generation_id: GenerationId,
        retained: RetainedDirtyClose,
        proof: BrowserMailExecutionProof,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-retained-dirty-close-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant_id = TenantId::parse(format!("tenant_01JRETAINCLOSE{counter}"))?;
            let profile_id = ProfileId::parse(format!("profile_01JRETAINCLOSE{counter}"))?;
            let device_id = DeviceId::parse(format!("device_01JRETAINCLOSE{counter}"))?;
            let base_generation_id =
                GenerationId::parse(format!("generation_01JRETAINBASE{counter}"))?;
            let candidate_generation_id =
                GenerationId::parse(format!("generation_01JRETAINCAND{counter}"))?;
            let workspace = root.create_generation(&tenant_id, &profile_id, &base_generation_id)?;
            std::fs::write(workspace.path().join("prefs.js"), b"retained-dirty-close")?;
            let workspace_lock = BridgeWorkspaceLock::acquire(&workspace, &device_id, 4)?;
            let mut base = LocalGenerationRecord::new(
                base_generation_id.clone(),
                workspace.inventory()?.total_bytes(),
                UnixMillis::new(10),
            );
            base.set_locked(true)?;
            base.begin_use(UnixMillis::new(11))?;
            let lease = ProfileLease::issue(
                tenant_id.clone(),
                profile_id.clone(),
                SessionId::parse(format!("session_01JRETAINCLOSE{counter}"))?,
                device_id.clone(),
                4,
                FencingToken::parse(format!("fence_01JRETAINCLOSE{counter}"))?,
            )?;
            let retained = RetainedDirtyClose::begin_after_browser_close(
                lease.clone(),
                workspace_lock,
                base,
                UnixMillis::new(12),
            )?;
            let proof = BrowserMailExecutionProof::new(
                BrowserMailboxExecutionBinding::new(
                    MailboxBindingId::parse(format!("binding_01JRETAINCLOSE{counter}"))?,
                    profile_id.clone(),
                ),
                base_generation_id,
                DeviceJobId::parse(format!("devjob_01JRETAINCLOSE{counter}"))?,
                DeviceClaimId::parse(format!("devclaim_01JRETAINCLOSE{counter}"))?,
                6,
                lease,
            )?;
            Ok(Self {
                root_path,
                scope: TenantScope::new(tenant_id),
                root,
                profile_id,
                device_id,
                candidate_generation_id,
                retained,
                proof,
            })
        }

        fn prepared(
            &self,
        ) -> Result<crate::dirty_generation::PreparedDirtyGeneration, Box<dyn std::error::Error>>
        {
            let workspace = self.root.open_generation(
                self.scope.tenant_id(),
                &self.profile_id,
                self.retained.base_record().generation_id(),
            )?;
            let mut keys = Keys;
            Ok(prepare_dirty_generation_candidate(
                self.retained.base_record(),
                &workspace,
                &self.root,
                self.scope.tenant_id(),
                &self.profile_id,
                &self.candidate_generation_id,
                &mut keys,
            )?)
        }

        fn base_workspace(
            &self,
        ) -> Result<crate::local_profile::GenerationWorkspace, Box<dyn std::error::Error>> {
            Ok(self.root.open_generation(
                self.scope.tenant_id(),
                &self.profile_id,
                self.retained.base_record().generation_id(),
            )?)
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn commit_failure_retains_workspace_lock_and_coordinator_lease()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let prepared = fixture.prepared()?;
        let mut coordinator = Coordinator::default();

        assert_eq!(
            block_on(fixture.retained.finalize(
                &fixture.scope,
                &fixture.proof,
                &prepared,
                &Upload,
                &Verify,
                &Commit { fail: true },
                &mut coordinator,
                UnixMillis::new(13),
            )),
            Err(RetainedDirtyCloseError::Finalize(
                crate::dirty_generation_finalize::DirtyGenerationFinalizeError::Commit(CommitError)
            ))
        );
        assert!(fixture.retained.holds_workspace_lock());
        assert_eq!(
            fixture.retained.base_record().state(),
            LocalGenerationState::DirtyLocal
        );
        assert_eq!(coordinator.close_calls, 0);
        assert!(matches!(
            BridgeWorkspaceLock::acquire(&fixture.base_workspace()?, &fixture.device_id, 4),
            Err(crate::local_profile::LocalProfileError::LockBusy)
        ));
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn authoritative_commit_releases_ownership_only_after_local_successor()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let prepared = fixture.prepared()?;
        let mut coordinator = Coordinator::default();
        let completion = block_on(fixture.retained.finalize(
            &fixture.scope,
            &fixture.proof,
            &prepared,
            &Upload,
            &Verify,
            &Commit { fail: false },
            &mut coordinator,
            UnixMillis::new(13),
        ))?;

        let DirtyCloseLocalOutcome::CandidateAccepted(candidate) = completion.local_outcome()
        else {
            return Err(std::io::Error::other(
                "committed unchanged candidate must be accepted locally",
            )
            .into());
        };
        assert_eq!(candidate.generation_id(), &fixture.candidate_generation_id);
        assert_eq!(candidate.state(), LocalGenerationState::MaterializedClean);
        assert_eq!(
            fixture.retained.base_record().state(),
            LocalGenerationState::SupersededEvictable
        );
        assert!(!fixture.retained.base_record().is_locked());
        assert!(!fixture.retained.holds_workspace_lock());
        assert!(completion.workspace_lock_released());
        assert!(completion.coordinator_lease_released());
        assert_eq!(coordinator.close_calls, 1);
        let reacquired =
            BridgeWorkspaceLock::acquire(&fixture.base_workspace()?, &fixture.device_id, 4)?;
        reacquired.release()?;
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn post_commit_candidate_change_releases_old_base_and_requires_rematerialization()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let prepared = fixture.prepared()?;
        std::fs::write(
            prepared.candidate_workspace().path().join("late-change"),
            b"changed-after-sealing",
        )?;
        let mut coordinator = Coordinator::default();
        let completion = block_on(fixture.retained.finalize(
            &fixture.scope,
            &fixture.proof,
            &prepared,
            &Upload,
            &Verify,
            &Commit { fail: false },
            &mut coordinator,
            UnixMillis::new(13),
        ))?;

        assert_eq!(
            completion.local_outcome(),
            &DirtyCloseLocalOutcome::RematerializeRequired(fixture.candidate_generation_id.clone())
        );
        assert_eq!(
            fixture.retained.base_record().state(),
            LocalGenerationState::SupersededEvictable
        );
        assert!(!fixture.retained.base_record().is_locked());
        assert!(completion.workspace_lock_released());
        assert!(completion.coordinator_lease_released());
        assert_eq!(coordinator.close_calls, 1);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn post_commit_candidate_read_failure_releases_old_base_and_requires_rematerialization()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = Fixture::new()?;
        let prepared = fixture.prepared()?;
        std::fs::remove_dir_all(prepared.candidate_workspace().path())?;
        let mut coordinator = Coordinator::default();
        let completion = block_on(fixture.retained.finalize(
            &fixture.scope,
            &fixture.proof,
            &prepared,
            &Upload,
            &Verify,
            &Commit { fail: false },
            &mut coordinator,
            UnixMillis::new(13),
        ))?;

        assert_eq!(
            completion.local_outcome(),
            &DirtyCloseLocalOutcome::RematerializeRequired(fixture.candidate_generation_id.clone())
        );
        assert_eq!(
            fixture.retained.base_record().state(),
            LocalGenerationState::SupersededEvictable
        );
        assert!(!fixture.retained.base_record().is_locked());
        assert!(!fixture.retained.holds_workspace_lock());
        assert!(completion.workspace_lock_released());
        assert!(completion.coordinator_lease_released());
        assert_eq!(coordinator.close_calls, 1);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn test_actor_constructor_remains_available_for_coordinator_contract()
    -> Result<(), Box<dyn std::error::Error>> {
        let _ = ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JRETAINACTOR")?),
            ActorId::parse("actor_01JRETAINACTOR")?,
            CorrelationId::parse("corr_01JRETAINACTOR")?,
        );
        Ok(())
    }
}
