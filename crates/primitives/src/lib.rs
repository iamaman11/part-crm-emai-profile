#![forbid(unsafe_code)]

use core::fmt;

const MIN_ID_LENGTH: usize = 8;
const MAX_ID_LENGTH: usize = 96;

/// Stable opaque identifier safe for API, metric and storage segments.
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
        formatter.write_str("opaque ID must be 8-96 ASCII alphanumeric, '-' or '_' characters")
    }
}

impl std::error::Error for ParseOpaqueIdError {}

macro_rules! define_typed_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(OpaqueId);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ParseOpaqueIdError> {
                OpaqueId::parse(value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            #[must_use]
            pub fn into_opaque(self) -> OpaqueId {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

define_typed_id!(TenantId);
define_typed_id!(ActorId);
define_typed_id!(IdentityId);
define_typed_id!(InvitationId);
define_typed_id!(ClientId);
define_typed_id!(ContactPointId);
define_typed_id!(ProfileId);
define_typed_id!(GenerationId);
define_typed_id!(AssignmentId);
define_typed_id!(SessionId);
define_typed_id!(DeviceId);
define_typed_id!(MailboxBindingId);
define_typed_id!(MailboxJobId);
define_typed_id!(LaunchIntentId);
define_typed_id!(CorrelationId);
define_typed_id!(IdempotencyKey);
define_typed_id!(AuditEventId);
define_typed_id!(OutboxEventId);
define_typed_id!(FencingToken);
define_typed_id!(SecretHandle);

/// Required scope for every tenant-owned repository operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantScope {
    tenant_id: TenantId,
}

impl TenantScope {
    #[must_use]
    pub const fn new(tenant_id: TenantId) -> Self {
        Self { tenant_id }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }
}

/// Verified identity and tenant context passed to every application command/query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActorContext {
    tenant_scope: TenantScope,
    actor_id: ActorId,
    correlation_id: CorrelationId,
}

impl ActorContext {
    #[must_use]
    pub const fn new(
        tenant_scope: TenantScope,
        actor_id: ActorId,
        correlation_id: CorrelationId,
    ) -> Self {
        Self {
            tenant_scope,
            actor_id,
            correlation_id,
        }
    }

    #[must_use]
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AggregateVersion(u64);

impl AggregateVersion {
    pub const INITIAL: Self = Self(1);

    /// Restores or creates a strictly positive aggregate version.
    pub const fn new(value: u64) -> Result<Self, ZeroAggregateVersion> {
        if value == 0 {
            Err(ZeroAggregateVersion)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, VersionOverflow> {
        self.0.checked_add(1).map(Self).ok_or(VersionOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZeroAggregateVersion;

impl fmt::Display for ZeroAggregateVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("aggregate version must be greater than zero")
    }
}

impl std::error::Error for ZeroAggregateVersion {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionOverflow;

impl fmt::Display for VersionOverflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("aggregate version overflow")
    }
}

impl std::error::Error for VersionOverflow {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UnixMillis(u64);

impl UnixMillis {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActorContext, ActorId, AggregateVersion, ContactPointId, CorrelationId, OpaqueId, TenantId,
        TenantScope,
    };

    #[test]
    fn accepts_safe_typed_identifiers() -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JABCDEF")?;
        let actor_id = ActorId::parse("actor_01JABCDEF")?;
        let correlation_id = CorrelationId::parse("corr_01JABCDEF")?;
        let scope = TenantScope::new(tenant_id.clone());
        let actor = ActorContext::new(scope, actor_id.clone(), correlation_id.clone());

        assert_eq!(actor.tenant_scope().tenant_id(), &tenant_id);
        assert_eq!(actor.actor_id(), &actor_id);
        assert_eq!(actor.correlation_id(), &correlation_id);
        Ok(())
    }

    #[test]
    fn contact_point_ids_are_opaque_and_reject_contact_values()
    -> Result<(), Box<dyn std::error::Error>> {
        let id = ContactPointId::parse("contact_01JABCDEF")?;
        assert_eq!(id.as_str(), "contact_01JABCDEF");
        assert!(ContactPointId::parse("person@example.com").is_err());
        assert!(ContactPointId::parse("+48123456789").is_err());
        assert!(ContactPointId::parse("https://example.com").is_err());
        Ok(())
    }

    #[test]
    fn rejects_email_and_path_like_values() {
        assert!(OpaqueId::parse("user@example.com").is_err());
        assert!(OpaqueId::parse("../../profile").is_err());
        assert!(OpaqueId::parse("short").is_err());
    }

    #[test]
    fn aggregate_versions_are_strictly_positive_and_never_wrap()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(AggregateVersion::new(1)?.next()?.value(), 2);
        assert!(AggregateVersion::new(0).is_err());
        assert!(AggregateVersion::new(u64::MAX)?.next().is_err());
        Ok(())
    }
}
