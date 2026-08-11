use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use core::future::Future;
pub use mailbox_domain::{MailboxBindingStatus, MailboxProvider};
use profile_platform_primitives::{ActorContext, AggregateVersion, MailboxBindingId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxReadProjection {
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    status: MailboxBindingStatus,
    version: AggregateVersion,
}

impl MailboxReadProjection {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        provider: MailboxProvider,
        status: MailboxBindingStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            binding_id,
            provider,
            status,
            version,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn status(&self) -> MailboxBindingStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

pub trait MailboxReadModelPort {
    fn list_mailboxes(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> impl Future<Output = Result<QueryPage<MailboxReadProjection>, QueryPortError>>;
}
