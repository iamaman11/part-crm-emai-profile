use crate::cloud_mailbox_secrets::{MailboxCredential, resolve_mailbox_credential};
use crate::gmail_mailbox::check_gmail_mailbox;
use crate::imap_mailbox::check_imap_mailbox;
use application_ports::mailboxes::{
    MailboxObservation, MailboxProviderPort, MailboxProviderPortError,
};
use mailbox_domain::{MailboxBinding, MailboxJob, MailboxProvider};
use worker::Env;

pub struct CloudMailboxProviderRouter<'a> {
    env: &'a Env,
}

impl<'a> CloudMailboxProviderRouter<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl MailboxProviderPort for CloudMailboxProviderRouter<'_> {
    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> impl Future<Output = Result<MailboxObservation, MailboxProviderPortError>> {
        async move {
            if !binding.provider().is_phase2e_cloud_supported() {
                return Err(MailboxProviderPortError::IntegrityFailure);
            }
            let credential = resolve_mailbox_credential(self.env, binding).await?;
            if credential.provider() != binding.provider() {
                return Err(MailboxProviderPortError::IntegrityFailure);
            }
            match (binding.provider(), credential) {
                (MailboxProvider::GmailApi, MailboxCredential::GmailApi(credential)) => {
                    check_gmail_mailbox(binding, job, &credential).await
                }
                (MailboxProvider::Imap, MailboxCredential::Imap(credential)) => {
                    check_imap_mailbox(binding, &credential).await
                }
                (MailboxProvider::BrowserFallback, _)
                | (MailboxProvider::GmailApi, MailboxCredential::Imap(_))
                | (MailboxProvider::Imap, MailboxCredential::GmailApi(_)) => {
                    Err(MailboxProviderPortError::IntegrityFailure)
                }
            }
        }
    }
}
