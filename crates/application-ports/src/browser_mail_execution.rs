use crate::query::QueryPortError;
use core::future::Future;
use profile_platform_primitives::{MailboxBindingId, ProfileId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailboxExecutionBinding {
    binding_id: MailboxBindingId,
    profile_id: ProfileId,
}

impl BrowserMailboxExecutionBinding {
    #[must_use]
    pub const fn new(binding_id: MailboxBindingId, profile_id: ProfileId) -> Self {
        Self {
            binding_id,
            profile_id,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }
}

pub trait BrowserMailboxExecutionBindingPort {
    fn resolve_browser_mailbox_execution_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> impl Future<Output = Result<Option<BrowserMailboxExecutionBinding>, QueryPortError>>;
}
