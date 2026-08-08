use core::fmt;
use profile_platform_primitives::{
    AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
};

pub const INTEGRATION_EVENT_ENVELOPE_VERSION: u16 = 1;
const MAX_EVENT_TYPE_LEN: usize = 160;
const MAX_AGGREGATE_TYPE_LEN: usize = 80;
const MAX_PAYLOAD_LEN: usize = 4_096;

const PROHIBITED_PAYLOAD_KEYS: &[&str] = &[
    "access_token",
    "authorization",
    "body_html",
    "cookie",
    "cookies",
    "credential",
    "display_name",
    "email",
    "mail_body",
    "message_body",
    "oauth_token",
    "password",
    "phone",
    "proxy_credentials",
    "raw_message",
    "recipient",
    "refresh_token",
    "secret",
    "secret_handle",
    "sender",
    "snippet",
    "subject",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationEventPayload(String);

impl IntegrationEventPayload {
    pub fn metadata_json(value: impl Into<String>) -> Result<Self, IntegrationEventContractError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.len() > MAX_PAYLOAD_LEN || !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err(IntegrationEventContractError::InvalidPayload);
        }
        if !trimmed.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte.is_ascii_whitespace()
                || matches!(byte, b'{' | b'}' | b'[' | b']' | b'"' | b':' | b',' | b'_' | b'-' | b'.')
        }) {
            return Err(IntegrationEventContractError::InvalidPayload);
        }

        let normalized: String = trimmed
            .chars()
            .filter(|value| !value.is_ascii_whitespace())
            .flat_map(char::to_lowercase)
            .collect();
        if PROHIBITED_PAYLOAD_KEYS
            .iter()
            .any(|key| normalized.contains(&format!("\"{key}\":")))
        {
            return Err(IntegrationEventContractError::ProhibitedPayload);
        }

        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self("{}".to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationEventEnvelope {
    envelope_version: u16,
    event_id: OutboxEventId,
    tenant_id: TenantId,
    aggregate_type: String,
    aggregate_id: OpaqueId,
    aggregate_version: AggregateVersion,
    event_type: String,
    event_version: u16,
    payload: IntegrationEventPayload,
    occurred_at: UnixMillis,
}

impl IntegrationEventEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: OutboxEventId,
        tenant_id: TenantId,
        aggregate_type: impl Into<String>,
        aggregate_id: OpaqueId,
        aggregate_version: AggregateVersion,
        event_type: impl Into<String>,
        event_version: u16,
        payload: IntegrationEventPayload,
        occurred_at: UnixMillis,
    ) -> Result<Self, IntegrationEventContractError> {
        let aggregate_type = aggregate_type.into();
        let event_type = event_type.into();
        if !valid_symbol(&aggregate_type, MAX_AGGREGATE_TYPE_LEN) {
            return Err(IntegrationEventContractError::InvalidAggregateType);
        }
        if event_version == 0 || !valid_symbol(&event_type, MAX_EVENT_TYPE_LEN) {
            return Err(IntegrationEventContractError::InvalidEventType);
        }
        let expected_suffix = format!(".v{event_version}");
        if !event_type.ends_with(&expected_suffix) {
            return Err(IntegrationEventContractError::EventVersionMismatch);
        }

        Ok(Self {
            envelope_version: INTEGRATION_EVENT_ENVELOPE_VERSION,
            event_id,
            tenant_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            event_type,
            event_version,
            payload,
            occurred_at,
        })
    }

    #[must_use]
    pub const fn envelope_version(&self) -> u16 {
        self.envelope_version
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    #[must_use]
    pub const fn aggregate_id(&self) -> &OpaqueId {
        &self.aggregate_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    pub const fn event_version(&self) -> u16 {
        self.event_version
    }

    #[must_use]
    pub const fn payload(&self) -> &IntegrationEventPayload {
        &self.payload
    }

    #[must_use]
    pub const fn occurred_at(&self) -> UnixMillis {
        self.occurred_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationEventContractError {
    InvalidAggregateType,
    InvalidEventType,
    EventVersionMismatch,
    InvalidPayload,
    ProhibitedPayload,
}

impl fmt::Display for IntegrationEventContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAggregateType => "integration event aggregate type is invalid",
            Self::InvalidEventType => "integration event type is invalid",
            Self::EventVersionMismatch => "integration event type/version mismatch",
            Self::InvalidPayload => "integration event metadata payload is invalid",
            Self::ProhibitedPayload => "integration event payload contains prohibited content",
        })
    }
}

impl std::error::Error for IntegrationEventContractError {}

fn valid_symbol(value: &str, maximum: usize) -> bool {
    let length = value.len();
    (1..=maximum).contains(&length)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::{
        INTEGRATION_EVENT_ENVELOPE_VERSION, IntegrationEventEnvelope,
        IntegrationEventPayload,
    };
    use profile_platform_primitives::{
        AggregateVersion, OpaqueId, OutboxEventId, TenantId, UnixMillis,
    };

    fn envelope(payload: IntegrationEventPayload) -> Result<IntegrationEventEnvelope, Box<dyn std::error::Error>> {
        Ok(IntegrationEventEnvelope::new(
            OutboxEventId::parse("outbox_01JEVENT")?,
            TenantId::parse("tenant_01JEVENT")?,
            "client",
            OpaqueId::parse("client_01JEVENT")?,
            AggregateVersion::INITIAL,
            "client.created.v1",
            1,
            payload,
            UnixMillis::new(42),
        )?)
    }

    #[test]
    fn versioned_envelope_is_provider_neutral_and_stable() -> Result<(), Box<dyn std::error::Error>> {
        let event = envelope(IntegrationEventPayload::empty())?;
        assert_eq!(event.envelope_version(), INTEGRATION_EVENT_ENVELOPE_VERSION);
        assert_eq!(event.event_type(), "client.created.v1");
        assert_eq!(event.event_version(), 1);
        assert_eq!(event.aggregate_version(), AggregateVersion::INITIAL);
        assert_eq!(event.payload().as_str(), "{}");
        Ok(())
    }

    #[test]
    fn event_type_version_must_match() -> Result<(), Box<dyn std::error::Error>> {
        let result = IntegrationEventEnvelope::new(
            OutboxEventId::parse("outbox_01JEVENT")?,
            TenantId::parse("tenant_01JEVENT")?,
            "client",
            OpaqueId::parse("client_01JEVENT")?,
            AggregateVersion::INITIAL,
            "client.created.v2",
            1,
            IntegrationEventPayload::empty(),
            UnixMillis::new(42),
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn sanitizer_rejects_credentials_pii_and_mail_content() {
        for payload in [
            r#"{"secret_handle":"secret_01JABCDEF"}"#,
            r#"{"password":"value"}"#,
            r#"{"email":"person_example_com"}"#,
            r#"{"message_body":"hello"}"#,
            r#"{"subject":"private"}"#,
            r#"{"access_token":"token"}"#,
        ] {
            assert!(IntegrationEventPayload::metadata_json(payload).is_err(), "payload unexpectedly accepted: {payload}");
        }
    }

    #[test]
    fn sanitizer_accepts_bounded_opaque_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let payload = IntegrationEventPayload::metadata_json(
            r#"{"resource_id":"client_01JABCDEF","result_code":"created"}"#,
        )?;
        assert_eq!(payload.as_str(), r#"{"resource_id":"client_01JABCDEF","result_code":"created"}"#);
        envelope(payload)?;
        Ok(())
    }
}
