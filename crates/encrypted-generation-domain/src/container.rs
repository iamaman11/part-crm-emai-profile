use crate::{ALGORITHM_SUITE, CONTAINER_VERSION, EncryptedGenerationError};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

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
const MAX_CONTAINER_BYTES: usize = 83_886_080;
const MAX_METADATA_BYTES: usize = 4_096;
const MIN_KEY_ID_BYTES: usize = 8;
const MAX_KEY_ID_BYTES: usize = 96;

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
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        base_generation_id: Option<GenerationId>,
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
        if self.plaintext_bytes
            > u64::try_from(MAX_PLAINTEXT_BYTES)
                .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?
        {
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
        if plaintext_bytes
            > u64::try_from(MAX_PLAINTEXT_BYTES)
                .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?
        {
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
pub struct SealedGeneration {
    metadata: GenerationMetadata,
    container: Vec<u8>,
    container_digest: ContainerDigest,
}

impl SealedGeneration {
    #[must_use]
    pub const fn metadata(&self) -> &GenerationMetadata {
        &self.metadata
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenedGeneration {
    metadata: GenerationMetadata,
    plaintext: Vec<u8>,
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

    #[must_use]
    pub fn into_plaintext(self) -> Vec<u8> {
        self.plaintext
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
    let metadata_digest: [u8; 32] = Sha256::digest(&metadata_bytes).into();
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
        let plaintext_length = u32::try_from(chunk.len())
            .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
        let aad = record_aad(&metadata_digest, RECORD_CHUNK, chunk_count, plaintext_length);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(chunk_count);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce_bytes),
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

    let aad = record_aad(&metadata_digest, RECORD_FINAL, chunk_count, 0);
    let nonce_bytes = metadata.nonce_prefix().nonce_for(chunk_count);
    let final_tag = cipher
        .encrypt(
            XNonce::from_slice(&nonce_bytes),
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
        u32::try_from(final_tag.len())
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?,
    );
    container.extend_from_slice(&final_tag);
    if container.len() > MAX_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::PlaintextTooLarge);
    }
    let container_digest = ContainerDigest::calculate(&container);
    Ok(SealedGeneration {
        metadata: metadata.clone(),
        container,
        container_digest,
    })
}

pub fn open_generation(
    container: &[u8],
    key: &GenerationDek,
) -> Result<OpenedGeneration, EncryptedGenerationError> {
    if container.len() > MAX_CONTAINER_BYTES {
        return Err(EncryptedGenerationError::InvalidContainer);
    }
    let mut cursor = Cursor::new(container);
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
    if metadata.key_id() != key.key_id() {
        return Err(EncryptedGenerationError::MetadataMismatch);
    }
    let metadata_digest: [u8; 32] = Sha256::digest(metadata_bytes).into();
    let cipher = key.cipher()?;
    let mut plaintext = Vec::with_capacity(
        usize::try_from(metadata.plaintext_bytes())
            .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?,
    );
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
        let aad = record_aad(&metadata_digest, record_type, index, plaintext_length);
        let nonce_bytes = metadata.nonce_prefix().nonce_for(index);
        let opened = cipher
            .decrypt(
                XNonce::from_slice(&nonce_bytes),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| EncryptedGenerationError::AuthenticationFailed)?;

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
    let plaintext_bytes = u64::try_from(plaintext.len())
        .map_err(|_| EncryptedGenerationError::PlaintextTooLarge)?;
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

fn validate_chunk_size(chunk_size: u32) -> Result<(), EncryptedGenerationError> {
    if !(MIN_CHUNK_BYTES..=MAX_CHUNK_BYTES).contains(&chunk_size) {
        return Err(EncryptedGenerationError::InvalidChunkSize);
    }
    Ok(())
}

fn record_aad(
    metadata_digest: &[u8; 32],
    record_type: u8,
    index: u64,
    plaintext_length: u32,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(45);
    aad.extend_from_slice(metadata_digest);
    aad.push(record_type);
    aad.extend_from_slice(&index.to_be_bytes());
    aad.extend_from_slice(&plaintext_length.to_be_bytes());
    aad
}

fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), EncryptedGenerationError> {
    let length = u16::try_from(value.len()).map_err(|_| EncryptedGenerationError::InvalidContainer)?;
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
        let value = core::str::from_utf8(bytes)
            .map_err(|_| EncryptedGenerationError::InvalidContainer)?;
        Ok(value.to_owned())
    }

    const fn is_finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}

#[must_use]
pub const fn algorithm_suite() -> &'static str {
    ALGORITHM_SUITE
}
