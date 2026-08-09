use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use core::future::Future;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientMailEligibilityProjection {
    client_id: ClientId,
    mailbox_binding_id: MailboxBindingId,
}

impl ClientMailEligibilityProjection {
    #[must_use]
    pub const fn new(client_id: ClientId, mailbox_binding_id: MailboxBindingId) -> Self {
        Self {
            client_id,
            mailbox_binding_id,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn mailbox_binding_id(&self) -> &MailboxBindingId {
        &self.mailbox_binding_id
    }
}

pub trait MailReadModelPort {
    fn list_eligible_mailboxes_for_client(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        page: &QueryPageRequest,
    ) -> impl Future<Output = Result<QueryPage<ClientMailEligibilityProjection>, QueryPortError>>;
}
