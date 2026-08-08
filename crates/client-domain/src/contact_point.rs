use core::fmt;
use profile_platform_primitives::{ContactPointId, TenantId};
use zeroize::Zeroize;

const MAX_CONTACT_VALUE_LENGTH: usize = 2048;
const MAX_CIPHERTEXT_LENGTH: usize = 4096;
const MAX_NONCE_LENGTH: usize = 64;
const EXACT_LOOKUP_TOKEN_LENGTH: usize = 32;
const LOOKUP_DOMAIN_TAG: &[u8] = b"client-contact-exact-lookup\0v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactKind {
    Email,
    Phone,
    Url,
}

impl ContactKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Url => "URL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactStatus {
    Active,
    Archived,
}

impl ContactStatus {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Archived => "ARCHIVED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContactNormalizationVersion {
    V1,
}

impl ContactNormalizationVersion {
    #[must_use]
    pub const fn value(self) -> u16 {
        match self {
            Self::V1 => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContactProtectionVersion {
    V1,
}

impl ContactProtectionVersion {
    #[must_use]
    pub const fn value(self) -> u16 {
        match self {
            Self::V1 => 1,
        }
    }
}

#[derive(Eq, PartialEq)]
pub struct NormalizedContactValue(String);

impl NormalizedContactValue {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NormalizedContactValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NormalizedContactValue([REDACTED])")
    }
}

impl Drop for NormalizedContactValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub fn normalize_contact_value(
    kind: ContactKind,
    version: ContactNormalizationVersion,
    raw: &str,
) -> Result<NormalizedContactValue, ContactValueError> {
    match version {
        ContactNormalizationVersion::V1 => normalize_v1(kind, raw).map(NormalizedContactValue),
    }
}

fn normalize_v1(kind: ContactKind, raw: &str) -> Result<String, ContactValueError> {
    let value = raw.trim();
    if value.is_empty() || value.len() > MAX_CONTACT_VALUE_LENGTH {
        return Err(ContactValueError::InvalidValue);
    }
    match kind {
        ContactKind::Email => normalize_email_v1(value),
        ContactKind::Phone => normalize_phone_v1(value),
        ContactKind::Url => normalize_url_v1(value),
    }
}

fn normalize_email_v1(value: &str) -> Result<String, ContactValueError> {
    if value.len() > 320 || value.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace()) {
        return Err(ContactValueError::InvalidValue);
    }
    let mut parts = value.split('@');
    let local = parts.next().ok_or(ContactValueError::InvalidValue)?;
    let domain = parts.next().ok_or(ContactValueError::InvalidValue)?;
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(ContactValueError::InvalidValue);
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_phone_v1(value: &str) -> Result<String, ContactValueError> {
    let mut normalized = String::with_capacity(value.len());
    let mut digit_count = 0_usize;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'+' if index == 0 => normalized.push('+'),
            b'0'..=b'9' => {
                normalized.push(char::from(byte));
                digit_count += 1;
            }
            b' ' | b'-' | b'(' | b')' | b'.' => {}
            _ => return Err(ContactValueError::InvalidValue),
        }
    }
    if !(7..=15).contains(&digit_count) {
        return Err(ContactValueError::InvalidValue);
    }
    Ok(normalized)
}

fn normalize_url_v1(value: &str) -> Result<String, ContactValueError> {
    if value.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace()) {
        return Err(ContactValueError::InvalidValue);
    }
    let bytes = value.as_bytes();
    let (scheme, offset) = if bytes.len() >= 7 && bytes[..7].eq_ignore_ascii_case(b"http://") {
        ("http://", 7_usize)
    } else if bytes.len() >= 8 && bytes[..8].eq_ignore_ascii_case(b"https://") {
        ("https://", 8_usize)
    } else {
        return Err(ContactValueError::InvalidValue);
    };
    let remainder = value.get(offset..).ok_or(ContactValueError::InvalidValue)?;
    if remainder.is_empty() {
        return Err(ContactValueError::InvalidValue);
    }
    let mut normalized = String::with_capacity(value.len());
    normalized.push_str(scheme);
    normalized.push_str(remainder);
    Ok(normalized)
}

#[derive(Eq, PartialEq)]
pub struct ExactLookupHmacInput(Vec<u8>);

impl ExactLookupHmacInput {
    #[must_use]
    pub fn expose_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for ExactLookupHmacInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExactLookupHmacInput([REDACTED])")
    }
}

impl Drop for ExactLookupHmacInput {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[must_use]
pub fn exact_lookup_hmac_input(
    tenant_id: &TenantId,
    kind: ContactKind,
    normalization_version: ContactNormalizationVersion,
    normalized: &NormalizedContactValue,
) -> ExactLookupHmacInput {
    let tenant = tenant_id.as_str().as_bytes();
    let kind = kind.stable_code().as_bytes();
    let version = normalization_version.value().to_be_bytes();
    let value = normalized.expose().as_bytes();
    let mut input = Vec::with_capacity(
        LOOKUP_DOMAIN_TAG.len() + tenant.len() + kind.len() + value.len() + 12,
    );
    input.extend_from_slice(LOOKUP_DOMAIN_TAG);
    input.extend_from_slice(&version);
    input.push(0);
    input.extend_from_slice(tenant);
    input.push(0);
    input.extend_from_slice(kind);
    input.push(0);
    input.extend_from_slice(value);
    ExactLookupHmacInput(input)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EncryptionKeyVersion(u32);

impl EncryptionKeyVersion {
    pub const fn new(value: u32) -> Result<Self, ContactProtectionError> {
        if value == 0 {
            Err(ContactProtectionError::InvalidKeyVersion)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LookupKeyVersion(u32);

impl LookupKeyVersion {
    pub const fn new(value: u32) -> Result<Self, ContactProtectionError> {
        if value == 0 {
            Err(ContactProtectionError::InvalidKeyVersion)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedContactValue {
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    key_version: EncryptionKeyVersion,
}

impl EncryptedContactValue {
    pub fn new(
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
        key_version: EncryptionKeyVersion,
    ) -> Result<Self, ContactProtectionError> {
        if ciphertext.is_empty()
            || ciphertext.len() > MAX_CIPHERTEXT_LENGTH
            || nonce.is_empty()
            || nonce.len() > MAX_NONCE_LENGTH
        {
            return Err(ContactProtectionError::InvalidProtectedValue);
        }
        Ok(Self {
            ciphertext,
            nonce,
            key_version,
        })
    }

    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    #[must_use]
    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    #[must_use]
    pub const fn key_version(&self) -> EncryptionKeyVersion {
        self.key_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactLookupToken {
    bytes: [u8; EXACT_LOOKUP_TOKEN_LENGTH],
    key_version: LookupKeyVersion,
}

impl ExactLookupToken {
    #[must_use]
    pub const fn new(
        bytes: [u8; EXACT_LOOKUP_TOKEN_LENGTH],
        key_version: LookupKeyVersion,
    ) -> Self {
        Self { bytes, key_version }
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; EXACT_LOOKUP_TOKEN_LENGTH] {
        &self.bytes
    }

    #[must_use]
    pub const fn key_version(&self) -> LookupKeyVersion {
        self.key_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedContactPoint {
    contact_point_id: ContactPointId,
    kind: ContactKind,
    status: ContactStatus,
    normalization_version: ContactNormalizationVersion,
    protection_version: ContactProtectionVersion,
    display_value: EncryptedContactValue,
    exact_lookup: ExactLookupToken,
}

impl ProtectedContactPoint {
    #[must_use]
    pub const fn new(
        contact_point_id: ContactPointId,
        kind: ContactKind,
        status: ContactStatus,
        normalization_version: ContactNormalizationVersion,
        protection_version: ContactProtectionVersion,
        display_value: EncryptedContactValue,
        exact_lookup: ExactLookupToken,
    ) -> Self {
        Self {
            contact_point_id,
            kind,
            status,
            normalization_version,
            protection_version,
            display_value,
            exact_lookup,
        }
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        &self.contact_point_id
    }

    #[must_use]
    pub const fn kind(&self) -> ContactKind {
        self.kind
    }

    #[must_use]
    pub const fn status(&self) -> ContactStatus {
        self.status
    }

    #[must_use]
    pub const fn normalization_version(&self) -> ContactNormalizationVersion {
        self.normalization_version
    }

    #[must_use]
    pub const fn protection_version(&self) -> ContactProtectionVersion {
        self.protection_version
    }

    #[must_use]
    pub const fn display_value(&self) -> &EncryptedContactValue {
        &self.display_value
    }

    #[must_use]
    pub const fn exact_lookup(&self) -> &ExactLookupToken {
        &self.exact_lookup
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactValueError {
    InvalidValue,
}

impl fmt::Display for ContactValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("contact value is invalid for the selected normalization contract")
    }
}

impl std::error::Error for ContactValueError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactProtectionError {
    InvalidKeyVersion,
    InvalidProtectedValue,
}

impl fmt::Display for ContactProtectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidKeyVersion => "contact protection key version must be greater than zero",
            Self::InvalidProtectedValue => "protected contact value is invalid",
        })
    }
}

impl std::error::Error for ContactProtectionError {}

#[cfg(test)]
mod tests {
    use super::{
        ContactKind, ContactNormalizationVersion, ContactProtectionVersion, ContactStatus,
        EncryptedContactValue, EncryptionKeyVersion, ExactLookupToken, LookupKeyVersion,
        ProtectedContactPoint, exact_lookup_hmac_input, normalize_contact_value,
    };
    use profile_platform_primitives::{ContactPointId, TenantId};

    #[test]
    fn v1_normalization_vectors_are_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let email = normalize_contact_value(
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            "  PERSON@Example.COM  ",
        )?;
        let phone = normalize_contact_value(
            ContactKind::Phone,
            ContactNormalizationVersion::V1,
            " +48 (123) 456-789 ",
        )?;
        let url = normalize_contact_value(
            ContactKind::Url,
            ContactNormalizationVersion::V1,
            " HTTPS://Example.COM/A?B=C ",
        )?;
        assert_eq!(email.expose(), "person@example.com");
        assert_eq!(phone.expose(), "+48123456789");
        assert_eq!(url.expose(), "https://Example.COM/A?B=C");
        Ok(())
    }

    #[test]
    fn exact_lookup_input_is_domain_and_tenant_separated() -> Result<(), Box<dyn std::error::Error>> {
        let normalized = normalize_contact_value(
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            "person@example.com",
        )?;
        let tenant_a = TenantId::parse("tenant_01JLOOKUPA")?;
        let tenant_b = TenantId::parse("tenant_01JLOOKUPB")?;
        let input_a = exact_lookup_hmac_input(
            &tenant_a,
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            &normalized,
        );
        let input_b = exact_lookup_hmac_input(
            &tenant_b,
            ContactKind::Email,
            ContactNormalizationVersion::V1,
            &normalized,
        );
        assert_ne!(input_a.expose_bytes(), input_b.expose_bytes());
        assert!(input_a.expose_bytes().starts_with(b"client-contact-exact-lookup\0v1\0"));
        Ok(())
    }

    #[test]
    fn protected_representation_has_no_plaintext_field() -> Result<(), Box<dyn std::error::Error>> {
        let encrypted = EncryptedContactValue::new(
            vec![1, 2, 3, 4],
            vec![9, 8, 7],
            EncryptionKeyVersion::new(1)?,
        )?;
        let lookup = ExactLookupToken::new([5_u8; 32], LookupKeyVersion::new(2)?);
        let protected = ProtectedContactPoint::new(
            ContactPointId::parse("contact_01JPROTECTED")?,
            ContactKind::Email,
            ContactStatus::Active,
            ContactNormalizationVersion::V1,
            ContactProtectionVersion::V1,
            encrypted,
            lookup,
        );
        assert_eq!(protected.kind().stable_code(), "EMAIL");
        assert_eq!(protected.status().stable_code(), "ACTIVE");
        assert_eq!(protected.normalization_version().value(), 1);
        assert_eq!(protected.protection_version().value(), 1);
        assert_eq!(protected.display_value().key_version().value(), 1);
        assert_eq!(protected.exact_lookup().key_version().value(), 2);
        Ok(())
    }
}
