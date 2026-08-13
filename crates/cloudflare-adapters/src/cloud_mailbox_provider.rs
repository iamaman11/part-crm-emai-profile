use crate::cloud_mailbox_secrets::{MailboxCredential, resolve_mailbox_credential};
use crate::gmail_mailbox::check_gmail_mailbox;
use crate::imap_mailbox::check_imap_mailbox;
use crate::microsoft_graph_authorization::D1MicrosoftGraphAuthorization;
use crate::microsoft_graph_delta::check_microsoft_graph_mailbox;
use application_ports::mailboxes::{
    MailboxObservation, MailboxProviderPort, MailboxProviderPortError,
};
use mailbox_domain::{MailboxBinding, MailboxJob, MailboxProvider};
use profile_platform_primitives::ActorContext;
use worker::Env;

pub struct CloudMailboxProviderRouter<'a> {
    env: &'a Env,
    graph_authorization: Option<D1MicrosoftGraphAuthorization>,
    actor: Option<&'a ActorContext>,
}

impl<'a> CloudMailboxProviderRouter<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self {
            env,
            graph_authorization: None,
            actor: None,
        }
    }

    #[must_use]
    pub fn with_microsoft_graph_authorization(
        mut self,
        authorization: D1MicrosoftGraphAuthorization,
        actor: &'a ActorContext,
    ) -> Self {
        self.graph_authorization = Some(authorization);
        self.actor = Some(actor);
        self
    }
}

impl MailboxProviderPort for CloudMailboxProviderRouter<'_> {
    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> impl Future<Output = Result<MailboxObservation, MailboxProviderPortError>> {
        async move {
            if binding.provider() != MailboxProvider::MicrosoftGraph
                && !binding.provider().is_phase2e_cloud_supported()
            {
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
                (
                    MailboxProvider::MicrosoftGraph,
                    MailboxCredential::MicrosoftGraph(credential),
                ) => {
                    let authorization = self
                        .graph_authorization
                        .as_ref()
                        .ok_or(MailboxProviderPortError::IntegrityFailure)?;
                    let actor = self
                        .actor
                        .ok_or(MailboxProviderPortError::IntegrityFailure)?;
                    check_microsoft_graph_mailbox(
                        self.env,
                        binding,
                        job,
                        &credential,
                        authorization,
                        actor,
                    )
                    .await
                }
                (MailboxProvider::BrowserFallback, _)
                | (MailboxProvider::GmailApi, MailboxCredential::Imap(_))
                | (MailboxProvider::GmailApi, MailboxCredential::MicrosoftGraph(_))
                | (MailboxProvider::Imap, MailboxCredential::GmailApi(_))
                | (MailboxProvider::Imap, MailboxCredential::MicrosoftGraph(_))
                | (MailboxProvider::MicrosoftGraph, MailboxCredential::GmailApi(_))
                | (MailboxProvider::MicrosoftGraph, MailboxCredential::Imap(_)) => {
                    Err(MailboxProviderPortError::IntegrityFailure)
                }
            }
        }
    }
}
