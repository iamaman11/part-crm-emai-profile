use core::fmt;

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

        let header = decode_json_segment(header_segment)?;
        let payload = decode_json_segment(payload_segment)?;
        let signature = decode_base64url(signature_segment)?;
        if signature.is_empty() {
            return Err(AccessIdentityError::MalformedToken);
        }

        let algorithm = json_string(&header, "alg")?;
        let key_id = json_string(&header, "kid")?;
        if algorithm != ACCESS_JWT_ALGORITHM || !valid_claim(&key_id) {
            return Err(AccessIdentityError::UnsupportedToken);
        }

        let signing_input = format!("{header_segment}.{payload_segment}");
        if !self
            .signature_verifier
            .verify_rs256(&key_id, &signing_input, &signature)
        {
            return Err(AccessIdentityError::InvalidSignature);
        }

        let issuer = json_string(&payload, "iss")?;
        if issuer != self.config.issuer {
            return Err(AccessIdentityError::IssuerMismatch);
        }
        if !json_audience_contains(&payload, &self.config.audience)? {
            return Err(AccessIdentityError::AudienceMismatch);
        }

        let expires_at = json_u64(&payload, "exp")?;
        if expires_at <= now_epoch_seconds {
            return Err(AccessIdentityError::Expired);
        }
        if json_optional_u64(&payload, "nbf")?
            .is_some_and(|not_before| not_before > now_epoch_seconds)
        {
            return Err(AccessIdentityError::NotYetValid);
        }

        let subject = json_string(&payload, "sub")?;
        if !valid_claim(&subject) {
            return Err(AccessIdentityError::InvalidSubject);
        }
        let contact_hint =
            json_optional_string(&payload, "email")?.filter(|value| valid_claim(value));

        Ok(VerifiedExternalIdentity::new(subject, contact_hint))
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

fn valid_claim(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_CLAIM_LENGTH && !value.contains('\0')
}

fn decode_json_segment(segment: &str) -> Result<String, AccessIdentityError> {
    let bytes = decode_base64url(segment)?;
    String::from_utf8(bytes).map_err(|_| AccessIdentityError::MalformedToken)
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

fn json_string(document: &str, key: &str) -> Result<String, AccessIdentityError> {
    json_optional_string(document, key)?.ok_or(AccessIdentityError::MalformedToken)
}

fn json_optional_string(document: &str, key: &str) -> Result<Option<String>, AccessIdentityError> {
    let Some(value_start) = json_value_start(document, key) else {
        return Ok(None);
    };
    parse_json_string(document, value_start).map(Some)
}

fn json_u64(document: &str, key: &str) -> Result<u64, AccessIdentityError> {
    json_optional_u64(document, key)?.ok_or(AccessIdentityError::MalformedToken)
}

fn json_optional_u64(document: &str, key: &str) -> Result<Option<u64>, AccessIdentityError> {
    let Some(start) = json_value_start(document, key) else {
        return Ok(None);
    };
    let digits: String = document[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return Err(AccessIdentityError::MalformedToken);
    }
    digits
        .parse::<u64>()
        .map(Some)
        .map_err(|_| AccessIdentityError::MalformedToken)
}

fn json_audience_contains(document: &str, expected: &str) -> Result<bool, AccessIdentityError> {
    let start = json_value_start(document, "aud").ok_or(AccessIdentityError::MalformedToken)?;
    let bytes = document.as_bytes();
    match bytes.get(start) {
        Some(b'"') => Ok(parse_json_string(document, start)? == expected),
        Some(b'[') => {
            let mut cursor = start + 1;
            loop {
                cursor = skip_whitespace(document, cursor);
                match bytes.get(cursor) {
                    Some(b']') => return Ok(false),
                    Some(b'"') => {
                        let value = parse_json_string(document, cursor)?;
                        if value == expected {
                            return Ok(true);
                        }
                        cursor = string_end(document, cursor)?;
                        cursor = skip_whitespace(document, cursor);
                        match bytes.get(cursor) {
                            Some(b',') => cursor += 1,
                            Some(b']') => return Ok(false),
                            _ => return Err(AccessIdentityError::MalformedToken),
                        }
                    }
                    _ => return Err(AccessIdentityError::MalformedToken),
                }
            }
        }
        _ => Err(AccessIdentityError::MalformedToken),
    }
}

fn json_value_start(document: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let key_start = document.find(&needle)?;
    let after_key = skip_whitespace(document, key_start + needle.len());
    if document.as_bytes().get(after_key) != Some(&b':') {
        return None;
    }
    Some(skip_whitespace(document, after_key + 1))
}

fn skip_whitespace(document: &str, mut cursor: usize) -> usize {
    while document
        .as_bytes()
        .get(cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        cursor += 1;
    }
    cursor
}

fn parse_json_string(document: &str, start: usize) -> Result<String, AccessIdentityError> {
    let end = string_end(document, start)?;
    let raw = &document[start + 1..end - 1];
    if raw.contains('\\') || raw.chars().any(char::is_control) {
        return Err(AccessIdentityError::MalformedToken);
    }
    Ok(raw.to_owned())
}

fn string_end(document: &str, start: usize) -> Result<usize, AccessIdentityError> {
    if document.as_bytes().get(start) != Some(&b'"') {
        return Err(AccessIdentityError::MalformedToken);
    }
    for (offset, byte) in document.as_bytes()[start + 1..].iter().enumerate() {
        if *byte == b'\\' {
            return Err(AccessIdentityError::MalformedToken);
        }
        if *byte == b'"' {
            return Ok(start + offset + 2);
        }
    }
    Err(AccessIdentityError::MalformedToken)
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
        let jwt = token(
            r#"{"iss":"https://team.cloudflareaccess.com","aud":["other","app-aud"],"sub":"access-subject-01","email":"member@example.test","exp":2000,"nbf":900}"#,
            &signature,
        );
        let access_identity = adapter.verify(&jwt, 1000)?;
        let fake_identity = DeterministicFakeIdentityAdapter::new(
            "access-subject-01",
            Some("member@example.test".to_owned()),
        )?
        .verify();

        assert_eq!(access_identity, fake_identity);
        Ok(())
    }

    #[test]
    fn rejects_bad_signature_and_expired_or_wrong_audience()
    -> Result<(), Box<dyn std::error::Error>> {
        let adapter = CloudflareAccessJwtAdapter::new(
            AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?,
            DeterministicVerifier {
                expected_signature: vec![9],
            },
        );
        let signature = [1_u8];
        let valid_payload = r#"{"iss":"https://team.cloudflareaccess.com","aud":"app-aud","sub":"access-subject-01","exp":2000}"#;
        assert_eq!(
            adapter.verify(&token(valid_payload, &signature), 1000),
            Err(AccessIdentityError::InvalidSignature)
        );

        let accepting = CloudflareAccessJwtAdapter::new(
            AccessJwtConfig::new("https://team.cloudflareaccess.com", "app-aud")?,
            DeterministicVerifier {
                expected_signature: signature.to_vec(),
            },
        );
        let expired = r#"{"iss":"https://team.cloudflareaccess.com","aud":"app-aud","sub":"access-subject-01","exp":1000}"#;
        assert_eq!(
            accepting.verify(&token(expired, &signature), 1000),
            Err(AccessIdentityError::Expired)
        );
        let wrong_audience = r#"{"iss":"https://team.cloudflareaccess.com","aud":"foreign-aud","sub":"access-subject-01","exp":2000}"#;
        assert_eq!(
            accepting.verify(&token(wrong_audience, &signature), 1000),
            Err(AccessIdentityError::AudienceMismatch)
        );
        Ok(())
    }
}
