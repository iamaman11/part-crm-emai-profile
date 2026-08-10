use application_ports::generation_objects::GenerationObjectDescriptor;
use core::fmt;
use profile_platform_primitives::TenantScope;
use sha2::{Digest, Sha256};

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const REGION: &str = "auto";
const SERVICE: &str = "s3";
const TERMINATOR: &str = "aws4_request";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
const MAX_EXPIRES_SECONDS: u32 = 300;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2SigV4Credentials {
    access_key_id: String,
    secret_access_key: String,
}

impl R2SigV4Credentials {
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Result<Self, R2GenerationUploadCapabilityError> {
        let access_key_id = access_key_id.into();
        let secret_access_key = secret_access_key.into();
        if access_key_id.is_empty()
            || access_key_id.len() > 256
            || secret_access_key.is_empty()
            || secret_access_key.len() > 1024
        {
            return Err(R2GenerationUploadCapabilityError::InvalidCredentials);
        }
        Ok(Self {
            access_key_id,
            secret_access_key,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2GenerationUploadSigningTime {
    amz_date: String,
    date_stamp: String,
}

impl R2GenerationUploadSigningTime {
    pub fn parse(amz_date: impl Into<String>) -> Result<Self, R2GenerationUploadCapabilityError> {
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
            return Err(R2GenerationUploadCapabilityError::InvalidSigningTime);
        }
        Ok(Self {
            date_stamp: amz_date[..8].to_owned(),
            amz_date,
        })
    }

    #[must_use]
    pub fn amz_date(&self) -> &str {
        &self.amz_date
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2GenerationUploadCapability {
    url: String,
    headers: Vec<(String, String)>,
    expires_seconds: u32,
}

impl R2GenerationUploadCapability {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    #[must_use]
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2GenerationUploadCapabilitySigner {
    account_id: String,
    bucket_name: String,
    credentials: R2SigV4Credentials,
}

impl R2GenerationUploadCapabilitySigner {
    pub fn new(
        account_id: impl Into<String>,
        bucket_name: impl Into<String>,
        credentials: R2SigV4Credentials,
    ) -> Result<Self, R2GenerationUploadCapabilityError> {
        let account_id = account_id.into();
        let bucket_name = bucket_name.into();
        if account_id.len() != 32 || !account_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(R2GenerationUploadCapabilityError::InvalidAccountId);
        }
        if !valid_bucket_name(&bucket_name) {
            return Err(R2GenerationUploadCapabilityError::InvalidBucketName);
        }
        Ok(Self {
            account_id: account_id.to_ascii_lowercase(),
            bucket_name,
            credentials,
        })
    }

    pub fn sign_put(
        &self,
        scope: &TenantScope,
        descriptor: &GenerationObjectDescriptor,
        signing_time: &R2GenerationUploadSigningTime,
        expires_seconds: u32,
    ) -> Result<R2GenerationUploadCapability, R2GenerationUploadCapabilityError> {
        if expires_seconds == 0 || expires_seconds > MAX_EXPIRES_SECONDS {
            return Err(R2GenerationUploadCapabilityError::InvalidExpiry);
        }
        validate_descriptor(scope, descriptor)?;

        let host = format!("{}.r2.cloudflarestorage.com", self.account_id);
        let checksum = sha256_hex_to_base64(descriptor.container_digest())?;
        let headers = vec![
            ("content-type".to_owned(), "application/octet-stream".to_owned()),
            ("if-none-match".to_owned(), "*".to_owned()),
            ("x-amz-checksum-sha256".to_owned(), checksum),
            (
                "x-amz-meta-container-bytes".to_owned(),
                descriptor.container_bytes().to_string(),
            ),
            (
                "x-amz-meta-container-digest".to_owned(),
                descriptor.container_digest().to_owned(),
            ),
            (
                "x-amz-meta-generation-id".to_owned(),
                descriptor.generation_id().as_str().to_owned(),
            ),
            (
                "x-amz-meta-metadata-digest".to_owned(),
                descriptor.metadata_digest().to_owned(),
            ),
            (
                "x-amz-meta-profile-id".to_owned(),
                descriptor.profile_id().as_str().to_owned(),
            ),
        ];

        let mut canonical_headers = String::new();
        canonical_headers.push_str("content-type:application/octet-stream\n");
        canonical_headers.push_str("host:");
        canonical_headers.push_str(&host);
        canonical_headers.push('\n');
        for (name, value) in &headers[1..] {
            canonical_headers.push_str(name);
            canonical_headers.push(':');
            canonical_headers.push_str(value);
            canonical_headers.push('\n');
        }

        let signed_headers = "content-type;host;if-none-match;x-amz-checksum-sha256;x-amz-meta-container-bytes;x-amz-meta-container-digest;x-amz-meta-generation-id;x-amz-meta-metadata-digest;x-amz-meta-profile-id";
        let credential_scope = format!(
            "{}/{}/{}/{}",
            signing_time.date_stamp, REGION, SERVICE, TERMINATOR
        );
        let credential = format!(
            "{}/{}",
            self.credentials.access_key_id, credential_scope
        );
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
            "PUT\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{UNSIGNED_PAYLOAD}"
        );
        let canonical_request_digest = hex_lower(&Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "{ALGORITHM}\n{}\n{credential_scope}\n{canonical_request_digest}",
            signing_time.amz_date
        );
        let signing_key = signing_key(
            self.credentials.secret_access_key.as_bytes(),
            &signing_time.date_stamp,
        );
        let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let url = format!(
            "https://{host}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}"
        );

        Ok(R2GenerationUploadCapability {
            url,
            headers,
            expires_seconds,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R2GenerationUploadCapabilityError {
    InvalidAccountId,
    InvalidBucketName,
    InvalidCredentials,
    InvalidSigningTime,
    InvalidExpiry,
    InvalidDescriptor,
    InvalidDigest,
}

impl fmt::Display for R2GenerationUploadCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAccountId => "R2 account identifier is invalid",
            Self::InvalidBucketName => "R2 bucket name is invalid",
            Self::InvalidCredentials => "R2 signing credentials are invalid",
            Self::InvalidSigningTime => "R2 signing time is invalid",
            Self::InvalidExpiry => "R2 upload capability expiry is invalid",
            Self::InvalidDescriptor => "R2 generation descriptor is invalid",
            Self::InvalidDigest => "R2 generation digest is invalid",
        })
    }
}

impl std::error::Error for R2GenerationUploadCapabilityError {}

fn validate_descriptor(
    scope: &TenantScope,
    descriptor: &GenerationObjectDescriptor,
) -> Result<(), R2GenerationUploadCapabilityError> {
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
        return Err(R2GenerationUploadCapabilityError::InvalidDescriptor);
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

fn sha256_hex_to_base64(value: &str) -> Result<String, R2GenerationUploadCapabilityError> {
    if !is_lower_hex_sha256(value) {
        return Err(R2GenerationUploadCapabilityError::InvalidDigest);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Ok(base64_encode(&bytes))
}

fn hex_value(byte: u8) -> Result<u8, R2GenerationUploadCapabilityError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(R2GenerationUploadCapabilityError::InvalidDigest),
    }
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(TABLE[usize::from(first >> 2)]));
        output.push(char::from(TABLE[usize::from(((first & 0x03) << 4) | (second >> 4))]));
        if chunk.len() > 1 {
            output.push(char::from(TABLE[usize::from(((second & 0x0f) << 2) | (third >> 6))]));
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(char::from(TABLE[usize::from(third & 0x3f)]));
        } else {
            output.push('=');
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
    use super::{
        R2GenerationUploadCapabilitySigner, R2GenerationUploadSigningTime, R2SigV4Credentials,
        hmac_sha256, uri_encode,
    };
    use application_ports::generation_objects::GenerationObjectDescriptor;
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId, TenantScope};

    fn descriptor() -> Result<GenerationObjectDescriptor, Box<dyn std::error::Error>> {
        Ok(GenerationObjectDescriptor::new(
            ProfileId::parse("profile_upload_capability_01")?,
            GenerationId::parse("generation_upload_capability_01")?,
            "tenants/tenant_upload_capability_01/profiles/profile_upload_capability_01/generations/generation_upload_capability_01.bpgc",
            "d".repeat(64),
            "e".repeat(64),
            4096,
        ))
    }

    fn signer() -> Result<R2GenerationUploadCapabilitySigner, Box<dyn std::error::Error>> {
        Ok(R2GenerationUploadCapabilitySigner::new(
            "0123456789abcdef0123456789abcdef",
            "profile-generations",
            R2SigV4Credentials::new("ACCESSKEYEXAMPLE", "secret-example-key")?,
        )?)
    }

    #[test]
    fn hmac_sha256_matches_rfc_4231_test_case_one() {
        let result = hmac_sha256(&[0x0b; 20], b"Hi There");
        assert_eq!(
            super::hex_lower(&result),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn uri_encoding_uses_sigv4_uppercase_percent_encoding() {
        assert_eq!(uri_encode("a b/c+d", true), "a%20b/c%2Bd");
        assert_eq!(uri_encode("a b/c+d", false), "a%20b%2Fc%2Bd");
    }

    #[test]
    fn exact_put_capability_binds_create_only_checksum_and_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let scope = TenantScope::new(TenantId::parse("tenant_upload_capability_01")?);
        let signing_time = R2GenerationUploadSigningTime::parse("20260810T120000Z")?;
        let capability = signer()?.sign_put(&scope, &descriptor()?, &signing_time, 300)?;

        assert!(capability.url().starts_with(
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/tenants/tenant_upload_capability_01/"
        ));
        for required in [
            "X-Amz-Algorithm=AWS4-HMAC-SHA256",
            "X-Amz-Expires=300",
            "X-Amz-SignedHeaders=content-type%3Bhost%3Bif-none-match%3Bx-amz-checksum-sha256%3Bx-amz-meta-container-bytes%3Bx-amz-meta-container-digest%3Bx-amz-meta-generation-id%3Bx-amz-meta-metadata-digest%3Bx-amz-meta-profile-id",
            "X-Amz-Signature=",
        ] {
            assert!(capability.url().contains(required));
        }
        let headers = capability.headers();
        assert!(headers.contains(&("if-none-match".to_owned(), "*".to_owned())));
        assert!(headers.iter().any(|(name, _)| name == "x-amz-checksum-sha256"));
        assert!(headers.contains(&(
            "x-amz-meta-container-digest".to_owned(),
            "e".repeat(64)
        )));
        assert!(headers.contains(&(
            "x-amz-meta-metadata-digest".to_owned(),
            "d".repeat(64)
        )));
        assert_eq!(capability.expires_seconds(), 300);
        Ok(())
    }

    #[test]
    fn signature_changes_when_protected_generation_metadata_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let scope = TenantScope::new(TenantId::parse("tenant_upload_capability_01")?);
        let signing_time = R2GenerationUploadSigningTime::parse("20260810T120000Z")?;
        let first = signer()?.sign_put(&scope, &descriptor()?, &signing_time, 300)?;
        let changed = GenerationObjectDescriptor::new(
            ProfileId::parse("profile_upload_capability_01")?,
            GenerationId::parse("generation_upload_capability_01")?,
            "tenants/tenant_upload_capability_01/profiles/profile_upload_capability_01/generations/generation_upload_capability_01.bpgc",
            "c".repeat(64),
            "e".repeat(64),
            4096,
        );
        let second = signer()?.sign_put(&scope, &changed, &signing_time, 300)?;
        assert_ne!(first.url(), second.url());
        Ok(())
    }
}
