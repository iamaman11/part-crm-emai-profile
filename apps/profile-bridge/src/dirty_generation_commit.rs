use crate::browser_mail_query::BrowserMailExecutionProof;
use crate::dirty_generation::PreparedDirtyGeneration;
use crate::dirty_generation_publish::{
    DirtyGenerationPublishError, PublishedDirtyGeneration, publish_prepared_dirty_generation,
};
use application_ports::generation_objects::{GenerationObjectExactVerifyPort, GenerationObjectUploadPort};
use core::future::Future;
use device_domain::{DeviceClaimId, DeviceJobId};
use profile_platform_primitives::{
    FencingToken, GenerationId, ProfileId, SessionId, TenantScope,
};
use session_domain::LeaseStatus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyGenerationCommitRequest {
    profile_id: ProfileId,
    base_generation_id: GenerationId,
    device_job_id: DeviceJobId,
    device_claim_id: DeviceClaimId,
    device_job_fence: u64,
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    coordinator_session_id: SessionId,
    coordinator_fencing_token: FencingToken,
    coordinator_epoch: u64,
}

impl DirtyGenerationCommitRequest {
    fn from_published(
        proof: &BrowserMailExecutionProof,
        published: &PublishedDirtyGeneration,
    ) -> Self {
        let lease = proof.coordinator_lease();
        Self {
            profile_id: lease.profile_id().clone(),
            base_generation_id: proof.generation_id().clone(),
            device_job_id: proof.device_job_id().clone(),
            device_claim_id: proof.device_claim_id().clone(),
            device_job_fence: proof.device_job_fence(),
            generation_id: published.generation_id().clone(),
            object_key: published.object_key().to_owned(),
            metadata_digest: published.metadata_digest().to_owned(),
            container_digest: published.container_digest().to_owned(),
            container_bytes: published.container_bytes(),
            coordinator_session_id: lease.session_id().clone(),
            coordinator_fencing_token: lease.fencing_token().clone(),
            coordinator_epoch: lease.epoch(),
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn base_generation_id(&self) -> &GenerationId {
        &self.base_generation_id
    }

    #[must_use]
    pub const fn device_job_id(&self) -> &DeviceJobId {
        &self.device_job_id
    }

    #[must_use]
    pub const fn device_claim_id(&self) -> &DeviceClaimId {
        &self.device_claim_id
    }

    #[must_use]
    pub const fn device_job_fence(&self) -> u64 {
        self.device_job_fence
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }

    #[must_use]
    pub const fn coordinator_session_id(&self) -> &SessionId {
        &self.coordinator_session_id
    }

    #[must_use]
    pub const fn coordinator_fencing_token(&self) -> &FencingToken {
        &self.coordinator_fencing_token
    }

    #[must_use]
    pub const fn coordinator_epoch(&self) -> u64 {
        self.coordinator_epoch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyGenerationCommitOutcome {
    Activated,
    AlreadyActive,
}

pub trait DirtyGenerationCommitClientPort {
    type Error;

    fn commit_dirty_generation(
        &self,
        scope: &TenantScope,
        request: &DirtyGenerationCommitRequest,
    ) -> impl Future<Output = Result<DirtyGenerationCommitOutcome, Self::Error>>;
}

#[derive(Debug, Eq, PartialEq)]
pub enum DirtyGenerationCommitFlowError<C> {
    InvalidExecutionProof,
    Publish(DirtyGenerationPublishError),
    Commit(C),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedDirtyGeneration {
    published: PublishedDirtyGeneration,
    outcome: DirtyGenerationCommitOutcome,
}

impl CommittedDirtyGeneration {
    #[must_use]
    pub const fn published(&self) -> &PublishedDirtyGeneration {
        &self.published
    }

    #[must_use]
    pub const fn outcome(&self) -> DirtyGenerationCommitOutcome {
        self.outcome
    }
}

pub async fn publish_and_commit_dirty_generation<U, V, C>(
    scope: &TenantScope,
    proof: &BrowserMailExecutionProof,
    prepared: &PreparedDirtyGeneration,
    upload: &U,
    verifier: &V,
    commit: &C,
) -> Result<CommittedDirtyGeneration, DirtyGenerationCommitFlowError<C::Error>>
where
    U: GenerationObjectUploadPort,
    V: GenerationObjectExactVerifyPort,
    C: DirtyGenerationCommitClientPort,
{
    validate_execution(scope, proof, prepared)
        .map_err(|()| DirtyGenerationCommitFlowError::InvalidExecutionProof)?;

    let published = publish_prepared_dirty_generation(scope, prepared, upload, verifier)
        .await
        .map_err(DirtyGenerationCommitFlowError::Publish)?;
    let request = DirtyGenerationCommitRequest::from_published(proof, &published);
    let outcome = commit
        .commit_dirty_generation(scope, &request)
        .await
        .map_err(DirtyGenerationCommitFlowError::Commit)?;

    Ok(CommittedDirtyGeneration { published, outcome })
}

fn validate_execution(
    scope: &TenantScope,
    proof: &BrowserMailExecutionProof,
    prepared: &PreparedDirtyGeneration,
) -> Result<(), ()> {
    let lease = proof.coordinator_lease();
    let metadata = prepared.sealed().metadata();
    if lease.status() != LeaseStatus::Active
        || lease.tenant_id() != scope.tenant_id()
        || lease.profile_id() != metadata.profile_id()
        || metadata.base_generation_id() != Some(proof.generation_id())
        || proof.device_job_fence() == 0
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DirtyGenerationCommitClientPort, DirtyGenerationCommitFlowError,
        DirtyGenerationCommitOutcome, DirtyGenerationCommitRequest,
        publish_and_commit_dirty_generation,
    };
    use crate::browser_mail_query::BrowserMailExecutionProof;
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
    use application_ports::generation_objects::{
        GenerationObjectExactVerifyPort, GenerationObjectUploadOutcome, GenerationObjectUploadPort,
        ImmutableGenerationObject,
    };
    use application_ports::generations::{GenerationPortError, GenerationPortErrorClass};
    use device_domain::{DeviceClaimId, DeviceJobId};
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
        TenantScope, UnixMillis,
    };
    use session_domain::ProfileLease;
    use std::cell::RefCell;
    use std::future::Future;
    use std::rc::Rc;
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
                    KeyId::parse("key_dirty_commit_01").map_err(|_| ())?,
                    [7; 32],
                ),
                NoncePrefix::new([8; 16]),
                4096,
            ))
        }
    }

    struct Upload {
        events: Rc<RefCell<Vec<&'static str>>>,
        outcome: GenerationObjectUploadOutcome,
    }

    impl GenerationObjectUploadPort for Upload {
        async fn put_generation_object_if_absent(
            &self,
            _scope: &TenantScope,
            _object: &ImmutableGenerationObject<'_>,
        ) -> Result<GenerationObjectUploadOutcome, GenerationPortError> {
            self.events.borrow_mut().push("upload");
            Ok(self.outcome)
        }
    }

    struct Verifier {
        events: Rc<RefCell<Vec<&'static str>>>,
        result: bool,
    }

    impl GenerationObjectExactVerifyPort for Verifier {
        async fn verify_generation_object_exact(
            &self,
            _scope: &TenantScope,
            _object: &ImmutableGenerationObject<'_>,
        ) -> Result<bool, GenerationPortError> {
            self.events.borrow_mut().push("verify");
            Ok(self.result)
        }
    }

    struct Commit {
        events: Rc<RefCell<Vec<&'static str>>>,
        observed: Rc<RefCell<Option<DirtyGenerationCommitRequest>>>,
        fail: bool,
    }

    impl DirtyGenerationCommitClientPort for Commit {
        type Error = ();

        async fn commit_dirty_generation(
            &self,
            _scope: &TenantScope,
            request: &DirtyGenerationCommitRequest,
        ) -> Result<DirtyGenerationCommitOutcome, Self::Error> {
            self.events.borrow_mut().push("commit");
            self.observed.replace(Some(request.clone()));
            if self.fail {
                Err(())
            } else {
                Ok(DirtyGenerationCommitOutcome::Activated)
            }
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        scope: TenantScope,
        proof: BrowserMailExecutionProof,
        prepared: crate::dirty_generation::PreparedDirtyGeneration,
        base_generation_id: GenerationId,
        candidate_generation_id: GenerationId,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-dirty-commit-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant_id = TenantId::parse(format!("tenant_01JDIRTYCOMMIT{counter}"))?;
            let profile_id = ProfileId::parse(format!("profile_01JDIRTYCOMMIT{counter}"))?;
            let base_generation_id = GenerationId::parse(format!("generation_01JBASEC{counter}"))?;
            let candidate_generation_id =
                GenerationId::parse(format!("generation_01JCANDC{counter}"))?;
            let workspace =
                root.create_generation(&tenant_id, &profile_id, &base_generation_id)?;
            std::fs::write(workspace.path().join("prefs.js"), b"dirty-generation-commit")?;
            let mut record =
                LocalGenerationRecord::new(base_generation_id.clone(), 0, UnixMillis::new(10));
            record.set_locked(true)?;
            record.begin_use(UnixMillis::new(11))?;
            record.graceful_close(UnixMillis::new(12))?;
            let mut keys = Keys;
            let prepared = prepare_dirty_generation_candidate(
                &record,
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
                SessionId::parse(format!("session_01JDIRTYC{counter}"))?,
                DeviceId::parse(format!("device_01JDIRTYC{counter}"))?,
                3,
                FencingToken::parse(format!("fence_01JDIRTYC{counter}"))?,
            )?;
            let proof = BrowserMailExecutionProof::new(
                BrowserMailboxExecutionBinding::new(
                    MailboxBindingId::parse(format!("binding_01JDIRTYC{counter}"))?,
                    profile_id,
                ),
                base_generation_id.clone(),
                DeviceJobId::parse(format!("devjob_01JDIRTYC{counter}"))?,
                DeviceClaimId::parse(format!("devclaim_01JDIRTYC{counter}"))?,
                5,
                lease,
            )?;
            Ok(Self {
                root_path,
                scope: TenantScope::new(tenant_id),
                proof,
                prepared,
                base_generation_id,
                candidate_generation_id,
            })
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn exact_publish_verifies_before_metadata_commit() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload {
                events: Rc::clone(&events),
                outcome: GenerationObjectUploadOutcome::Created,
            },
            &Verifier {
                events: Rc::clone(&events),
                result: true,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: false,
            },
        ))?;
        assert_eq!(events.borrow().as_slice(), &["upload", "verify", "commit"]);
        assert_eq!(result.outcome(), DirtyGenerationCommitOutcome::Activated);
        let request = observed.borrow();
        let request = request.as_ref().expect("commit request observed");
        assert_eq!(request.base_generation_id(), &fixture.base_generation_id);
        assert_eq!(request.generation_id(), &fixture.candidate_generation_id);
        assert_eq!(
            request.coordinator_fencing_token(),
            fixture.proof.coordinator_lease().fencing_token()
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn publish_failure_never_reaches_commit() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload {
                events: Rc::clone(&events),
                outcome: GenerationObjectUploadOutcome::ImmutableConflict,
            },
            &Verifier {
                events: Rc::clone(&events),
                result: true,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: false,
            },
        ));
        assert!(matches!(result, Err(DirtyGenerationCommitFlowError::Publish(_))));
        assert_eq!(events.borrow().as_slice(), &["upload"]);
        assert!(observed.borrow().is_none());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn commit_failure_preserves_published_candidate_as_non_authoritative_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload {
                events: Rc::clone(&events),
                outcome: GenerationObjectUploadOutcome::Created,
            },
            &Verifier {
                events: Rc::clone(&events),
                result: true,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: true,
            },
        ));
        assert_eq!(result, Err(DirtyGenerationCommitFlowError::Commit(())));
        assert_eq!(events.borrow().as_slice(), &["upload", "verify", "commit"]);
        assert!(observed.borrow().is_some());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn mismatched_base_generation_fails_before_upload() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let lease = fixture.proof.coordinator_lease().clone();
        let mismatched = BrowserMailExecutionProof::new(
            fixture.proof.execution_binding().clone(),
            GenerationId::parse("generation_01JWRONGBASE")?,
            fixture.proof.device_job_id().clone(),
            fixture.proof.device_claim_id().clone(),
            fixture.proof.device_job_fence(),
            lease,
        )?;
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &mismatched,
            &fixture.prepared,
            &Upload {
                events: Rc::clone(&events),
                outcome: GenerationObjectUploadOutcome::Created,
            },
            &Verifier {
                events: Rc::clone(&events),
                result: true,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: false,
            },
        ));
        assert_eq!(result, Err(DirtyGenerationCommitFlowError::InvalidExecutionProof));
        assert!(events.borrow().is_empty());
        assert!(observed.borrow().is_none());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn verifier_failure_never_reaches_commit() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &Upload {
                events: Rc::clone(&events),
                outcome: GenerationObjectUploadOutcome::Created,
            },
            &Verifier {
                events: Rc::clone(&events),
                result: false,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: false,
            },
        ));
        assert!(matches!(result, Err(DirtyGenerationCommitFlowError::Publish(_))));
        assert_eq!(events.borrow().as_slice(), &["upload", "verify"]);
        assert!(observed.borrow().is_none());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn upload_dependency_failure_is_not_converted_into_commit_attempt()
    -> Result<(), Box<dyn std::error::Error>> {
        struct FailingUpload;
        impl GenerationObjectUploadPort for FailingUpload {
            async fn put_generation_object_if_absent(
                &self,
                _scope: &TenantScope,
                _object: &ImmutableGenerationObject<'_>,
            ) -> Result<GenerationObjectUploadOutcome, GenerationPortError> {
                Err(GenerationPortError::new(
                    GenerationPortErrorClass::DependencyUnavailable,
                ))
            }
        }

        let fixture = Fixture::new()?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::new(RefCell::new(None));
        let result = block_on(publish_and_commit_dirty_generation(
            &fixture.scope,
            &fixture.proof,
            &fixture.prepared,
            &FailingUpload,
            &Verifier {
                events: Rc::clone(&events),
                result: true,
            },
            &Commit {
                events: Rc::clone(&events),
                observed: Rc::clone(&observed),
                fail: false,
            },
        ));
        assert!(matches!(result, Err(DirtyGenerationCommitFlowError::Publish(_))));
        assert!(events.borrow().is_empty());
        assert!(observed.borrow().is_none());
        fixture.cleanup();
        Ok(())
    }
}
