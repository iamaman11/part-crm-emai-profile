use crate::container::NonceDomain;
use crate::{
    ContainerDigest, EncryptedGenerationError, GenerationDek, GenerationMetadata, NoncePrefix,
    open_generation_expected, seal_generation,
};
use profile_platform_primitives::{GenerationId, UnixMillis};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CloudGenerationStatus {
    Staged,
    Verified,
    Quarantined,
}

impl CloudGenerationStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Verified => "verified",
            Self::Quarantined => "quarantined",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudGenerationRecord {
    metadata: GenerationMetadata,
    object_key: String,
    container_digest: ContainerDigest,
    status: CloudGenerationStatus,
    created_at: UnixMillis,
    verified_at: Option<UnixMillis>,
}

impl CloudGenerationRecord {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub const fn container_digest(&self) -> ContainerDigest {
        self.container_digest
    }

    #[must_use]
    pub const fn status(&self) -> CloudGenerationStatus {
        self.status
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixMillis {
        self.created_at
    }

    #[must_use]
    pub const fn verified_at(&self) -> Option<UnixMillis> {
        self.verified_at
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PointerSnapshot {
    version: u64,
    current_generation_id: Option<GenerationId>,
    rollback_generation_id: Option<GenerationId>,
}

impl PointerSnapshot {
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn current_generation_id(&self) -> Option<&GenerationId> {
        self.current_generation_id.as_ref()
    }

    #[must_use]
    pub const fn rollback_generation_id(&self) -> Option<&GenerationId> {
        self.rollback_generation_id.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishResult {
    Created,
    Idempotent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreResult {
    metadata: GenerationMetadata,
    plaintext: Vec<u8>,
}

impl RestoreResult {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }

    #[must_use]
    pub fn into_plaintext(self) -> Vec<u8> {
        self.plaintext
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrphanPlan {
    generation_ids: Vec<GenerationId>,
    reclaimable_bytes: u64,
}

impl OrphanPlan {
    #[must_use]
    pub fn generation_ids(&self) -> &[GenerationId] {
        &self.generation_ids
    }

    #[must_use]
    pub const fn reclaimable_bytes(&self) -> u64 {
        self.reclaimable_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportSummary {
    total_generations: u64,
    total_container_bytes: u64,
    status_counts: BTreeMap<CloudGenerationStatus, u64>,
    pointer_version: u64,
    has_current: bool,
    has_rollback: bool,
}

impl SupportSummary {
    #[must_use]
    pub fn render_metadata_only(&self) -> String {
        let mut output = format!(
            "schema=encrypted-generation-support-v1\ntotal_generations={}\ntotal_container_bytes={}\npointer_version={}\nhas_current={}\nhas_rollback={}\n",
            self.total_generations,
            self.total_container_bytes,
            self.pointer_version,
            self.has_current,
            self.has_rollback
        );
        for (status, count) in &self.status_counts {
            output.push_str(&format!("status.{}={}\n", status.as_str(), count));
        }
        output
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FakeImmutableObjectStore {
    objects: BTreeMap<String, Vec<u8>>,
}

impl FakeImmutableObjectStore {
    fn put_immutable(
        &mut self,
        object_key: String,
        bytes: &[u8],
    ) -> Result<bool, EncryptedGenerationError> {
        if let Some(existing) = self.objects.get(&object_key) {
            if existing == bytes {
                return Ok(false);
            }
            return Err(EncryptedGenerationError::ImmutableConflict);
        }
        self.objects.insert(object_key, bytes.to_vec());
        Ok(true)
    }

    fn get(&self, object_key: &str) -> Result<&[u8], EncryptedGenerationError> {
        self.objects
            .get(object_key)
            .map(Vec::as_slice)
            .ok_or(EncryptedGenerationError::MissingObject)
    }

    fn object_bytes(&self, object_key: &str) -> Result<u64, EncryptedGenerationError> {
        let bytes = self
            .objects
            .get(object_key)
            .ok_or(EncryptedGenerationError::MissingObject)?;
        u64::try_from(bytes.len()).map_err(|_| EncryptedGenerationError::PlaintextTooLarge)
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct CloudGenerationRepository {
    objects: FakeImmutableObjectStore,
    records: BTreeMap<GenerationId, CloudGenerationRecord>,
    nonce_claims: BTreeMap<(NonceDomain, NoncePrefix), GenerationId>,
    pointer: PointerSnapshot,
}

impl CloudGenerationRepository {
    #[must_use]
    pub fn pointer(&self) -> &PointerSnapshot {
        &self.pointer
    }

    #[must_use]
    pub fn record(&self, generation_id: &GenerationId) -> Option<&CloudGenerationRecord> {
        self.records.get(generation_id)
    }

    pub fn publish(
        &mut self,
        metadata: GenerationMetadata,
        key: &GenerationDek,
        plaintext: &[u8],
        observed_at: UnixMillis,
    ) -> Result<PublishResult, EncryptedGenerationError> {
        let sealed = seal_generation(&metadata, key, plaintext)?;
        let nonce_claim = (key.nonce_domain(), metadata.nonce_prefix());
        if let Some(claimed_generation_id) = self.nonce_claims.get(&nonce_claim)
            && claimed_generation_id != metadata.generation_id()
        {
            return Err(EncryptedGenerationError::NonceReuse);
        }

        let object_key = metadata.object_key();
        let object_created = self
            .objects
            .put_immutable(object_key.clone(), sealed.container())?;
        if let Some(existing) = self.records.get(metadata.generation_id()) {
            if existing.metadata != metadata
                || existing.object_key != object_key
                || existing.container_digest != sealed.container_digest()
            {
                return Err(EncryptedGenerationError::ImmutableConflict);
            }
            self.nonce_claims
                .entry(nonce_claim)
                .or_insert_with(|| metadata.generation_id().clone());
            return Ok(PublishResult::Idempotent);
        }

        self.nonce_claims
            .insert(nonce_claim, metadata.generation_id().clone());
        let record = CloudGenerationRecord {
            metadata: metadata.clone(),
            object_key: object_key.clone(),
            container_digest: sealed.container_digest(),
            status: CloudGenerationStatus::Staged,
            created_at: observed_at,
            verified_at: None,
        };
        self.records
            .insert(metadata.generation_id().clone(), record);

        let opened = open_generation_expected(
            self.objects.get(&object_key)?,
            key,
            metadata.tenant_id(),
            metadata.profile_id(),
            metadata.generation_id(),
        );
        match opened {
            Ok(_) => {
                let record = self
                    .records
                    .get_mut(metadata.generation_id())
                    .ok_or(EncryptedGenerationError::MissingGeneration)?;
                record.status = CloudGenerationStatus::Verified;
                record.verified_at = Some(observed_at);
                Ok(if object_created {
                    PublishResult::Created
                } else {
                    PublishResult::Idempotent
                })
            }
            Err(error) => {
                self.quarantine(metadata.generation_id())?;
                Err(error)
            }
        }
    }

    pub fn restore(
        &mut self,
        generation_id: &GenerationId,
        key: &GenerationDek,
        observed_at: UnixMillis,
    ) -> Result<RestoreResult, EncryptedGenerationError> {
        let record = self
            .records
            .get(generation_id)
            .cloned()
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        if record.status == CloudGenerationStatus::Quarantined {
            return Err(EncryptedGenerationError::GenerationQuarantined);
        }
        if record.metadata.key_id() != key.key_id() {
            return Err(EncryptedGenerationError::MetadataMismatch);
        }
        let container = self.objects.get(&record.object_key)?;
        if ContainerDigest::calculate(container) != record.container_digest {
            self.quarantine(generation_id)?;
            return Err(EncryptedGenerationError::DigestMismatch);
        }
        let opened = match open_generation_expected(
            container,
            key,
            record.metadata.tenant_id(),
            record.metadata.profile_id(),
            generation_id,
        ) {
            Ok(opened) => opened,
            Err(error) => {
                self.quarantine(generation_id)?;
                return Err(error);
            }
        };
        if observed_at < record.created_at {
            return Err(EncryptedGenerationError::TimeRegression);
        }
        let current = self
            .records
            .get_mut(generation_id)
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        current.status = CloudGenerationStatus::Verified;
        current.verified_at = Some(observed_at);
        Ok(RestoreResult {
            metadata: opened.metadata().clone(),
            plaintext: opened.into_plaintext(),
        })
    }

    pub fn commit_current(
        &mut self,
        expected_pointer_version: u64,
        generation_id: &GenerationId,
    ) -> Result<PointerSnapshot, EncryptedGenerationError> {
        self.require_pointer_version(expected_pointer_version)?;
        let record = self
            .records
            .get(generation_id)
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        match record.status {
            CloudGenerationStatus::Verified => {}
            CloudGenerationStatus::Quarantined => {
                return Err(EncryptedGenerationError::GenerationQuarantined);
            }
            CloudGenerationStatus::Staged => {
                return Err(EncryptedGenerationError::GenerationNotVerified);
            }
        }
        if self.pointer.current_generation_id.as_ref() == Some(generation_id) {
            return Ok(self.pointer.clone());
        }
        let next_version = self
            .pointer
            .version
            .checked_add(1)
            .ok_or(EncryptedGenerationError::VersionOverflow)?;
        self.pointer.rollback_generation_id = self.pointer.current_generation_id.clone();
        self.pointer.current_generation_id = Some(generation_id.clone());
        self.pointer.version = next_version;
        Ok(self.pointer.clone())
    }

    pub fn rollback(
        &mut self,
        expected_pointer_version: u64,
        generation_id: &GenerationId,
    ) -> Result<PointerSnapshot, EncryptedGenerationError> {
        self.require_pointer_version(expected_pointer_version)?;
        if self.pointer.rollback_generation_id.as_ref() != Some(generation_id) {
            return Err(EncryptedGenerationError::InvalidRollback);
        }
        let record = self
            .records
            .get(generation_id)
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        if record.status != CloudGenerationStatus::Verified {
            return Err(EncryptedGenerationError::GenerationNotVerified);
        }
        let next_version = self
            .pointer
            .version
            .checked_add(1)
            .ok_or(EncryptedGenerationError::VersionOverflow)?;
        let prior_current = self.pointer.current_generation_id.clone();
        self.pointer.current_generation_id = Some(generation_id.clone());
        self.pointer.rollback_generation_id = prior_current;
        self.pointer.version = next_version;
        Ok(self.pointer.clone())
    }

    pub fn plan_orphans(
        &self,
        created_before_or_at: UnixMillis,
    ) -> Result<OrphanPlan, EncryptedGenerationError> {
        let protected = [
            self.pointer.current_generation_id.as_ref(),
            self.pointer.rollback_generation_id.as_ref(),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>();
        let mut generation_ids = Vec::new();
        let mut reclaimable_bytes = 0_u64;
        for (generation_id, record) in &self.records {
            if record.created_at <= created_before_or_at && !protected.contains(generation_id) {
                reclaimable_bytes = reclaimable_bytes
                    .checked_add(self.objects.object_bytes(&record.object_key)?)
                    .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
                generation_ids.push(generation_id.clone());
            }
        }
        Ok(OrphanPlan {
            generation_ids,
            reclaimable_bytes,
        })
    }

    pub fn support_summary(&self) -> Result<SupportSummary, EncryptedGenerationError> {
        let mut total_container_bytes = 0_u64;
        let mut status_counts = BTreeMap::new();
        for record in self.records.values() {
            total_container_bytes = total_container_bytes
                .checked_add(self.objects.object_bytes(&record.object_key)?)
                .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
            let count = status_counts.entry(record.status).or_insert(0_u64);
            *count = count
                .checked_add(1)
                .ok_or(EncryptedGenerationError::VersionOverflow)?;
        }
        Ok(SupportSummary {
            total_generations: u64::try_from(self.records.len())
                .map_err(|_| EncryptedGenerationError::VersionOverflow)?,
            total_container_bytes,
            status_counts,
            pointer_version: self.pointer.version,
            has_current: self.pointer.current_generation_id.is_some(),
            has_rollback: self.pointer.rollback_generation_id.is_some(),
        })
    }

    fn quarantine(&mut self, generation_id: &GenerationId) -> Result<(), EncryptedGenerationError> {
        let record = self
            .records
            .get_mut(generation_id)
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        record.status = CloudGenerationStatus::Quarantined;
        record.verified_at = None;
        Ok(())
    }

    fn require_pointer_version(
        &self,
        expected_pointer_version: u64,
    ) -> Result<(), EncryptedGenerationError> {
        if self.pointer.version != expected_pointer_version {
            return Err(EncryptedGenerationError::StalePointer);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn corrupt_object_for_test(
        &mut self,
        generation_id: &GenerationId,
        offset: usize,
    ) -> Result<(), EncryptedGenerationError> {
        let object_key = self
            .records
            .get(generation_id)
            .map(|record| record.object_key.clone())
            .ok_or(EncryptedGenerationError::MissingGeneration)?;
        let object = self
            .objects
            .objects
            .get_mut(&object_key)
            .ok_or(EncryptedGenerationError::MissingObject)?;
        let byte = object
            .get_mut(offset)
            .ok_or(EncryptedGenerationError::InvalidContainer)?;
        *byte ^= 0x01;
        Ok(())
    }
}
