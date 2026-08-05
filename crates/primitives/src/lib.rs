#![forbid(unsafe_code)]

use core::fmt;

const MIN_ID_LENGTH: usize = 8;
const MAX_ID_LENGTH: usize = 64;

/// Stable opaque identifier safe for API and storage segments.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpaqueId(String);

impl OpaqueId {
    /// Parses an identifier without assigning business meaning to its contents.
    pub fn parse(value: impl Into<String>) -> Result<Self, ParseOpaqueIdError> {
        let value = value.into();
        let valid_length = (MIN_ID_LENGTH..=MAX_ID_LENGTH).contains(&value.len());
        let valid_chars = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));

        if !valid_length || !valid_chars {
            return Err(ParseOpaqueIdError);
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpaqueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseOpaqueIdError;

impl fmt::Display for ParseOpaqueIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("opaque ID must be 8-64 ASCII alphanumeric, '-' or '_' characters")
    }
}

impl std::error::Error for ParseOpaqueIdError {}

/// Required scope for every tenant-owned repository operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantScope {
    tenant_id: OpaqueId,
}

impl TenantScope {
    #[must_use]
    pub const fn new(tenant_id: OpaqueId) -> Self {
        Self { tenant_id }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &OpaqueId {
        &self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::{OpaqueId, TenantScope};

    #[test]
    fn accepts_safe_opaque_identifier() -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = OpaqueId::parse("tenant_01JABCDEF")?;
        let scope = TenantScope::new(tenant_id.clone());

        assert_eq!(scope.tenant_id(), &tenant_id);
        assert_eq!(tenant_id.as_str(), "tenant_01JABCDEF");
        Ok(())
    }

    #[test]
    fn rejects_email_and_path_like_values() {
        assert!(OpaqueId::parse("user@example.com").is_err());
        assert!(OpaqueId::parse("../../profile").is_err());
        assert!(OpaqueId::parse("short").is_err());
    }
}
