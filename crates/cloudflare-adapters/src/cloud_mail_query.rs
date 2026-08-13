use crate::cloud_mailbox_secrets::{MailboxCredential, resolve_mailbox_credential};
use crate::d1_mailboxes::D1MailboxRepository;
use crate::gmail_mail_query::{get_gmail_message, search_gmail_messages};
use crate::imap_query::{get_imap_message, search_imap_messages};
use crate::microsoft_graph_authorization::D1MicrosoftGraphAuthorization;
use crate::microsoft_graph_mail_query::{
    get_microsoft_graph_message, search_microsoft_graph_messages,
};
use application_ports::mailboxes::MailboxProviderPortError;
use application_ports::query::{QueryPage, QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use mailbox_domain::{MailboxBinding, MailboxProvider};
use profile_platform_primitives::{
    ActorContext, ClientId, MailboxBindingId, TenantScope,
};
use worker::Env;
use worker::d1::D1Database;

pub struct CloudMailboxQueryAdapter<'a> {
    env: &'a Env,
    mailboxes: D1MailboxRepository,
    graph_authorization: D1MicrosoftGraphAuthorization,
    actor: &'a ActorContext,
    client_id: &'a ClientId,
}

impl<'a> CloudMailboxQueryAdapter<'a> {
    #[must_use]
    pub const fn new(
        env: &'a Env,
        mailbox_database: D1Database,
        authorization_database: D1Database,
        actor: &'a ActorContext,
        client_id: &'a ClientId,
    ) -> Self {
        Self {
            env,
            mailboxes: D1MailboxRepository::new(mailbox_database),
            graph_authorization: D1MicrosoftGraphAuthorization::new(authorization_database),
            actor,
            client_id,
        }
    }

    async fn load_executable_binding(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxBinding>, QueryPortError> {
        let binding = self
            .mailboxes
            .find_binding(scope, binding_id)
            .await
            .map_err(|_| dependency_unavailable())?;
        Ok(binding.filter(MailboxBinding::is_executable))
    }
}

impl ClientMailProviderQueryPort for CloudMailboxQueryAdapter<'_> {
    fn search_messages(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        request: &SearchClientMailboxMessagesRequest,
    ) -> impl Future<Output = Result<QueryPage<MailMessageSummary>, QueryPortError>> {
        async move {
            let Some(binding) = self.load_executable_binding(scope, binding_id).await? else {
                return Ok(QueryPage::empty());
            };
            if binding.provider() != MailboxProvider::MicrosoftGraph
                && !binding.provider().is_phase2e_cloud_supported()
            {
                return Err(dependency_unavailable());
            }
            let credential = resolve_mailbox_credential(self.env, &binding)
                .await
                .map_err(map_secret_error)?;
            if credential.provider() != binding.provider() {
                return Err(integrity_failure());
            }
            match (binding.provider(), credential) {
                (MailboxProvider::GmailApi, MailboxCredential::GmailApi(credential)) => {
                    search_gmail_messages(&binding, request, &credential).await
                }
                (MailboxProvider::Imap, MailboxCredential::Imap(credential)) => {
                    search_imap_messages(&binding, request, &credential).await
                }
                (
                    MailboxProvider::MicrosoftGraph,
                    MailboxCredential::MicrosoftGraph(credential),
                ) => {
                    search_microsoft_graph_messages(
                        self.env,
                        &binding,
                        request,
                        &credential,
                        &self.graph_authorization,
                        self.actor,
                        self.client_id,
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
                    Err(integrity_failure())
                }
            }
        }
    }

    fn get_message(
        &self,
        scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> impl Future<Output = Result<Option<MailMessageBody>, QueryPortError>> {
        async move {
            let Some(binding) = self
                .load_executable_binding(scope, reference.binding_id())
                .await?
            else {
                return Ok(None);
            };
            if binding.provider() != MailboxProvider::MicrosoftGraph
                && !binding.provider().is_phase2e_cloud_supported()
            {
                return Err(dependency_unavailable());
            }
            let credential = resolve_mailbox_credential(self.env, &binding)
                .await
                .map_err(map_secret_error)?;
            if credential.provider() != binding.provider() {
                return Err(integrity_failure());
            }
            match (binding.provider(), credential) {
                (MailboxProvider::GmailApi, MailboxCredential::GmailApi(credential)) => {
                    get_gmail_message(&binding, reference.provider_reference(), &credential).await
                }
                (MailboxProvider::Imap, MailboxCredential::Imap(credential)) => {
                    get_imap_message(&binding, reference.provider_reference(), &credential).await
                }
                (
                    MailboxProvider::MicrosoftGraph,
                    MailboxCredential::MicrosoftGraph(credential),
                ) => {
                    get_microsoft_graph_message(
                        self.env,
                        &binding,
                        reference.provider_reference(),
                        &credential,
                        &self.graph_authorization,
                        self.actor,
                        self.client_id,
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
                    Err(integrity_failure())
                }
            }
        }
    }
}

fn map_secret_error(error: MailboxProviderPortError) -> QueryPortError {
    match error {
        MailboxProviderPortError::IntegrityFailure => integrity_failure(),
        MailboxProviderPortError::Failure(_) => dependency_unavailable(),
    }
}

const fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

const fn dependency_unavailable() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}
