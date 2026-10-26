use crate::{CONTAINER_VERSION, EncryptedGenerationError};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: [u8; 8] = *b"BPGC0001";
const ALGORITHM_ID: u16 = 1;
const RECORD_CHUNK: u8 = 1;
const RECORD_FINAL: u8 = 2;
const TAG_BYTES: usize = 16;
const NONCE_PREFIX_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const MIN_CHUNK_BYTES: u32 = 1_024;
const MAX_CHUNK_BYTES: u32 = 1_048_576;
const MAX_PLAINTEXT_BYTES: usize = 67_108_864;
/// Streaming-only limit; old all-in-RAM APIs remain capped at 64 MiB.
pub const MAX_STREAM_PLAINTEXT_BYTES: u64 = 536_870_912;
pub const MAX_STREAM_CONTAINER_BYTES: u64 = 603_979_776;
const MAX_CONTAINER_BYTES: usize = 83_886_080;
const MAX_METADATA_BYTES: usize = 4_096;
const MIN_KEY_ID_BYTES: usize = 8;
const MAX_KEY_ID_BYTES: usize = 96;
pub const MAX_GENERATION_METADATA_PRELUDE_BYTES: usize = MAGIC.len() + 4 + MAX_METADATA_BYTES;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeyId(String);

impl KeyId {
    pub fn parse(value: impl Into<String>) -> Result<Self, EncryptedGenerationError> {
        let value = value.into();
        let valid_length = (MIN_KEY_ID_BYTES..=MAX_KEY_ID_BYTES).contains(&value.len());
        let valid_chars = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !valid_length || !valid_chars {
            return Err(EncryptedGenerationError::InvalidKeyId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct NonceDomain([u8; 32]);

impl Drop for NonceDomain {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub struct GenerationDek {
    key_id: KeyId,
    bytes: [u8; 32],
}

impl GenerationDek {
    #[must_use]
    pub const fn new(key_id: KeyId, bytes: [u8; 32]) -> Self {
        Self { key_id, bytes }
    }

    #[must_use]
    pub const fn key_id(&self) -> &KeyId {
        &self.key_id
    }

    pub(crate) fn nonce_domain(&self) -> NonceDomain {
        let mut digest = Sha256::new();
        digest.update(b"profile-platform-generation-nonce-domain-v1");
        digest.update(self.bytes.as_slice());
        NonceDomain(digest.finalize().into())
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, EncryptedGenerationError> {
        XChaCha20Poly1305::new_from_slice(&self.bytes)
            .map_err(|_| EncryptedGenerationError::MetadataMismatch)
    }
}

impl Drop for GenerationDek {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NoncePrefix([u8; NONCE_PREFIX_BYTES]);

impl NoncePrefix {
    #[must_use]
    pub const fn new(bytes: [u8; NONCE_PREFIX_BYTES]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; NONCE_PREFIX_BYTES] {
        self.0
    }

    fn nonce_for(self, index: u64) -> [u8; NONCE_BYTES] {
        let mut nonce = [0_u8; NONCE_BYTES];
        nonce[..NONCE_PREFIX_BYTES].copy_from_slice(&self.0);
        nonce[NONCE_PREFIX_BYTES..].copy_from_slice(&index.to_be_bytes());
        nonce
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaintextDigest([u8; 32]);

impl PlaintextDigest {
    #[must_use]
    pub fn calculate(plaintext: &[u8]) -> Self {
        Self(Sha256::digest(plaintext).into())
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataDigest([u8; 32]);

impl MetadataDigest {
    #[must_use]
    fn calculate(metadata_bytes: &[u8]) -> Self {
        Self(Sha256::digest(metadata_bytes).into())
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerDigest([u8; 32]);

impl ContainerDigest {
    #[must_use]
    pub fn calculate(container: &[u8]) -> Self {
        Self(Sha256::digest(container).into())
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationIdentity {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    base_generation_id: Option<GenerationId>,
}

impl GenerationIdentity {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        base_generation_id: Option<GenerationId>,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationMetadata {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    base_generation_id: Option<GenerationId>,
    key_id: KeyId,
    nonce_prefix: NoncePrefix,
    chunk_size: u32,
    plaintext_bytes: u64,
    plaintext_digest: PlaintextDigest,
}

impl GenerationMetadata {
    pub fn for_plaintext(
        identity: GenerationIdentity,
        key_id: KeyId,
        nonce_prefix: NoncePrefix,
        chunk_size: u32,
        plaintext: &[u8],
    ) -> Result<Self, EncryptedGenerationError> {
        validate_chunk_size(chunk_size)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(EncryptedGenerationError::PlaintextTooLarge);
        }
        let plaintext_bytes = u64::try_from(plaintext.len())
            .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        let GenerationIdentity {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
        } = identity;
        Ok(Self {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
            key_id,
            nonce_prefix,
            chunk_size,
            plaintext_bytes,
            plaintext_digest: PlaintextDigest::calculate(plaintext),
        })
    }

    /// Streaming first-pass SHA-256. The caller must use an immutable,
    /// checked source for both this pass and the subsequent seal operation.
    pub fn for_reader(
        identity: GenerationIdentity,
        key_id: KeyId,
        nonce_prefix: NoncePrefix,
        chunk_size: u32,
        source: &mut impl Read,
    ) -> Result<Self, StreamingGenerationError> {
        validate_chunk_size(chunk_size)?;
        let mut digest = Sha256::new();
        let mut plaintext_bytes = 0_u64;
        let mut buffer = Zeroizing::new(vec![0_u8; 65_536]);
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            plaintext_bytes = plaintext_bytes
                .checked_add(
                    u64::try_from(count)
                        .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?,
                )
                .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
            if plaintext_bytes > MAX_STREAM_PLAINTEXT_BYTES {
                return Err(EncryptedGenerationError::PlaintextTooLarge.into());
            }
            digest.update(&buffer[..count]);
        }
        let GenerationIdentity {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
        } = identity;
        Ok(Self {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
            key_id,
            nonce_prefix,
            chunk_size,
            plaintext_bytes,
            plaintext_digest: PlaintextDigest(digest.finalize().into()),
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn base_generation_id(&self) -> Option<&GenerationId> {
        self.base_generation_id.as_ref()
    }

    #[must_use]
    pub const fn key_id(&self) -> &KeyId {
        &self.key_id
    }

    #[must_use]
    pub const fn nonce_prefix(&self) -> NoncePrefix {
        self.nonce_prefix
    }

    #[must_use]
    pub const fn chunk_size(&self) -> u32 {
        self.chunk_size
    }

    #[must_use]
    pub const fn plaintext_bytes(&self) -> u64 {
        self.plaintext_bytes
    }

    #[must_use]
    pub const fn plaintext_digest(&self) -> PlaintextDigest {
        self.plaintext_digest
    }

    #[must_use]
    pub fn object_key(&self) -> String {
        format!(
            "tenants/{}/profiles/{}/generations/{}.bpgc",
            self.tenant_id.as_str(),
            self.profile_id.as_str(),
            self.generation_id.as_str()
        )
    }

    pub fn canonical_digest(&self) -> Result<MetadataDigest, EncryptedGenerationError> {
        Ok(MetadataDigest::calculate(&self.encode()?))
    }

    fn validate_plaintext(&self, plaintext: &[u8]) -> Result<(), EncryptedGenerationError> {
        let bytes = u64::try_from(plaintext.len())
            .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        if bytes != self.plaintext_bytes
            || PlaintextDigest::calculate(plaintext) != self.plaintext_digest
        {
            return Err(EncryptedGenerationError::MetadataMismatch);
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EncryptedGenerationError> {
        validate_chunk_size(self.chunk_size)?;
        if self.plaintext_bytes > MAX_STREAM_PLAINTEXT_BYTES {
            return Err(EncryptedGenerationError::PlaintextTooLarge);
        }
        let mut output = Vec::with_capacity(512);
        push_u16(&mut output, CONTAINER_VERSION);
        push_u16(&mut output, ALGORITHM_ID);
        push_string(&mut output, self.tenant_id.as_str())?;
        push_string(&mut output, self.profile_id.as_str())?;
        push_string(&mut output, self.generation_id.as_str())?;
        match &self.base_generation_id {
            Some(generation_id) => {
                output.push(1);
                push_string(&mut output, generation_id.as_str())?;
            }
            None => output.push(0),
        }
        push_string(&mut output, self.key_id.as_str())?;
        output.extend_from_slice(&self.nonce_prefix.bytes());
        push_u32(&mut output, self.chunk_size);
        push_u64(&mut output, self.plaintext_bytes);
        output.extend_from_slice(&self.plaintext_digest.bytes());
        if output.len() > MAX_METADATA_BYTES {
            return Err(EncryptedGenerationError::InvalidContainer);
        }
        Ok(output)
    }

    fn decode(bytes: &[u8]) -> Result<Self, EncryptedGenerationError> {
        let mut cursor = Cursor::new(bytes);
        if cursor.read_u16()? != CONTAINER_VERSION || cursor.read_u16()? != ALGORITHM_ID {
            return Err(EncryptedGenerationError::UnsupportedVersion);
        }
        let tenant_id = TenantId::parse(cursor.read_string()?)
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        let profile_id = ProfileId::parse(cursor.read_string()?)
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        let generation_id = GenerationId::parse(cursor.read_string()?)
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        let base_generation_id = match cursor.read_u8()? {
            0 => None,
            1 => Some(
                GenerationId::parse(cursor.read_string()?)
                    .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
            ),
            _ => return Err(EncryptedGenerationError::InvalidContainer),
        };
        let key_id = KeyId::parse(cursor.read_string()?)?;
        let nonce_prefix = {
            let mut prefix = [0_u8; NONCE_PREFIX_BYTES];
            prefix.copy_from_slice(cursor.take(NONCE_PREFIX_BYTES)?);
            NoncePrefix::new(prefix)
        };
        let chunk_size = cursor.read_u32()?;
        validate_chunk_size(chunk_size)?;
        let plaintext_bytes = cursor.read_u64()?;
        if plaintext_bytes > MAX_STREAM_PLAINTEXT_BYTES {
            return Err(EncryptedGenerationError::PlaintextTooLarge);
        }
        let plaintext_digest = {
            let mut digest = [0_u8; 32];
            digest.copy_from_slice(cursor.take(32)?);
            PlaintextDigest(digest)
        };
        if !cursor.is_finished() {
            return Err(EncryptedGenerationError::InvalidContainer);
        }
        Ok(Self {
            tenant_id,
            profile_id,
            generation_id,
            base_generation_id,
            key_id,
            nonce_prefix,
            chunk_size,
            plaintext_bytes,
            plaintext_digest,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectedGenerationMetadataPrelude {
    metadata: GenerationMetadata,
    metadata_digest: MetadataDigest,
    prelude_bytes: usize,
}

impl InspectedGenerationMetadataPrelude {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }

    #[must_use]
    pub const fn metadata_digest(&self) -> MetadataDigest {
        self.metadata_digest
    }

    #[must_use]
    pub const fn prelude_bytes(&self) -> usize {
        self.prelude_bytes
    }
}

pub fn inspect_generation_metadata_prelude(
    container_or_prelude: &[u8],
) -> Result<InspectedGenerationMetadataPrelude, EncryptedGenerationError> {
    if container_or_prelude.len() > MAX_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::InvalidContainer);
    }
    let mut cursor = Cursor::new(container_or_prelude);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(EncryptedGenerationError::InvalidContainer);
    }
    let metadata_length = usize::try_from(cursor.read_u32()?)
        .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
    if metadata_length == 0 || metadata_length > MAX_METADATA_BYTES {
        return Err(EncryptedGenerationError::InvalidContainer);
    }
    let metadata_bytes = cursor.take(metadata_length)?;
    let metadata = GenerationMetadata::decode(metadata_bytes)?;
    Ok(InspectedGenerationMetadataPrelude {
        metadata,
        metadata_digest: MetadataDigest::calculate(metadata_bytes),
        prelude_bytes: cursor.position,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedGeneration {
    metadata: GenerationMetadata,
    metadata_digest: MetadataDigest,
    container: Vec<u8>,
    container_digest: ContainerDigest,
}

impl SealedGeneration {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }

    #[must_use]
    pub const fn metadata_digest(&self) -> MetadataDigest {
        self.metadata_digest
    }

    #[must_use]
    pub fn container(&self) -> &[u8] {
        &self.container
    }

    #[must_use]
    pub const fn container_digest(&self) -> ContainerDigest {
        self.container_digest
    }

    #[must_use]
    pub fn into_container(self) -> Vec<u8> {
        self.container
    }
}

pub struct OpenedGeneration {
    metadata: GenerationMetadata,
    plaintext: Zeroizing<Vec<u8>>,
}

impl OpenedGeneration {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }

    pub(crate) fn into_parts(self) -> (GenerationMetadata, Zeroizing<Vec<u8>>) {
        (self.metadata, self.plaintext)
    }
}

pub fn seal_generation(
    metadata: &GenerationMetadata,
    key: &GenerationDek,
    plaintext: &[u8],
) -> Result<SealedGeneration, EncryptedGenerationError> {
    if metadata.key_id() != key.key_id() {
        return Err(EncryptedGenerationError::MetadataMismatch);
    }
    metadata.validate_plaintext(plaintext)?;
    let metadata_bytes = metadata.encode()?;
    let metadata_digest = MetadataDigest::calculate(&metadata_bytes);
    let metadata_length = u32::try_from(metadata_bytes.len())
        .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
    let chunk_size = usize::try_from(metadata.chunk_size())
        .map_err(|_| EncryptedGenerationError::InvalidChunkSize)?;
    let cipher = key.cipher()?;
    let mut container = Vec::with_capacity(
        plaintext
            .len()
            .checked_add(metadata_bytes.len())
            .and_then(|value| value.checked_add(256))
            .ok_or(EncryptedGenerationError::PlaintextTooLarge)?,
    );
    container.extend_from_slice(&MAGIC);
    push_u32(&mut container, metadata_length);
    container.extend_from_slice(&metadata_bytes);

    let mut chunk_count = 0_u64;
    for chunk in plaintext.chunks(chunk_size) {
        let plaintext_length =
            u32::try_from(chunk.len()).map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        let aad = record_aad(metadata_digest, RECORD_CHUNK, chunk_count, plaintext_length);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(chunk_count);
        let ciphertext = cipher
            .encrypt(
                &nonce_from_bytes(&nonce_bytes)?,
                Payload {
                    msg: chunk,
                    aad: &aad,
                },
            )
            .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?;
        container.push(RECORD_CHUNK);
        push_u64(&mut container, chunk_count);
        push_u32(&mut container, plaintext_length);
        push_u32(
            &mut container,
            u32::try_from(ciphertext.len())
                .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?,
        );
        container.extend_from_slice(&ciphertext);
        chunk_count = chunk_count
            .checked_add(1)
            .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
    }

    let aad = record_aad(metadata_digest, RECORD_FINAL, chunk_count, 0);
    let nonce_bytes = metadata.nonce_prefix().nonce_for(chunk_count);
    let final_tag = cipher
        .encrypt(
            &nonce_from_bytes(&nonce_bytes)?,
            Payload {
                msg: &[],
                aad: &aad,
            },
        )
        .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?;
    container.push(RECORD_FINAL);
    push_u64(&mut container, chunk_count);
    push_u32(&mut container, 0);
    push_u32(
        &mut container,
        u32::try_from(final_tag.len()).map_err(|_| EncryptedGenerationError::InvalidContainer)?,
    );
    container.extend_from_slice(&final_tag);
    if container.len() > MAX_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::PlaintextTooLarge);
    }
    let container_digest = ContainerDigest::calculate(&container);
    Ok(SealedGeneration {
        metadata: metadata.clone(),
        metadata_digest,
        container,
        container_digest,
    })
}

pub fn open_generation(
    container: &[u8],
    key: &GenerationDek,
) -> Result<OpenedGeneration, EncryptedGenerationError> {
    let inspected = inspect_generation_metadata_prelude(container)?;
    if inspected.metadata().key_id() != key.key_id() {
        return Err(EncryptedGenerationError::MetadataMismatch);
    }
    // Legacy Vec-returning entrypoint must NOT allocate 512 MiB just because
    // streamed metadata decode can now parse that amount.
    if inspected.metadata().plaintext_bytes() > MAX_PLAINTEXT_BYTES as u64 {
        return Err(EncryptedGenerationError::PlaintextTooLarge);
    }
    let InspectedGenerationMetadataPrelude {
        metadata,
        metadata_digest,
        prelude_bytes,
    } = inspected;
    let cipher = key.cipher()?;
    let mut cursor = Cursor {
        bytes: container,
        position: prelude_bytes,
    };
    let mut plaintext = Zeroizing::new(Vec::new());
    let mut expected_index = 0_u64;
    let mut final_seen = false;

    while !cursor.is_finished() {
        if final_seen {
            return Err(EncryptedGenerationError::InvalidContainer);
        }
        let record_type = cursor.read_u8()?;
        let index = cursor.read_u64()?;
        let plaintext_length = cursor.read_u32()?;
        let ciphertext_length = usize::try_from(cursor.read_u32()?)
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        if index != expected_index {
            return Err(EncryptedGenerationError::InvalidContainer);
        }
        let ciphertext = cursor.take(ciphertext_length)?;
        let aad = record_aad(metadata_digest, record_type, index, plaintext_length);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(index);
        let opened = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce_from_bytes(&nonce_bytes)?,
                    Payload {
                        msg: ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?,
        );

        match record_type {
            RECORD_CHUNK => {
                if plaintext_length == 0
                    || plaintext_length > metadata.chunk_size()
                    || ciphertext_length
                        != usize::try_from(plaintext_length)
                            .map_err(|_| EncryptedGenerationError::InvalidContainer)?
                            .checked_add(TAG_BYTES)
                            .ok_or(EncryptedGenerationError::InvalidContainer)?
                    || opened.len()
                        != usize::try_from(plaintext_length)
                            .map_err(|_| EncryptedGenerationError::InvalidContainer)?
                {
                    return Err(EncryptedGenerationError::InvalidContainer);
                }
                let new_length = plaintext
                    .len()
                    .checked_add(opened.len())
                    .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
                if new_length > MAX_PLAINTEXT_BYTES {
                    return Err(EncryptedGenerationError::PlaintextTooLarge);
                }
                plaintext.extend_from_slice(&opened);
                expected_index = expected_index
                    .checked_add(1)
                    .ok_or(EncryptedGenerationError::InvalidContainer)?;
            }
            RECORD_FINAL => {
                if plaintext_length != 0 || ciphertext_length != TAG_BYTES || !opened.is_empty() {
                    return Err(EncryptedGenerationError::InvalidContainer);
                }
                final_seen = true;
            }
            _ => return Err(EncryptedGenerationError::InvalidContainer),
        }
    }

    if !final_seen {
        return Err(EncryptedGenerationError::InvalidContainer);
    }
    let plaintext_bytes =
        u64::try_from(plaintext.len()).map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
    if plaintext_bytes != metadata.plaintext_bytes() {
        return Err(EncryptedGenerationError::DigestMismatch);
    }
    if PlaintextDigest::calculate(&plaintext) != metadata.plaintext_digest() {
        return Err(EncryptedGenerationError::DigestMismatch);
    }
    Ok(OpenedGeneration {
        metadata,
        plaintext,
    })
}

pub fn open_generation_expected(
    container: &[u8],
    key: &GenerationDek,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<OpenedGeneration, EncryptedGenerationError> {
    let opened = open_generation(container, key)?;
    if opened.metadata().tenant_id() != tenant_id
        || opened.metadata().profile_id() != profile_id
        || opened.metadata().generation_id() != generation_id
    {
        return Err(EncryptedGenerationError::IdentityMismatch);
    }
    Ok(opened)
}

/// Typed I/O boundary: a failed source/sink must never be disguised as a
/// successfully authenticated generation. Caller owns fsynced staging and
/// MUST publish only after Ok(...) and inventory validation.
#[derive(Debug)]
pub enum StreamingGenerationError {
    Io(std::io::Error),
    Crypto(EncryptedGenerationError),
}

impl std::fmt::Display for StreamingGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "encrypted-generation stream I/O: {error}"),
            Self::Crypto(error) => write!(f, "encrypted-generation stream rejected: {error}"),
        }
    }
}
impl std::error::Error for StreamingGenerationError {}
impl From<std::io::Error> for StreamingGenerationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<EncryptedGenerationError> for StreamingGenerationError {
    fn from(error: EncryptedGenerationError) -> Self {
        Self::Crypto(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingGenerationReport {
    metadata: GenerationMetadata,
    metadata_digest: MetadataDigest,
    container_digest: ContainerDigest,
    container_bytes: u64,
}

impl StreamingGenerationReport {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
    }
    #[must_use]
    pub const fn metadata_digest(&self) -> MetadataDigest {
        self.metadata_digest
    }
    #[must_use]
    pub const fn container_digest(&self) -> ContainerDigest {
        self.container_digest
    }
    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }
}

/// Exact BPGC0001 format, byte-identical to seal_generation, but only one
/// encrypted chunk and its plaintext buffer are resident at a time.
/// The sink is UNTRUSTED until this returns Ok: never publish partial output.
pub fn seal_generation_streaming(
    metadata: &GenerationMetadata,
    key: &GenerationDek,
    source: &mut impl Read,
    sink: &mut impl Write,
) -> Result<StreamingGenerationReport, StreamingGenerationError> {
    if metadata.key_id() != key.key_id() {
        return Err(EncryptedGenerationError::MetadataMismatch.into());
    }
    if metadata.plaintext_bytes() > MAX_STREAM_PLAINTEXT_BYTES {
        return Err(EncryptedGenerationError::PlaintextTooLarge.into());
    }
    let metadata_bytes = metadata.encode()?;
    let metadata_digest = MetadataDigest::calculate(&metadata_bytes);
    let cipher = key.cipher()?;
    let mut bytes = 0_u64;
    let mut hashed = Sha256::new();
    stream_write(sink, &mut hashed, &mut bytes, &MAGIC)?;
    stream_write(
        sink,
        &mut hashed,
        &mut bytes,
        &u32::try_from(metadata_bytes.len())
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?
            .to_be_bytes(),
    )?;
    stream_write(sink, &mut hashed, &mut bytes, &metadata_bytes)?;
    let chunk_size = usize::try_from(metadata.chunk_size())
        .map_err(|_| EncryptedGenerationError::InvalidChunkSize)?;
    let mut buffer = Zeroizing::new(vec![0_u8; chunk_size]);
    let mut original_hash = Sha256::new();
    let mut remaining = metadata.plaintext_bytes();
    let mut index = 0_u64;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(chunk_size as u64))
            .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        source.read_exact(&mut buffer[..count])?;
        original_hash.update(&buffer[..count]);
        let length =
            u32::try_from(count).map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        let aad = record_aad(metadata_digest, RECORD_CHUNK, index, length);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(index);
        let ciphertext = cipher
            .encrypt(
                &nonce_from_bytes(&nonce_bytes)?,
                Payload {
                    msg: &buffer[..count],
                    aad: &aad,
                },
            )
            .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?;
        stream_write(sink, &mut hashed, &mut bytes, &[RECORD_CHUNK])?;
        stream_write(sink, &mut hashed, &mut bytes, &index.to_be_bytes())?;
        stream_write(sink, &mut hashed, &mut bytes, &length.to_be_bytes())?;
        stream_write(
            sink,
            &mut hashed,
            &mut bytes,
            &u32::try_from(ciphertext.len())
                .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?
                .to_be_bytes(),
        )?;
        stream_write(sink, &mut hashed, &mut bytes, &ciphertext)?;
        remaining -= count as u64;
        index = index
            .checked_add(1)
            .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
    }
    // Refuse appended bytes, or a modified source since the first digest pass.
    let mut trailing = [0_u8; 1];
    if source.read(&mut trailing)? != 0
        || PlaintextDigest(original_hash.finalize().into()) != metadata.plaintext_digest()
    {
        return Err(EncryptedGenerationError::MetadataMismatch.into());
    }
    let aad = record_aad(metadata_digest, RECORD_FINAL, index, 0);
    let nonce_bytes = metadata.nonce_prefix().nonce_for(index);
    let final_tag = cipher
        .encrypt(
            &nonce_from_bytes(&nonce_bytes)?,
            Payload {
                msg: &[],
                aad: &aad,
            },
        )
        .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?;
    stream_write(sink, &mut hashed, &mut bytes, &[RECORD_FINAL])?;
    stream_write(sink, &mut hashed, &mut bytes, &index.to_be_bytes())?;
    stream_write(sink, &mut hashed, &mut bytes, &0_u32.to_be_bytes())?;
    stream_write(
        sink,
        &mut hashed,
        &mut bytes,
        &u32::try_from(final_tag.len())
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?
            .to_be_bytes(),
    )?;
    stream_write(sink, &mut hashed, &mut bytes, &final_tag)?;
    Ok(StreamingGenerationReport {
        metadata: metadata.clone(),
        metadata_digest,
        container_digest: ContainerDigest(hashed.finalize().into()),
        container_bytes: bytes,
    })
}

/// Decrypt into a caller-owned *unpublished* staging sink. AEAD authentication
/// happens per chunk, but whole-generation validity is known ONLY at the
/// authenticated final tag, total length, full digest and strict EOF.
pub fn open_generation_streaming(
    source: &mut impl Read,
    key: &GenerationDek,
    sink: &mut impl Write,
) -> Result<StreamingGenerationReport, StreamingGenerationError> {
    let mut bytes = 0_u64;
    let mut hashed = Sha256::new();
    let mut prefix = [0_u8; 12];
    stream_read(source, &mut hashed, &mut bytes, &mut prefix)?;
    if prefix[..8] != MAGIC {
        return Err(EncryptedGenerationError::InvalidContainer.into());
    }
    let meta_len = u32::from_be_bytes(
        prefix[8..12]
            .try_into()
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
    ) as usize;
    if meta_len == 0 || meta_len > MAX_METADATA_BYTES {
        return Err(EncryptedGenerationError::InvalidContainer.into());
    }
    let mut meta = vec![0_u8; meta_len];
    stream_read(source, &mut hashed, &mut bytes, &mut meta)?;
    let metadata = GenerationMetadata::decode(&meta)?;
    if metadata.key_id() != key.key_id() {
        return Err(EncryptedGenerationError::MetadataMismatch.into());
    }
    let metadata_digest = MetadataDigest::calculate(&meta);
    let cipher = key.cipher()?;
    let mut plain_hash = Sha256::new();
    let mut plain_bytes = 0_u64;
    let mut index = 0_u64;
    loop {
        let mut header = [0_u8; 17];
        stream_read(source, &mut hashed, &mut bytes, &mut header)?;
        let kind = header[0];
        let record_index = u64::from_be_bytes(
            header[1..9]
                .try_into()
                .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
        );
        let plain_len = u32::from_be_bytes(
            header[9..13]
                .try_into()
                .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
        );
        let cipher_len = u32::from_be_bytes(
            header[13..17]
                .try_into()
                .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
        ) as usize;
        if record_index != index {
            return Err(EncryptedGenerationError::InvalidContainer.into());
        }
        if !matches!(kind, RECORD_CHUNK | RECORD_FINAL)
            || (kind == RECORD_CHUNK
                && (plain_len == 0
                    || plain_len > metadata.chunk_size()
                    || cipher_len != plain_len as usize + TAG_BYTES))
            || (kind == RECORD_FINAL && (plain_len != 0 || cipher_len != TAG_BYTES))
        {
            return Err(EncryptedGenerationError::InvalidContainer.into());
        }
        let mut ciphertext = vec![0_u8; cipher_len]; // at most 1 MiB + AEAD tag
        stream_read(source, &mut hashed, &mut bytes, &mut ciphertext)?;
        let aad = record_aad(metadata_digest, kind, index, plain_len);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(index);
        let opened = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce_from_bytes(&nonce_bytes)?,
                    Payload {
                        msg: &ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?,
        );
        if kind == RECORD_FINAL {
            if !opened.is_empty()
                || plain_bytes != metadata.plaintext_bytes()
                || PlaintextDigest(plain_hash.finalize().into()) != metadata.plaintext_digest()
            {
                return Err(EncryptedGenerationError::DigestMismatch.into());
            }
            let mut trailing = [0_u8; 1];
            if source.read(&mut trailing)? != 0 {
                return Err(EncryptedGenerationError::InvalidContainer.into());
            }
            return Ok(StreamingGenerationReport {
                metadata,
                metadata_digest,
                container_digest: ContainerDigest(hashed.finalize().into()),
                container_bytes: bytes,
            });
        }
        if opened.len() != plain_len as usize {
            return Err(EncryptedGenerationError::InvalidContainer.into());
        }
        plain_bytes = plain_bytes
            .checked_add(plain_len as u64)
            .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
        if plain_bytes > metadata.plaintext_bytes() || plain_bytes > MAX_STREAM_PLAINTEXT_BYTES {
            return Err(EncryptedGenerationError::PlaintextTooLarge.into());
        }
        plain_hash.update(&opened);
        sink.write_all(&opened)?;
        index = index
            .checked_add(1)
            .ok_or(EncryptedGenerationError::InvalidContainer)?;
    }
}

fn stream_write(
    sink: &mut impl Write,
    hasher: &mut Sha256,
    bytes: &mut u64,
    data: &[u8],
) -> Result<(), StreamingGenerationError> {
    let next = bytes
        .checked_add(data.len() as u64)
        .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
    if next > MAX_STREAM_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::PlaintextTooLarge.into());
    }
    sink.write_all(data)?;
    hasher.update(data);
    *bytes = next;
    Ok(())
}

fn stream_read(
    source: &mut impl Read,
    hasher: &mut Sha256,
    bytes: &mut u64,
    data: &mut [u8],
) -> Result<(), StreamingGenerationError> {
    let next = bytes
        .checked_add(data.len() as u64)
        .ok_or(EncryptedGenerationError::PlaintextTooLarge)?;
    if next > MAX_STREAM_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::PlaintextTooLarge.into());
    }
    source.read_exact(data)?;
    hasher.update(data);
    *bytes = next;
    Ok(())
}

fn nonce_from_bytes(bytes: &[u8; NONCE_BYTES]) -> Result<XNonce, EncryptedGenerationError> {
    XNonce::try_from(bytes.as_slice()).map_err(|_| EncryptedGenerationError::InvalidContainer)
}

fn validate_chunk_size(chunk_size: u32) -> Result<(), EncryptedGenerationError> {
    if !(MIN_CHUNK_BYTES..=MAX_CHUNK_BYTES).contains(&chunk_size) {
        return Err(EncryptedGenerationError::InvalidChunkSize);
    }
    Ok(())
}

fn record_aad(
    metadata_digest: MetadataDigest,
    record_type: u8,
    index: u64,
    plaintext_length: u32,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(45);
    aad.extend_from_slice(&metadata_digest.bytes());
    aad.push(record_type);
    aad.extend_from_slice(&index.to_be_bytes());
    aad.extend_from_slice(&plaintext_length.to_be_bytes());
    aad
}

fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), EncryptedGenerationError> {
    let length =
        u16::try_from(value.len()).map_err(|_| EncryptedGenerationError::InvalidContainer)?;
    push_u16(output, length);
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], EncryptedGenerationError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(EncryptedGenerationError::InvalidContainer)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(EncryptedGenerationError::InvalidContainer)?;
        self.position = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, EncryptedGenerationError> {
        self.take(1)?
            .first()
            .copied()
            .ok_or(EncryptedGenerationError::InvalidContainer)
    }

    fn read_u16(&mut self) -> Result<u16, EncryptedGenerationError> {
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(bytes))
    }

    fn read_u32(&mut self) -> Result<u32, EncryptedGenerationError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, EncryptedGenerationError> {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(bytes))
    }

    fn read_string(&mut self) -> Result<String, EncryptedGenerationError> {
        let length = usize::from(self.read_u16()?);
        let bytes = self.take(length)?;
        let value =
            core::str::from_utf8(bytes).map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        Ok(value.to_owned())
    }

    const fn is_finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}
