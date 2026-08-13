use core::fmt;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientMailboxAccessPortErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientMailboxAccessPortError {
    class: ClientMailboxAccessPortErrorClass,
}

impl ClientMailboxAccessPortError {
    #[must_use]
    pub const fn new(class: ClientMailboxAccessPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ClientMailboxAccessPortErrorClass {
        self.class
    }
}

impl fmt::Display for ClientMailboxAccessPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ClientMailboxAccessPortErrorClass::IntegrityFailure => {
                "client mailbox access integrity failure"
            }
            ClientMailboxAccessPortErrorClass::DependencyUnavailable => {
                "client mailbox access dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ClientMailboxAccessPortError {}

#[allow(async_fn_in_trait)]
pub trait ClientMailboxAccessPort {
    async fn is_mailbox_accessible(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, ClientMailboxAccessPortError>;
}
