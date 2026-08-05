use core::fmt;
use serde::Deserialize;

const ACCESS_JWT_ALGORITHM: &str = "RS256";
const MAX_CLAIM_LENGTH: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedExternalIdentity {
    subject: String,
    contact_hint: Option<String>,
}

impl VerifiedExternalIdentity {
    fn new(subject: String, contact_hint: Option<String>) -> Self {
        Self {
            subject,
            contact_hint,
        }
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn contact_hint(&self) -> Option<&str> {
        self.contact_hint.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessJwtConfig {
    issuer: String,
    audience: String,
}

impl AccessJwtConfig {
    pub fn new(
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Result<Self, AccessIdentityError> {
        let issuer = issuer.into();
        let audience = audience.into();
        if !valid_claim(&issuer) || !valid_claim(&audience) {
            return Err(AccessIdentityError::InvalidConfiguration);
        }
        Ok(Self { issuer, audience })
    }

    pub fn prepare(
        &self,
        token: &str,
        now_epoch_seconds: u64,
    ) -> Result<PreparedAccessJwt, AccessIdentityError> {
        let mut segments = token.split('.');
        let header_segment = segments.next().ok_or(AccessIdentityError::MalformedToken)?;
        let payload_segment = segments.next().ok_or(AccessIdentityError::MalformedToken)?;
        let signature_segment = segments.next().ok_or(AccessIdentityError::MalformedToken)?;
        if segments.next().is_some()
            || header_segment.is_empty()
            || payload_segment.is_empty()
            || signature_segment.is_empty()
        {
            return Err(AccessIdentityError::MalformedToken);
        }

        let header: AccessHeader = decode_json_segment(header_segment)?;
        let claims: AccessClaims = decode_json_segment(payload_segment)?;
        let signature = decode_base64url(signature_segment)?;
        if signature.is_empty() {
            return Err(AccessIdentityError::MalformedToken);
        }
        if header.algorithm != ACCESS_JWT_ALGORITHM || !valid_claim(&header.key_id) {
            return Err(AccessIdentityError::UnsupportedToken);
        }
        if claims.issuer != self.issuer {
            return Err(AccessIdentityError::IssuerMismatch);
        }
        if !claims.audience.contains(&self.audience) {
            return Err(AccessIdentityError::AudienceMismatch);
        }
        if claims.expires_at <= now_epoch_seconds {
            return Err(AccessIdentityError::Expired);
        }
        if claims
            .not_before
            .is_some_and(|not_before| not_before > now_epoch_seconds)
        {
            return Err(AccessIdentityError::NotYetValid);
        }
        if !valid_claim(&claims.subject) {
            return Err(AccessIdentityError::InvalidSubject);
        }
        let contact_hint = claims.contact_hint.filter(|value| valid_claim(value));

        Ok(PreparedAccessJwt {
            key_id: header.key_id,
            signing_input: format!("{header_segment}.{payload_segment}"),
            signature,
            identity: VerifiedExternalIdentity::new(claims.subject, contact_hint),
        })
    }

    pub fn accept_verified(
        &self,
        prepared: PreparedAccessJwt,
        signature_valid: bool,
    ) -> Result<VerifiedExternalIdentity, AccessIdentityError> {
        if !signature_valid {
            return Err(AccessIdentityError::InvalidSignature);
        }
        Ok(prepared.identity)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedAccessJwt {
    key_id: String,
    signing_input: String,
    signature: Vec<u8>,
    identity: VerifiedExternalIdentity,
}

impl PreparedAccessJwt {
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    #[must_use]
    pub fn signing_input(&self) -> &str {
        &self.signing_input
    }

    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

pub trait AccessJwtSignatureVerifier {
    fn verify_rs256(&self, key_id: &str, signing_input: &str, signature: &[u8]) -> bool;
}

pub struct CloudflareAccessJwtAdapter<V> {
    config: AccessJwtConfig,
    signature_verifier: V,
}

impl<V> CloudflareAccessJwtAdapter<V>
where
    V: AccessJwtSignatureVerifier,
{
    #[must_use]
    pub const fn new(config: AccessJwtConfig, signature_verifier: V) -> Self {
        Self {
            config,
            signature_verifier,
        }
    }

    pub fn verify(
        &self,
        token: &str,
        now_epoch_seconds: u64,
    ) -> Result<VerifiedExternalIdentity, AccessIdentityError> {
        let prepared = self.config.prepare(token, now_epoch_seconds)?;
        let signature_valid = self.signature_verifier.verify_rs256(
            prepared.key_id(),
            prepared.signing_input(),
            prepared.signature(),
        );
        self.config.accept_verified(prepared, signature_valid)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicFakeIdentityAdapter {
    subject: String,
    contact_hint: Option<String>,
}

impl DeterministicFakeIdentityAdapter {
    pub fn new(
        subject: impl Into<String>,
        contact_hint: Option<String>,
    ) -> Result<Self, AccessIdentityError> {
        let subject = subject.into();
        if !valid_claim(&subject)
            || contact_hint
                .as_deref()
                .is_some_and(|value| !valid_claim(value))
        {
            return Err(AccessIdentityError::InvalidSubject);
        }
        Ok(Self {
            subject,
            contact_hint,
        })
    }

    #[must_use]
    pub fn verify(&self) -> VerifiedExternalIdentity {
        VerifiedExternalIdentity::new(self.subject.clone(), self.contact_hint.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessIdentityError {
    InvalidConfiguration,
    MalformedToken,
    UnsupportedToken,
    InvalidSignature,
    IssuerMismatch,
    AudienceMismatch,
    Expired,
    NotYetValid,
    InvalidSubject,
}

impl fmt::Display for AccessIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfiguration => "invalid Access JWT configuration",
            Self::MalformedToken => "malformed Access JWT",
            Self::UnsupportedToken => "unsupported Access JWT",
            Self::InvalidSignature => "invalid Access JWT signature",
            Self::IssuerMismatch => "Access JWT issuer mismatch",
            Self::AudienceMismatch => "Access JWT audience mismatch",
            Self::Expired => "Access JWT expired",
            Self::NotYetValid => "Access JWT is not yet valid",
            Self::InvalidSubject => "Access JWT subject is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AccessIdentityError {}

#[derive(Deserialize)]
struct AccessHeader {
    #[serde(rename = "alg")]
    algorithm: String,
    #[serde(rename = "kid")]
    key_id: String,
}

#[derive(Deserialize)]
struct AccessClaims {
    #[serde(rename = "iss")]
    issuer: String,
    #[serde(rename = "aud")]
    audience: AudienceClaim,
    #[serde(rename = "sub")]
    subject: String,
    #[serde(rename = "email")]
    contact_hint: Option<String>,
    #[serde(rename = "exp")]
    expires_at: u64,
    #[serde(rename = "nbf")]
    not_before: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl AudienceClaim {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.iter().any(|value| value == expected),
        }
    }
}

fn valid_claim(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_CLAIM_LENGTH && !value.contains('\0')
}

fn decode_json_segment<T>(segment: &str) -> Result<T, AccessIdentityError>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = decode_base64url(segment)?;
    serde_json::from_slice(&bytes).map_err(|_| AccessIdentityError::MalformedToken)
}

fn decode_base64url(value: &str) -> Result<Vec<u8>, AccessIdentityError> {
    if value.len() % 4 == 1 {
        return Err(AccessIdentityError::MalformedToken);
    }

    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    let mut output = Vec::with_capacity(value.len().saturating_mul(3) / 4);

    for byte in value.bytes() {
        let sextet = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a' + 26),
            b'0'..=b'9' => u32::from(byte - b'0' + 52),
            b'-' => 62,
            b'_' => 63,
            _ => return Err(AccessIdentityError::MalformedToken),
        };
        accumulator = (accumulator << 6) | sextet;
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    if bits > 0 && (accumulator & ((1_u32 << bits) - 1)) != 0 {
        return Err(AccessIdentityError::MalformedToken);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{
        AccessIdentityError, AccessJwtConfig, AccessJwtSignatureVerifier,
        CloudflareAccessJwtAdapter, DeterministicFakeIdentityAdapter,
    };

    struct DeterministicVerifier {
        expected_signature: Vec<u8>,
    }

    impl AccessJwtSignatureVerifier for DeterministicVerifier {
        fn verify_rs256(&self, key_id: &str, signing_input: &str, signature: &[u8]) -> bool {
            key_id == "key-01"
                && signing_input.contains('.')
                && signature == self.expected_signature
        }
    }

    fn encode_base64url(value: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut output = String::new();
        let mut cursor = 0;
        while cursor < value.len() {
            let first = u32::from(value[cursor]);
            let second = value.get(cursor + 1).copied().map_or(0, u32::from);
            let third = value.get(cursor + 2).copied().map_or(0, u32::from);
            let combined = (first << 16) | (second << 8) | third;
            output.push(char::from(ALPHABET[((combined >> 18) & 63) as usize]));
            output.push(char::from(ALPHABET[((combined >> 12) & 63) as usize]));
            if cursor + 1 < value.len() {
                output.push(char::from(ALPHABET[((combined >> 6) & 63) as usize]));
            }
            if cursor + 2 < value.len() {
                output.push(char::from(ALPHABET[(combined & 63) as usize]));
            }
            cursor += 3;
        }
        output
    }

    fn token(payload: &str, signature: &[u8]) -> String {
        let header = encode_base64url(br#"{"alg":"RS256","kid":"key-01"}"#);
        let payload = encode_base64url(payload.as_bytes());
        let signature = encode_base64url(signature);
        format!("{header}.{payload}.{signature}")
    }

    fn valid_payload() -> &'static str {
        r#"{"iss":"https://team.cloudflareaccess.com","aud":["other","app-aud"],"sub":"access-subject-01","email":"member@example.test","exp":2000,"nbf":900}"#
    }

    #[test]
    fn access_and_fake_adapters_produce_identical_verified_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let signature = vec![1, 2, 3, 4];
        let adapter = CloudflareAccessJwtAdapter::new(
            AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?,
            DeterministicVerifier {
                expected_signature: signature.clone(),
            },
        );
        let access_identity = adapter.verify(&token(valid_payload(), &signature), 1000)?;
        let fake_identity = DeterministicFakeIdentityAdapter::new(
            "access-subject-01",
            Some("member@example.test".to_owned()),
        )?
        .verify();

        assert_eq!(access_identity, fake_identity);
        Ok(())
    }

    #[test]
    fn prepared_token_requires_verified_signature() -> Result<(), Box<dyn std::error::Error>> {
        let config = AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?;
        let prepared = config.prepare(&token(valid_payload(), &[1, 2, 3]), 1000)?;
        assert_eq!(prepared.key_id(), "key-01");
        assert_eq!(
            config.accept_verified(prepared, false),
            Err(AccessIdentityError::InvalidSignature)
        );
        Ok(())
    }

    #[test]
    fn rejects_bad_signature_expiry_and_wrong_audience() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = CloudflareAccessJwtAdapter::new(
            AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?,
            DeterministicVerifier {
                expected_signature: vec![9],
            },
        );
        assert_eq!(
            adapter.verify(&token(valid_payload(), &[1]), 1000),
            Err(AccessIdentityError::InvalidSignature)
        );

        let accepting = CloudflareAccessJwtAdapter::new(
            AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?,
            DeterministicVerifier {
                expected_signature: vec![1],
            },
        );
        let expired = r#"{"iss":"https://team.cloudflareaccess.com","aud":"app-aud","sub":"access-subject-01","exp":1000}"#;
        assert_eq!(
            accepting.verify(&token(expired, &[1]), 1000),
            Err(AccessIdentityError::Expired)
        );
        let wrong_audience = r#"{"iss":"https://team.cloudflareaccess.com","aud":"foreign-aud","sub":"access-subject-01","exp":2000}"#;
        assert_eq!(
            accepting.verify(&token(wrong_audience, &[1]), 1000),
            Err(AccessIdentityError::AudienceMismatch)
        );
        Ok(())
    }
}
