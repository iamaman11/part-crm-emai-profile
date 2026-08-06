use super::{
    CloudGenerationRepository, CloudGenerationStatus, ContainerDigest, EncryptedGenerationError,
    GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix, PlaintextDigest,
    PublishResult, open_generation, open_generation_expected, seal_generation,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId, UnixMillis};

fn ids(
    generation: &str,
) -> Result<(TenantId, ProfileId, GenerationId), Box<dyn std::error::Error>> {
    Ok((
        TenantId::parse("tenant_01JSTEP9")?,
        ProfileId::parse("profile_01JSTEP9")?,
        GenerationId::parse(generation)?,
    ))
}

fn key_id() -> Result<KeyId, EncryptedGenerationError> {
    KeyId::parse("generation_key_01JSTEP9")
}

fn key(byte: u8) -> Result<GenerationDek, EncryptedGenerationError> {
    Ok(GenerationDek::new(key_id()?, [byte; 32]))
}

fn metadata(
    generation: &str,
    prefix_byte: u8,
    plaintext: &[u8],
) -> Result<GenerationMetadata, Box<dyn std::error::Error>> {
    let (tenant_id, profile_id, generation_id) = ids(generation)?;
    Ok(GenerationMetadata::for_plaintext(
        GenerationIdentity::new(tenant_id, profile_id, generation_id, None),
        key_id()?,
        NoncePrefix::new([prefix_byte; 16]),
        1_024,
        plaintext,
    )?)
}

fn metadata_length(container: &[u8]) -> Result<usize, EncryptedGenerationError> {
    let bytes = container
        .get(8..12)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    let mut length = [0_u8; 4];
    length.copy_from_slice(bytes);
    usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| EncryptedGenerationError::InvalidContainer)
}

#[test]
fn sha256_plaintext_vector_is_stable() {
    assert_eq!(
        PlaintextDigest::calculate(b"abc").bytes(),
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]
    );
}

#[test]
fn fixed_container_vector_is_stable() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"synthetic encrypted generation vector";
    let metadata = metadata("generation_01JSTEP9VECTOR", 0x22, plaintext)?;
    let sealed = seal_generation(&metadata, &key(0x11)?, plaintext)?;
    assert_eq!(
        sealed.container_digest().bytes(),
        [
            0x5b, 0xd2, 0x26, 0xf8, 0x3e, 0x0a, 0x8c, 0xf3, 0x7d, 0xf0, 0xf8, 0x18, 0x07, 0x6a,
            0xe4, 0x66, 0x25, 0x63, 0x48, 0xc9, 0x65, 0x87, 0x14, 0xa7, 0x88, 0x3c, 0xa9, 0x95,
            0x7d, 0x75, 0x86, 0x16,
        ]
    );
    Ok(())
}

#[test]
fn chunked_round_trip_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = (0_u16..3_000)
        .map(|value| u8::try_from(value % 251))
        .collect::<Result<Vec<_>, _>>()?;
    let metadata = metadata("generation_01JSTEP9ROUNDTRIP", 0x33, &plaintext)?;
    let first = seal_generation(&metadata, &key(0x44)?, &plaintext)?;
    let second = seal_generation(&metadata, &key(0x44)?, &plaintext)?;
    assert_eq!(first, second);
    let opened = open_generation(first.container(), &key(0x44)?)?;
    assert_eq!(opened.metadata(), &metadata);
    assert_eq!(opened.plaintext(), plaintext);
    assert!(
        !first
            .container()
            .windows(plaintext.len())
            .any(|window| window == plaintext)
    );
    Ok(())
}

#[test]
fn empty_plaintext_has_authenticated_final_record() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = metadata("generation_01JSTEP9EMPTY", 0x34, &[])?;
    let sealed = seal_generation(&metadata, &key(0x45)?, &[])?;
    let opened = open_generation(sealed.container(), &key(0x45)?)?;
    assert!(opened.plaintext().is_empty());
    Ok(())
}

#[test]
fn metadata_tampering_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"metadata authentication";
    let metadata = metadata("generation_01JSTEP9META", 0x35, plaintext)?;
    let mut container = seal_generation(&metadata, &key(0x46)?, plaintext)?.into_container();
    let byte = container
        .get_mut(20)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    *byte ^= 0x01;
    assert!(open_generation(&container, &key(0x46)?).is_err());
    Ok(())
}

#[test]
fn chunk_tampering_and_truncation_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = vec![0x47; 2_048];
    let metadata = metadata("generation_01JSTEP9CHUNK", 0x36, &plaintext)?;
    let sealed = seal_generation(&metadata, &key(0x48)?, &plaintext)?;

    let mut tampered = sealed.container().to_vec();
    let metadata_end = 12_usize
        .checked_add(metadata_length(&tampered)?)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    let ciphertext_offset = metadata_end
        .checked_add(1 + 8 + 4 + 4)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    let byte = tampered
        .get_mut(ciphertext_offset)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    *byte ^= 0x01;
    assert_eq!(
        open_generation(&tampered, &key(0x48)?),
        Err(EncryptedGenerationError::AuthenticationFailed)
    );

    let mut truncated = sealed.into_container();
    truncated.pop();
    assert!(open_generation(&truncated, &key(0x48)?).is_err());
    Ok(())
}

#[test]
fn reordered_chunk_index_is_rejected_before_decryption() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = vec![0x49; 2_048];
    let metadata = metadata("generation_01JSTEP9ORDER", 0x37, &plaintext)?;
    let mut container = seal_generation(&metadata, &key(0x50)?, &plaintext)?.into_container();
    let record_start = 12_usize
        .checked_add(metadata_length(&container)?)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    let index_last_byte = record_start
        .checked_add(8)
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    *container
        .get_mut(index_last_byte)
        .ok_or(EncryptedGenerationError::InvalidContainer)? = 1;
    assert_eq!(
        open_generation(&container, &key(0x50)?),
        Err(EncryptedGenerationError::InvalidContainer)
    );
    Ok(())
}

#[test]
fn final_record_tampering_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"final marker authentication";
    let metadata = metadata("generation_01JSTEP9FINAL", 0x38, plaintext)?;
    let mut container = seal_generation(&metadata, &key(0x51)?, plaintext)?.into_container();
    let final_byte = container
        .last_mut()
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    *final_byte ^= 0x01;
    assert_eq!(
        open_generation(&container, &key(0x51)?),
        Err(EncryptedGenerationError::AuthenticationFailed)
    );
    Ok(())
}

#[test]
fn wrong_key_and_identity_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"identity binding";
    let metadata = metadata("generation_01JSTEP9IDENTITY", 0x39, plaintext)?;
    let sealed = seal_generation(&metadata, &key(0x52)?, plaintext)?;
    assert_eq!(
        open_generation(sealed.container(), &key(0x53)?),
        Err(EncryptedGenerationError::AuthenticationFailed)
    );
    let (tenant_id, profile_id, _) = ids("generation_01JSTEP9IDENTITY")?;
    let other = GenerationId::parse("generation_01JSTEP9OTHER")?;
    assert_eq!(
        open_generation_expected(
            sealed.container(),
            &key(0x52)?,
            &tenant_id,
            &profile_id,
            &other,
        ),
        Err(EncryptedGenerationError::IdentityMismatch)
    );
    Ok(())
}

#[test]
fn repository_rejects_nonce_reuse_and_immutable_conflict() -> Result<(), Box<dyn std::error::Error>>
{
    let mut repository = CloudGenerationRepository::default();
    let first_plaintext = b"first generation";
    let first = metadata("generation_01JSTEP9NONCE1", 0x40, first_plaintext)?;
    assert_eq!(
        repository.publish(
            first.clone(),
            &key(0x54)?,
            first_plaintext,
            UnixMillis::new(1)
        )?,
        PublishResult::Created
    );
    assert_eq!(
        repository.publish(
            first.clone(),
            &key(0x54)?,
            first_plaintext,
            UnixMillis::new(1)
        )?,
        PublishResult::Idempotent
    );

    let (tenant_id, profile_id, generation_id) = ids("generation_01JSTEP9NONCE2")?;
    let reused = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(tenant_id, profile_id, generation_id, None),
        key_id()?,
        first.nonce_prefix(),
        1_024,
        b"second generation",
    )?;
    assert_eq!(
        repository.publish(
            reused,
            &key(0x54)?,
            b"second generation",
            UnixMillis::new(2),
        ),
        Err(EncryptedGenerationError::NonceReuse)
    );

    let conflicting = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(
            first.tenant_id().clone(),
            first.profile_id().clone(),
            first.generation_id().clone(),
            None,
        ),
        key_id()?,
        first.nonce_prefix(),
        1_024,
        b"conflicting bytes",
    )?;
    assert_eq!(
        repository.publish(
            conflicting,
            &key(0x54)?,
            b"conflicting bytes",
            UnixMillis::new(3),
        ),
        Err(EncryptedGenerationError::ImmutableConflict)
    );
    Ok(())
}

#[test]
fn pointer_compare_and_swap_and_rollback_are_strict() -> Result<(), Box<dyn std::error::Error>> {
    let mut repository = CloudGenerationRepository::default();
    let first = metadata("generation_01JSTEP9POINTER1", 0x41, b"first")?;
    let second = metadata("generation_01JSTEP9POINTER2", 0x42, b"second")?;
    repository.publish(first.clone(), &key(0x55)?, b"first", UnixMillis::new(1))?;
    repository.publish(second.clone(), &key(0x55)?, b"second", UnixMillis::new(2))?;

    let pointer = repository.commit_current(0, first.generation_id())?;
    assert_eq!(pointer.version(), 1);
    assert_eq!(pointer.current_generation_id(), Some(first.generation_id()));
    assert_eq!(
        repository.commit_current(0, second.generation_id()),
        Err(EncryptedGenerationError::StalePointer)
    );
    let pointer = repository.commit_current(1, second.generation_id())?;
    assert_eq!(pointer.version(), 2);
    assert_eq!(
        pointer.current_generation_id(),
        Some(second.generation_id())
    );
    assert_eq!(
        pointer.rollback_generation_id(),
        Some(first.generation_id())
    );
    assert_eq!(
        repository.rollback(2, second.generation_id()),
        Err(EncryptedGenerationError::InvalidRollback)
    );
    let pointer = repository.rollback(2, first.generation_id())?;
    assert_eq!(pointer.version(), 3);
    assert_eq!(pointer.current_generation_id(), Some(first.generation_id()));
    assert_eq!(
        pointer.rollback_generation_id(),
        Some(second.generation_id())
    );
    Ok(())
}

#[test]
fn corruption_is_quarantined_and_cannot_become_current() -> Result<(), Box<dyn std::error::Error>> {
    let mut repository = CloudGenerationRepository::default();
    let metadata = metadata("generation_01JSTEP9CORRUPT", 0x43, b"corrupt me")?;
    repository.publish(
        metadata.clone(),
        &key(0x56)?,
        b"corrupt me",
        UnixMillis::new(1),
    )?;
    repository.corrupt_object_for_test(metadata.generation_id(), 20)?;
    assert!(
        repository
            .restore(metadata.generation_id(), &key(0x56)?, UnixMillis::new(2))
            .is_err()
    );
    assert_eq!(
        repository
            .record(metadata.generation_id())
            .ok_or(EncryptedGenerationError::MissingGeneration)?
            .status(),
        CloudGenerationStatus::Quarantined
    );
    assert_eq!(
        repository.commit_current(0, metadata.generation_id()),
        Err(EncryptedGenerationError::GenerationQuarantined)
    );
    assert!(repository.pointer().current_generation_id().is_none());
    Ok(())
}

#[test]
fn orphan_plan_protects_current_and_rollback_generations() -> Result<(), Box<dyn std::error::Error>>
{
    let mut repository = CloudGenerationRepository::default();
    let first = metadata("generation_01JSTEP9ORPHAN1", 0x44, b"first")?;
    let second = metadata("generation_01JSTEP9ORPHAN2", 0x45, b"second")?;
    let orphan = metadata("generation_01JSTEP9ORPHAN3", 0x46, b"orphan")?;
    repository.publish(first.clone(), &key(0x57)?, b"first", UnixMillis::new(1))?;
    repository.publish(second.clone(), &key(0x57)?, b"second", UnixMillis::new(2))?;
    repository.publish(orphan.clone(), &key(0x57)?, b"orphan", UnixMillis::new(3))?;
    repository.commit_current(0, first.generation_id())?;
    repository.commit_current(1, second.generation_id())?;

    let plan = repository.plan_orphans(UnixMillis::new(10))?;
    assert_eq!(plan.generation_ids(), [orphan.generation_id().clone()]);
    assert!(plan.reclaimable_bytes() > 0);
    Ok(())
}

#[test]
fn support_summary_is_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
    let mut repository = CloudGenerationRepository::default();
    let metadata = metadata("generation_01JSTEP9SUPPORT", 0x47, b"private payload")?;
    repository.publish(
        metadata.clone(),
        &key(0x58)?,
        b"private payload",
        UnixMillis::new(1),
    )?;
    repository.commit_current(0, metadata.generation_id())?;
    let rendered = repository.support_summary()?.render_metadata_only();
    assert!(rendered.contains("total_generations=1"));
    assert!(rendered.contains("status.verified=1"));
    assert!(!rendered.contains(metadata.generation_id().as_str()));
    assert!(!rendered.contains(metadata.key_id().as_str()));
    assert!(!rendered.contains("private payload"));
    assert!(!rendered.contains("tenants/"));
    Ok(())
}

#[test]
fn invalid_key_id_and_chunk_size_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        KeyId::parse("bad key"),
        Err(EncryptedGenerationError::InvalidKeyId)
    );
    let (tenant_id, profile_id, generation_id) = ids("generation_01JSTEP9LIMIT")?;
    assert_eq!(
        GenerationMetadata::for_plaintext(
            GenerationIdentity::new(tenant_id, profile_id, generation_id, None),
            key_id()?,
            NoncePrefix::new([0x48; 16]),
            1,
            b"small",
        ),
        Err(EncryptedGenerationError::InvalidChunkSize)
    );
    Ok(())
}

#[test]
fn container_digest_detects_byte_changes() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"digest";
    let metadata = metadata("generation_01JSTEP9DIGEST", 0x49, plaintext)?;
    let sealed = seal_generation(&metadata, &key(0x59)?, plaintext)?;
    let mut changed = sealed.container().to_vec();
    let byte = changed
        .last_mut()
        .ok_or(EncryptedGenerationError::InvalidContainer)?;
    *byte ^= 0x80;
    assert_ne!(
        ContainerDigest::calculate(sealed.container()),
        ContainerDigest::calculate(&changed)
    );
    Ok(())
}
