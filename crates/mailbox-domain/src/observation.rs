use crate::{MailboxError, validate_cursor};
use profile_platform_primitives::{MailboxBindingId, UnixMillis};

const MAX_PROVIDER_STATUS_LENGTH: usize = 64;
const MAX_BOUNDED_ITEM_COUNT: u32 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxObservation {
    binding_id: MailboxBindingId,
    provider_status: String,
    bounded_item_count: u32,
    next_cursor: Option<String>,
}

impl MailboxObservation {
    pub fn new(
        binding_id: MailboxBindingId,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    ) -> Result<Self, MailboxError> {
        let provider_status = provider_status.into();
        validate_provider_status(&provider_status)?;
        validate_bounded_item_count(bounded_item_count)?;
        validate_cursor(next_cursor.as_deref())?;
        Ok(Self {
            binding_id,
            provider_status,
            bounded_item_count,
            next_cursor,
        })
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub fn provider_status(&self) -> &str {
        &self.provider_status
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProviderFailureClass {
    Authentication,
    RateLimited,
    TransientDependency,
    PermanentPolicy,
    Backpressure,
}

impl MailboxProviderFailureClass {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Authentication => "AUTH",
            Self::RateLimited => "RATE_LIMIT",
            Self::TransientDependency => "TRANSIENT_DEPENDENCY",
            Self::PermanentPolicy => "PERMANENT_POLICY",
            Self::Backpressure => "BACKPRESSURE",
        }
    }

    #[must_use]
    pub const fn disposition(self) -> MailboxFailureDisposition {
        match self {
            Self::Authentication => MailboxFailureDisposition::AuthRequired,
            Self::RateLimited | Self::TransientDependency | Self::Backpressure => {
                MailboxFailureDisposition::Retryable
            }
            Self::PermanentPolicy => MailboxFailureDisposition::Terminal,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxFailureDisposition {
    Retryable,
    AuthRequired,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxProviderFailure {
    class: MailboxProviderFailureClass,
    retry_at: Option<UnixMillis>,
}

impl MailboxProviderFailure {
    pub fn new(
        class: MailboxProviderFailureClass,
        retry_at: Option<UnixMillis>,
    ) -> Result<Self, MailboxError> {
        if retry_at.is_some() && class.disposition() != MailboxFailureDisposition::Retryable {
            return Err(MailboxError::InvalidFailureRetryHint);
        }
        Ok(Self { class, retry_at })
    }

    #[must_use]
    pub const fn class(self) -> MailboxProviderFailureClass {
        self.class
    }

    #[must_use]
    pub const fn disposition(self) -> MailboxFailureDisposition {
        self.class.disposition()
    }

    #[must_use]
    pub const fn retry_at(self) -> Option<UnixMillis> {
        self.retry_at
    }
}

pub fn validate_provider_status(value: &str) -> Result<(), MailboxError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_STATUS_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(MailboxError::InvalidProviderStatus);
    }
    Ok(())
}

pub fn validate_bounded_item_count(value: u32) -> Result<(), MailboxError> {
    if value > MAX_BOUNDED_ITEM_COUNT {
        return Err(MailboxError::InvalidBoundedItemCount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MailboxFailureDisposition, MailboxObservation, MailboxProviderFailure,
        MailboxProviderFailureClass,
    };
    use crate::MailboxError;
    use profile_platform_primitives::{MailboxBindingId, UnixMillis};

    #[test]
    fn observations_are_content_free_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let observation = MailboxObservation::new(
            MailboxBindingId::parse("mailbox_01JOBSERVATION")?,
            "IMAP_OK",
            4,
            Some("cursor-2".to_owned()),
        )?;
        assert_eq!(observation.provider_status(), "IMAP_OK");
        assert_eq!(observation.bounded_item_count(), 4);
        assert_eq!(observation.next_cursor(), Some("cursor-2"));
        assert_eq!(
            MailboxObservation::new(
                MailboxBindingId::parse("mailbox_01JOBSERVATION")?,
                "IMAP_OK",
                10_001,
                None,
            ),
            Err(MailboxError::InvalidBoundedItemCount)
        );
        Ok(())
    }

    #[test]
    fn failure_taxonomy_has_explicit_remediation_semantics()
    -> Result<(), Box<dyn std::error::Error>> {
        let auth = MailboxProviderFailure::new(MailboxProviderFailureClass::Authentication, None)?;
        assert_eq!(auth.disposition(), MailboxFailureDisposition::AuthRequired);
        let limited = MailboxProviderFailure::new(
            MailboxProviderFailureClass::RateLimited,
            Some(UnixMillis::new(20)),
        )?;
        assert_eq!(limited.disposition(), MailboxFailureDisposition::Retryable);
        assert_eq!(limited.retry_at(), Some(UnixMillis::new(20)));
        assert_eq!(
            MailboxProviderFailure::new(
                MailboxProviderFailureClass::PermanentPolicy,
                Some(UnixMillis::new(20)),
            ),
            Err(MailboxError::InvalidFailureRetryHint)
        );
        Ok(())
    }
}
