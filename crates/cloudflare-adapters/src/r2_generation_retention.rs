//! R2 effect for one already-retired immutable generation.
//!
//! D1 owns admission. This adapter owns exactly one R2 key, never prefixes,
//! LOCAL Firefox files, CURRENT, or a browser workspace.
use application_ports::generation_objects::{
    GenerationObjectCatalogReference, GenerationObjectDescriptorReadPort,
};
use application_ports::generations::{GenerationPortError, GenerationPortErrorClass};
use profile_platform_primitives::TenantScope;
use worker::Bucket;

use crate::r2_generation_objects::R2GenerationObjects;

pub struct R2GenerationRetention {
    bucket: Bucket,
}

impl R2GenerationRetention {
    #[must_use]
    pub const fn new(bucket: Bucket) -> Self {
        Self { bucket }
    }

    /// The caller must have first frozen a profile with its exact D1 deletion
    /// inventory. This shares the exact-key R2 verification and effect used
    /// by historical retirement, without prefix deletes or a second client.
    pub async fn delete_frozen_profile_exact(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectCatalogReference,
    ) -> Result<(), GenerationPortError> {
        self.delete_exact_after_d1_admission(scope, reference).await
    }

    /// Caller must have already committed the exact D1 retirement receipt.
    /// An absent object is success on retry after a lost DELETE reply.
    /// HEAD must match all immutable metadata and checksum before DELETE.
    pub async fn delete_retired_exact(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectCatalogReference,
    ) -> Result<(), GenerationPortError> {
        self.delete_exact_after_d1_admission(scope, reference).await
    }

    async fn delete_exact_after_d1_admission(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectCatalogReference,
    ) -> Result<(), GenerationPortError> {
        let verifier = R2GenerationObjects::new(self.bucket.clone());
        if verifier
            .load_generation_object_descriptor_exact(scope, reference)
            .await?
            .is_none()
        {
            return Ok(());
        }

        // No list or prefix delete. The generation is immutable; new
        // activation/grant/job is fenced by the existing D1 retire receipt.
        let deleted = self.bucket.delete(reference.object_key()).await;

        // R2 is strongly consistent. This handles a lost successful response
        // without another DELETE and rejects any unexpected reappearance.
        if verifier
            .load_generation_object_descriptor_exact(scope, reference)
            .await?
            .is_none()
        {
            return Ok(());
        }
        deleted.map_err(|_| {
            GenerationPortError::new(GenerationPortErrorClass::DependencyUnavailable)
        })?;
        Err(GenerationPortError::new(
            GenerationPortErrorClass::IntegrityFailure,
        ))
    }
}
