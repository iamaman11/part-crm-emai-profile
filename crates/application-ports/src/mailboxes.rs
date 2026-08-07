use mailbox_domain::{MailboxBinding, MailboxJob};
use profile_platform_primitives::MailboxBindingId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxObservation {
    binding_id: MailboxBindingId,
    provider_status: String,
    bounded_item_count: u32,
    next_cursor: Option<String>,
}

impl MailboxObservation {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            binding_id,
            provider_status: provider_status.into(),
            bounded_item_count,
            next_cursor,
        }
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

pub trait MailboxProviderPort {
    type Error;

    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, Self::Error>;
}
