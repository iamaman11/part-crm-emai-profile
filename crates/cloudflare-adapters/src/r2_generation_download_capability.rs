use application_ports::generation_objects::GenerationObjectDescriptor;
use core::fmt;
use profile_platform_primitives::TenantScope;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const REGION: &str = "auto";
const SERVICE: &str = "s3";
const TERMINATOR: &str = "aws4_request";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
const MAX_EXPIRES_SECONDS: u32 = 300;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2GenerationDownloadSigningTime {
    amz_date: String,
    date_stamp: String,
}

impl R2GenerationDownloadSigningTime {
    pub fn parse(amz_date: impl Into<String>) -> Result<Self, R2GenerationDownloadCapabilityError> {
        let amz_date = amz_date.into();
        let bytes = amz_date.as_bytes();
        if bytes.len() != 16
            || bytes[8] != b'T'
            || bytes[15] != b'Z'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| index != 8 && index != 15 && !byte.is_ascii_digit())
        {
            return Err(R2GenerationDownloadCapabilityError::InvalidSigningTime);
        }
        Ok(Self {
            date_stamp: amz_date[..8].to_owned(),
            amz_date,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2GenerationDownloadCapability {
    url: String,
    expires_seconds: u32,
}

impl R2GenerationDownloadCapability {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct R2GenerationDownloadCapabilitySigner {
    account_id: String,
    bucket_name: String,
    access_key_id: String,
    secret_access_key: String,
}

impl R2GenerationDownloadCapabilitySigner {
    pub fn new(
        account_id: impl Into<String>,
        bucket_name: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Result<Self, R2GenerationDownloadCapabilityError> {
        let account_id = account_id.into();
        let bucket_name = bucket_name.into();
        let access_key_id = access_key_id.into();
        let secret_access_key = secret_access_key.into();
        if account_id.len() != 32 || !account_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(R2GenerationDownloadCapabilityError::InvalidAccountId);
        }
        if !valid_bucket_name(&bucket_name) {
            return Err(R2GenerationDownloadCapabilityError::InvalidBucketName);
        }
        if access_key_id.is_empty()
            || access_key_id.len() > 256
            || secret_access_key.is_empty()
            || secret_access_key.len() > 1024
        {
            return Err(R2GenerationDownloadCapabilityError::InvalidCredentials);
        }
        Ok(Self {
            account_id: account_id.to_ascii_lowercase(),
            bucket_name,
            access_key_id,
            secret_access_key,
        })
    }

    pub fn sign_get(
        &self,
        scope: &TenantScope,
        descriptor: &GenerationObjectDescriptor,
        signing_time: &R2GenerationDownloadSigningTime,
        expires_seconds: u32,
    ) -> Result<R2GenerationDownloadCapability, R2GenerationDownloadCapabilityError> {
        if expires_seconds == 0 || expires_seconds > MAX_EXPIRES_SECONDS {
            return Err(R2GenerationDownloadCapabilityError::InvalidExpiry);
        }
        validate_descriptor(scope, descriptor)?;

        let host = format!("{}.r2.cloudflarestorage.com", self.account_id);
        let canonical_headers = format!("host:{host}\n");
        let signed_headers = "host";
        let credential_scope = format!(
            "{}/{}/{}/{}",
            signing_time.date_stamp, REGION, SERVICE, TERMINATOR
        );
        let credential = format!("{}/{}", self.access_key_id, credential_scope);
        let canonical_query = format!(
            "X-Amz-Algorithm={}&X-Amz-Credential={}&X-Amz-Date={}&X-Amz-Expires={}&X-Amz-SignedHeaders={}",
            uri_encode(ALGORITHM, false),
            uri_encode(&credential, false),
            uri_encode(&signing_time.amz_date, false),
            expires_seconds,
            uri_encode(signed_headers, false),
        );
        let canonical_uri = format!(
            "/{}/{}",
            uri_encode(&self.bucket_name, false),
            uri_encode(descriptor.object_key(), true)
        );
        let canonical_request = format!(
            "GET\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{UNSIGNED_PAYLOAD}"
        );
        let canonical_request_digest = hex_lower(&Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "{ALGORITHM}\n{}\n{credential_scope}\n{canonical_request_digest}",
            signing_time.amz_date
        );
        let signing_key = signing_key(self.secret_access_key.as_bytes(), &signing_time.date_stamp);
        let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let url =
            format!("https://{host}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}");

        Ok(R2GenerationDownloadCapability {
            url,
            expires_seconds,
        })
    }
}

impl fmt::Debug for R2GenerationDownloadCapabilitySigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("R2GenerationDownloadCapabilitySigner")
            .field("account_id", &self.account_id)
            .field("bucket_name", &self.bucket_name)
            .field("access_key_id", &"[REDACTED]")
            .field("secret_access_key", &"[REDACTED]")
            .finish()
    }
}

impl Drop for R2GenerationDownloadCapabilitySigner {
    fn drop(&mut self) {
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R2GenerationDownloadCapabilityError {
    InvalidAccountId,
    InvalidBucketName,
    InvalidCredentials,
    InvalidSigningTime,
    InvalidExpiry,
    InvalidDescriptor,
}

impl fmt::Display for R2GenerationDownloadCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAccountId => "R2 account identifier is invalid",
            Self::InvalidBucketName => "R2 bucket name is invalid",
            Self::InvalidCredentials => "R2 signing credentials are invalid",
            Self::InvalidSigningTime => "R2 signing time is invalid",
            Self::InvalidExpiry => "R2 download capability expiry is invalid",
            Self::InvalidDescriptor => "R2 generation descriptor is invalid",
        })
    }
}

impl std::error::Error for R2GenerationDownloadCapabilityError {}

fn validate_descriptor(
    scope: &TenantScope,
    descriptor: &GenerationObjectDescriptor,
) -> Result<(), R2GenerationDownloadCapabilityError> {
    let canonical_key = format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        scope.tenant_id().as_str(),
        descriptor.profile_id().as_str(),
        descriptor.generation_id().as_str(),
    );
    if descriptor.object_key() != canonical_key
        || descriptor.container_bytes() == 0
        || !is_lower_hex_sha256(descriptor.metadata_digest())
        || !is_lower_hex_sha256(descriptor.container_digest())
    {
        return Err(R2GenerationDownloadCapabilityError::InvalidDescriptor);
    }
    Ok(())
}

fn valid_bucket_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn uri_encode(value: &str, preserve_slash: bool) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (preserve_slash && byte == b'/')
        {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(hex_upper(byte >> 4));
            output.push(hex_upper(byte & 0x0f));
        }
    }
    output
}

fn signing_key(secret: &[u8], date_stamp: &str) -> [u8; 32] {
    let mut prefixed = Vec::with_capacity(secret.len() + 4);
    prefixed.extend_from_slice(b"AWS4");
    prefixed.extend_from_slice(secret);
    let date = hmac_sha256(&prefixed, date_stamp.as_bytes());
    let region = hmac_sha256(&date, REGION.as_bytes());
    let service = hmac_sha256(&region, SERVICE.as_bytes());
    hmac_sha256(&service, TERMINATOR.as_bytes())
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_BYTES: usize = 64;
    let mut normalized = [0_u8; BLOCK_BYTES];
    if key.len() > BLOCK_BYTES {
        let digest = Sha256::digest(key);
        normalized[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; BLOCK_BYTES];
    for index in 0..BLOCK_BYTES {
        inner_pad[index] ^= normalized[index];
        outer_pad[index] ^= normalized[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(hex_lower_digit(byte >> 4));
        output.push(hex_lower_digit(byte & 0x0f));
    }
    output
}

const fn hex_upper(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

const fn hex_lower_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + value - 10) as char,
    }
}

#[cfg(test)]
mod tests {
    use super::{R2GenerationDownloadCapabilitySigner, R2GenerationDownloadSigningTime};
    use application_ports::generation_objects::GenerationObjectDescriptor;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId, TenantScope};

    fn descriptor() -> Result<GenerationObjectDescriptor, Box<dyn std::error::Error>> {
        Ok(GenerationObjectDescriptor::new(
            ProfileId::parse("profile_download_capability_01")?,
            GenerationId::parse("generation_download_capability_01")?,
            "tenants/tenant_download_capability_01/profiles/profile_download_capability_01/generations/generation_download_capability_01.bpgc",
            "d".repeat(64),
            "e".repeat(64),
            4096,
        ))
    }

    fn signer() -> Result<R2GenerationDownloadCapabilitySigner, Box<dyn std::error::Error>> {
        Ok(R2GenerationDownloadCapabilitySigner::new(
            "0123456789abcdef0123456789abcdef",
            "profile-generations",
            "ACCESSKEYEXAMPLE",
            "secret-example-key",
        )?)
    }

    #[test]
    fn debug_redacts_download_signing_credentials() -> Result<(), Box<dyn std::error::Error>> {
        let signer = R2GenerationDownloadCapabilitySigner::new(
            "0123456789abcdef0123456789abcdef",
            "profile-generations",
            "DOWNLOAD_ACCESS_SENTINEL",
            "DOWNLOAD_SECRET_SENTINEL",
        )?;
        let debug = format!("{signer:?}");
        assert!(!debug.contains("DOWNLOAD_ACCESS_SENTINEL"));
        assert!(!debug.contains("DOWNLOAD_SECRET_SENTINEL"));
        assert!(debug.matches("[REDACTED]").count() >= 2);
        Ok(())
    }

    #[test]
    fn exact_get_capability_is_short_lived_and_object_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        let scope = TenantScope::new(TenantId::parse("tenant_download_capability_01")?);
        let signing_time = R2GenerationDownloadSigningTime::parse("20260829T120000Z")?;
        let capability = signer()?.sign_get(&scope, &descriptor()?, &signing_time, 300)?;
        assert!(capability.url().contains("/generation_download_capability_01.bpgc?"));
        assert!(capability.url().contains("X-Amz-Expires=300"));
        assert!(capability.url().contains("X-Amz-SignedHeaders=host"));
        assert!(capability.url().contains("X-Amz-Signature="));
        assert_eq!(capability.expires_seconds(), 300);
        Ok(())
    }

    #[test]
    fn signer_rejects_noncanonical_or_unbounded_descriptor()
    -> Result<(), Box<dyn std::error::Error>> {
        let scope = TenantScope::new(TenantId::parse("tenant_download_capability_01")?);
        let signing_time = R2GenerationDownloadSigningTime::parse("20260829T120000Z")?;
        let invalid = GenerationObjectDescriptor::new(
            ProfileId::parse("profile_download_capability_01")?,
            GenerationId::parse("generation_download_capability_01")?,
            "tenants/other/profiles/profile_download_capability_01/generations/generation_download_capability_01.bpgc",
            "d".repeat(64),
            "e".repeat(64),
            4096,
        );
        assert!(signer()?.sign_get(&scope, &invalid, &signing_time, 300).is_err());
        assert!(signer()?.sign_get(&scope, &descriptor()?, &signing_time, 301).is_err());
        Ok(())
    }
}
