use core::fmt;
use profile_platform_primitives::{ClientId, OpaqueId, TenantId};

const MAX_CIPHERTEXT_LENGTH: usize = 4096;
const HMAC_SHA256_HEX_LENGTH: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientContactKind {
    Email,
    Phone,
    Url,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedContactValue {
    ciphertext: String,
    lookup_token: String,
    key_version: u32,
}

impl ProtectedContactValue {
    pub fn new(
        ciphertext: impl Into<String>,
        lookup_token: impl Into<String>,
        key_version: u32,
    ) -> Result<Self, ClientContactError> {
        let ciphertext = ciphertext.into();
        let lookup_token = lookup_token.into();

        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_LENGTH {
            return Err(ClientContactError::InvalidCiphertext);
        }
        if lookup_token.len() != HMAC_SHA256_HEX_LENGTH
            || !lookup_token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ClientContactError::InvalidLookupToken);
        }
        if key_version == 0 {
            return Err(ClientContactError::InvalidKeyVersion);
        }

        Ok(Self {
            ciphertext,
            lookup_token,
            key_version,
        })
    }

    #[must_use]
    pub fn ciphertext(&self) -> &str {
        &self.ciphertext
    }

    #[must_use]
    pub fn lookup_token(&self) -> &str {
        &self.lookup_token
    }

    #[must_use]
    pub const fn key_version(&self) -> u32 {
        self.key_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientContactPoint {
    tenant_id: TenantId,
    client_id: ClientId,
    contact_id: OpaqueId,
    kind: ClientContactKind,
    protected_value: ProtectedContactValue,
}

impl ClientContactPoint {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        client_id: ClientId,
        contact_id: OpaqueId,
        kind: ClientContactKind,
        protected_value: ProtectedContactValue,
    ) -> Self {
        Self {
            tenant_id,
            client_id,
            contact_id,
            kind,
            protected_value,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn contact_id(&self) -> &OpaqueId {
        &self.contact_id
    }

    #[must_use]
    pub const fn kind(&self) -> ClientContactKind {
        self.kind
    }

    #[must_use]
    pub const fn protected_value(&self) -> &ProtectedContactValue {
        &self.protected_value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientContactError {
    InvalidCiphertext,
    InvalidLookupToken,
    InvalidKeyVersion,
}

impl fmt::Display for ClientContactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCiphertext => "client contact ciphertext is invalid",
            Self::InvalidLookupToken => "client contact lookup token is invalid",
            Self::InvalidKeyVersion => "client contact key version is invalid",
        })
    }
}

impl std::error::Error for ClientContactError {}

#[cfg(test)]
mod tests {
    use super::{ClientContactError, ClientContactKind, ClientContactPoint, ProtectedContactValue};
    use profile_platform_primitives::{ClientId, OpaqueId, TenantId};

    fn protected() -> Result<ProtectedContactValue, ClientContactError> {
        ProtectedContactValue::new("v1.synthetic-ciphertext", "a".repeat(64), 1)
    }

    #[test]
    fn protected_value_requires_hmac_sha256_hex_and_key_version() {
        assert_eq!(
            ProtectedContactValue::new("ciphertext", "a".repeat(63), 1),
            Err(ClientContactError::InvalidLookupToken)
        );
        assert_eq!(
            ProtectedContactValue::new("ciphertext", "A".repeat(64), 1),
            Err(ClientContactError::InvalidLookupToken)
        );
        assert_eq!(
            ProtectedContactValue::new("ciphertext", "a".repeat(64), 0),
            Err(ClientContactError::InvalidKeyVersion)
        );
    }

    #[test]
    fn contact_uses_caller_supplied_opaque_id_not_contact_value()
    -> Result<(), Box<dyn std::error::Error>> {
        let contact = ClientContactPoint::new(
            TenantId::parse("tenant_01JCONTACT")?,
            ClientId::parse("client_01JCONTACT")?,
            OpaqueId::parse("contact_01JCONTACT")?,
            ClientContactKind::Email,
            protected()?,
        );
        assert_eq!(contact.contact_id().as_str(), "contact_01JCONTACT");
        assert_eq!(contact.kind(), ClientContactKind::Email);
        assert_eq!(contact.protected_value().key_version(), 1);
        Ok(())
    }
}
