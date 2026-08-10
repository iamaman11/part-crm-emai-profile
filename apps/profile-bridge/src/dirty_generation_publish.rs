use crate::dirty_generation::PreparedDirtyGeneration;
use application_ports::generation_objects::{
    GenerationObjectUploadOutcome, GenerationObjectUploadPort, ImmutableGenerationObject,
};
use application_ports::generations::{
    GenerationObjectReference, GenerationObjectStorePort, GenerationPortErrorClass,
};
use profile_platform_primitives::{GenerationId, TenantScope};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyGenerationPublishError {
    Upload(GenerationPortErrorClass),
    ImmutableConflict,
    VerificationUnavailable,
    VerificationFailed,
}

impl fmt::Display for DirtyGenerationPublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Upload(_) => "immutable generation upload failed",
            Self::ImmutableConflict => "immutable generation object conflicts with existing bytes",
            Self::VerificationUnavailable => "uploaded generation verification is unavailable",
            Self::VerificationFailed => "uploaded generation failed exact digest verification",
        })
    }
}

impl std::error::Error for DirtyGenerationPublishError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedDirtyGeneration {
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
}

impl PublishedDirtyGeneration {
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
}

pub async fn publish_prepared_dirty_generation<U, V>(
    scope: &TenantScope,
    prepared: &PreparedDirtyGeneration,
    upload: &U,
    verifier: &V,
) -> Result<PublishedDirtyGeneration, DirtyGenerationPublishError>
where
    U: GenerationObjectUploadPort,
    V: GenerationObjectStorePort,
{
    let metadata = prepared.sealed().metadata();
    if metadata.tenant_id() != scope.tenant_id() {
        return Err(DirtyGenerationPublishError::VerificationFailed);
    }
    let generation_id = metadata.generation_id().clone();
    let object_key = prepared.object_key();
    let object = ImmutableGenerationObject::new(
        &generation_id,
        &object_key,
        prepared.metadata_digest(),
        prepared.container_digest(),
        prepared.sealed().container(),
    );
    match upload
        .put_generation_object_if_absent(scope, &object)
        .await
        .map_err(|error| DirtyGenerationPublishError::Upload(error.class()))?
    {
        GenerationObjectUploadOutcome::Created | GenerationObjectUploadOutcome::Idempotent => {}
        GenerationObjectUploadOutcome::ImmutableConflict => {
            return Err(DirtyGenerationPublishError::ImmutableConflict);
        }
    }

    let reference =
        GenerationObjectReference::new(generation_id.clone(), prepared.container_digest());
    let verified = verifier
        .verify_generation_object(scope, &reference)
        .map_err(|_| DirtyGenerationPublishError::VerificationUnavailable)?;
    if !verified {
        return Err(DirtyGenerationPublishError::VerificationFailed);
    }

    Ok(PublishedDirtyGeneration {
        generation_id,
        object_key,
        metadata_digest: prepared.metadata_digest().to_owned(),
        container_digest: prepared.container_digest().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DirtyGenerationPublishError, publish_prepared_dirty_generation};
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use application_ports::generation_objects::{
        GenerationObjectUploadOutcome, GenerationObjectUploadPort, ImmutableGenerationObject,
    };
    use application_ports::generations::{
        GenerationObjectReference, GenerationObjectStorePort, GenerationPortError,
        GenerationPortErrorClass,
    };
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId, TenantScope, UnixMillis};
    use std::cell::Cell;
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
                    KeyId::parse("key_publish_dirty_01").map_err(|_| ())?,
                    [3; 32],
                ),
                NoncePrefix::new([4; 16]),
                4096,
            ))
        }
    }

    struct Upload {
        outcome: GenerationObjectUploadOutcome,
        calls: Rc<Cell<u32>>,
    }

    impl GenerationObjectUploadPort for Upload {
        async fn put_generation_object_if_absent(
            &self,
            _scope: &TenantScope,
            _object: &ImmutableGenerationObject<'_>,
        ) -> Result<GenerationObjectUploadOutcome, GenerationPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.outcome)
        }
    }

    struct Verifier {
        result: bool,
        calls: Rc<Cell<u32>>,
    }

    impl GenerationObjectStorePort for Verifier {
        type Error = ();

        fn verify_generation_object(
            &self,
            _scope: &TenantScope,
            _reference: &GenerationObjectReference,
        ) -> Result<bool, Self::Error> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.result)
        }
    }

    fn fixture() -> Result<
        (
            TenantScope,
            crate::dirty_generation::PreparedDirtyGeneration,
            std::path::PathBuf,
        ),
        Box<dyn std::error::Error>,
    > {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-publish-dirty-{}-{counter}",
            std::process::id()
        ));
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant_id = TenantId::parse(format!("tenant_01JPUBLISHDIRTY{counter}"))?;
        let profile_id = ProfileId::parse(format!("profile_01JPUBLISHDIRTY{counter}"))?;
        let base_id = GenerationId::parse(format!("generation_01JPUBLISHBASE{counter}"))?;
        let candidate_id = GenerationId::parse(format!("generation_01JPUBLISHCAND{counter}"))?;
        let source = root.create_generation(&tenant_id, &profile_id, &base_id)?;
        std::fs::write(source.path().join("prefs.js"), b"published-dirty")?;
        let mut record = LocalGenerationRecord::new(base_id, 0, UnixMillis::new(1));
        record.set_locked(true)?;
        record.begin_use(UnixMillis::new(2))?;
        record.graceful_close(UnixMillis::new(3))?;
        let mut keys = Keys;
        let prepared = prepare_dirty_generation_candidate(
            &record,
            &source,
            &root,
            &tenant_id,
            &profile_id,
            &candidate_id,
            &mut keys,
        )?;
        Ok((TenantScope::new(tenant_id), prepared, root_path))
    }

    #[test]
    fn created_or_idempotent_upload_must_verify_before_publish_success()
    -> Result<(), Box<dyn std::error::Error>> {
        for outcome in [
            GenerationObjectUploadOutcome::Created,
            GenerationObjectUploadOutcome::Idempotent,
        ] {
            let (scope, prepared, root_path) = fixture()?;
            let upload_calls = Rc::new(Cell::new(0));
            let verify_calls = Rc::new(Cell::new(0));
            let published = block_on(publish_prepared_dirty_generation(
                &scope,
                &prepared,
                &Upload {
                    outcome,
                    calls: Rc::clone(&upload_calls),
                },
                &Verifier {
                    result: true,
                    calls: Rc::clone(&verify_calls),
                },
            ))?;
            assert_eq!(upload_calls.get(), 1);
            assert_eq!(verify_calls.get(), 1);
            assert_eq!(
                published.generation_id(),
                prepared.sealed().metadata().generation_id()
            );
            assert_eq!(published.container_digest(), prepared.container_digest());
            let _ = crate::test_support::remove_test_root(&root_path);
        }
        Ok(())
    }

    #[test]
    fn immutable_conflict_never_reaches_verification() -> Result<(), Box<dyn std::error::Error>> {
        let (scope, prepared, root_path) = fixture()?;
        let upload_calls = Rc::new(Cell::new(0));
        let verify_calls = Rc::new(Cell::new(0));
        let result = block_on(publish_prepared_dirty_generation(
            &scope,
            &prepared,
            &Upload {
                outcome: GenerationObjectUploadOutcome::ImmutableConflict,
                calls: Rc::clone(&upload_calls),
            },
            &Verifier {
                result: true,
                calls: Rc::clone(&verify_calls),
            },
        ));
        assert_eq!(result, Err(DirtyGenerationPublishError::ImmutableConflict));
        assert_eq!(upload_calls.get(), 1);
        assert_eq!(verify_calls.get(), 0);
        let _ = crate::test_support::remove_test_root(&root_path);
        Ok(())
    }

    #[test]
    fn verification_failure_preserves_fail_closed_publish_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let (scope, prepared, root_path) = fixture()?;
        let result = block_on(publish_prepared_dirty_generation(
            &scope,
            &prepared,
            &Upload {
                outcome: GenerationObjectUploadOutcome::Created,
                calls: Rc::new(Cell::new(0)),
            },
            &Verifier {
                result: false,
                calls: Rc::new(Cell::new(0)),
            },
        ));
        assert_eq!(result, Err(DirtyGenerationPublishError::VerificationFailed));
        let _ = crate::test_support::remove_test_root(&root_path);
        Ok(())
    }

    #[test]
    fn upload_dependency_failure_is_preserved() -> Result<(), Box<dyn std::error::Error>> {
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

        let (scope, prepared, root_path) = fixture()?;
        let result = block_on(publish_prepared_dirty_generation(
            &scope,
            &prepared,
            &FailingUpload,
            &Verifier {
                result: true,
                calls: Rc::new(Cell::new(0)),
            },
        ));
        assert_eq!(
            result,
            Err(DirtyGenerationPublishError::Upload(
                GenerationPortErrorClass::DependencyUnavailable
            ))
        );
        let _ = crate::test_support::remove_test_root(&root_path);
        Ok(())
    }
}
