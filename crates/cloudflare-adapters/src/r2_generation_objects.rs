use application_ports::generation_objects::{
    GenerationObjectExactVerifyPort, GenerationObjectUploadOutcome, GenerationObjectUploadPort,
    ImmutableGenerationObject,
};
use application_ports::generations::{GenerationPortError, GenerationPortErrorClass};
use profile_platform_primitives::TenantScope;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use worker::{Bucket, Conditional, Object};

const META_TENANT_ID: &str = "profile-platform-tenant-id";
const META_PROFILE_ID: &str = "profile-platform-profile-id";
const META_GENERATION_ID: &str = "profile-platform-generation-id";
const META_METADATA_DIGEST: &str = "profile-platform-metadata-sha256";
const META_CONTAINER_DIGEST: &str = "profile-platform-container-sha256";

pub struct R2GenerationObjects {
    bucket: Bucket,
}

impl R2GenerationObjects {
    #[must_use]
    pub const fn new(bucket: Bucket) -> Self {
        Self { bucket }
    }

    fn validate_object(
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> Result<(), GenerationPortError> {
        if !is_sha256_hex(object.metadata_digest()) || !is_sha256_hex(object.container_digest()) {
            return Err(integrity_failure());
        }
        let canonical = canonical_object_key(
            scope,
            object.profile_id().as_str(),
            object.generation_id().as_str(),
        );
        if object.object_key() != canonical {
            return Err(integrity_failure());
        }
        let actual_container_digest = sha256_hex(object.container());
        if actual_container_digest != object.container_digest() {
            return Err(integrity_failure());
        }
        Ok(())
    }

    fn custom_metadata(
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> HashMap<String, String> {
        HashMap::from([
            (
                META_TENANT_ID.to_owned(),
                scope.tenant_id().as_str().to_owned(),
            ),
            (
                META_PROFILE_ID.to_owned(),
                object.profile_id().as_str().to_owned(),
            ),
            (
                META_GENERATION_ID.to_owned(),
                object.generation_id().as_str().to_owned(),
            ),
            (
                META_METADATA_DIGEST.to_owned(),
                object.metadata_digest().to_owned(),
            ),
            (
                META_CONTAINER_DIGEST.to_owned(),
                object.container_digest().to_owned(),
            ),
        ])
    }

    async fn head_exact(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> Result<Option<Object>, GenerationPortError> {
        Self::validate_object(scope, object)?;
        self.bucket
            .head(object.object_key())
            .await
            .map_err(|_| dependency_unavailable())
    }

    fn object_matches(
        stored: &Object,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> Result<bool, GenerationPortError> {
        let Ok(expected_size) = u64::try_from(object.container().len()) else {
            return Ok(false);
        };
        if stored.key() != object.object_key() || stored.size() != expected_size {
            return Ok(false);
        }
        let metadata = stored.custom_metadata().map_err(|_| integrity_failure())?;
        if metadata.get(META_TENANT_ID).map(String::as_str) != Some(scope.tenant_id().as_str())
            || metadata.get(META_PROFILE_ID).map(String::as_str)
                != Some(object.profile_id().as_str())
            || metadata.get(META_GENERATION_ID).map(String::as_str)
                != Some(object.generation_id().as_str())
            || metadata.get(META_METADATA_DIGEST).map(String::as_str)
                != Some(object.metadata_digest())
            || metadata.get(META_CONTAINER_DIGEST).map(String::as_str)
                != Some(object.container_digest())
        {
            return Ok(false);
        }
        let expected_checksum = Sha256::digest(object.container()).to_vec();
        Ok(stored.checksum().sha256.as_deref() == Some(expected_checksum.as_slice()))
    }
}

impl GenerationObjectUploadPort for R2GenerationObjects {
    async fn put_generation_object_if_absent(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> Result<GenerationObjectUploadOutcome, GenerationPortError> {
        Self::validate_object(scope, object)?;
        let checksum = Sha256::digest(object.container()).to_vec();
        let created = self
            .bucket
            .put(object.object_key(), object.container().to_vec())
            .custom_metadata(Self::custom_metadata(scope, object))
            .sha256(checksum)
            .only_if(Conditional {
                etag_does_not_match: Some("*".to_owned()),
                ..Default::default()
            })
            .execute()
            .await
            .map_err(|_| dependency_unavailable())?;
        if created.is_some() {
            return Ok(GenerationObjectUploadOutcome::Created);
        }

        let Some(stored) = self.head_exact(scope, object).await? else {
            return Err(dependency_unavailable());
        };
        if Self::object_matches(&stored, scope, object)? {
            Ok(GenerationObjectUploadOutcome::Idempotent)
        } else {
            Ok(GenerationObjectUploadOutcome::ImmutableConflict)
        }
    }
}

impl GenerationObjectExactVerifyPort for R2GenerationObjects {
    async fn verify_generation_object_exact(
        &self,
        scope: &TenantScope,
        object: &ImmutableGenerationObject<'_>,
    ) -> Result<bool, GenerationPortError> {
        let Some(stored) = self.head_exact(scope, object).await? else {
            return Ok(false);
        };
        Self::object_matches(&stored, scope, object)
    }
}

fn canonical_object_key(scope: &TenantScope, profile_id: &str, generation_id: &str) -> String {
    format!(
        "tenants/{}/profiles/{profile_id}/generations/{generation_id}.bpgc",
        scope.tenant_id().as_str()
    )
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn dependency_unavailable() -> GenerationPortError {
    GenerationPortError::new(GenerationPortErrorClass::DependencyUnavailable)
}

fn integrity_failure() -> GenerationPortError {
    GenerationPortError::new(GenerationPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{canonical_object_key, is_sha256_hex};
    use profile_platform_primitives::{TenantId, TenantScope};

    #[test]
    fn canonical_key_is_tenant_profile_and_generation_scoped()
    -> Result<(), Box<dyn std::error::Error>> {
        let scope = TenantScope::new(TenantId::parse("tenant_01JR2GEN")?);
        assert_eq!(
            canonical_object_key(&scope, "profile_01JR2GEN", "generation_01JR2GEN"),
            "tenants/tenant_01JR2GEN/profiles/profile_01JR2GEN/generations/generation_01JR2GEN.bpgc"
        );
        Ok(())
    }

    #[test]
    fn digest_shape_is_lowercase_sha256_only() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
    }
}
