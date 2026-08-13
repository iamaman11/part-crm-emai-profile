use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortError,
};
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct FakeAccess {
    allowed: bool,
    calls: AtomicUsize,
}

impl FakeAccess {
    pub(crate) const fn new(allowed: bool) -> Self {
        Self {
            allowed,
            calls: AtomicUsize::new(0),
        }
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ClientMailboxAccessPort for FakeAccess {
    async fn is_mailbox_accessible(
        &self,
        _actor: &ActorContext,
        _client_id: &ClientId,
        _binding_id: &MailboxBindingId,
    ) -> Result<bool, ClientMailboxAccessPortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.allowed)
    }
}
