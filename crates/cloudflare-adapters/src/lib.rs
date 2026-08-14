#![forbid(unsafe_code)]

pub mod access_identity;
pub mod access_webcrypto;
pub mod cloud_mail_query;
pub mod cloud_mailbox_provider;
mod cloud_mailbox_secrets;
pub mod contact_keyring;
pub mod contact_lookup;
pub mod contact_protection;
pub mod control_plane_queue;
pub mod coordinator_ingress;
pub mod d1_authenticated_device;
pub mod d1_browser_mail_execution;
pub mod d1_catalog;
pub mod d1_client_mail_eligibility;
pub mod d1_client_merge;
pub mod d1_client_persistence;
pub mod d1_client_registry;
pub mod d1_clients;
mod d1_command_identity;
pub mod d1_contact_query;
pub mod d1_device_authorization;
pub mod d1_device_generation_commit;
pub mod d1_device_jobs;
pub mod d1_device_preconditions;
pub mod d1_global_query;
pub mod d1_governed_commands;
pub mod d1_idempotency;
pub mod d1_identity_acl;
pub mod d1_identity_ceremonies;
mod d1_identity_failure;
pub mod d1_identity_governance;
pub mod d1_identity_queries;
pub mod d1_integration_events;
pub mod d1_invitation_acceptance;
pub mod d1_mailbox_bindings;
pub mod d1_mailbox_client_associations;
pub mod d1_mailbox_jobs;
pub mod d1_mailbox_onboarding;
pub mod d1_mailbox_scheduling;
pub mod d1_mailboxes;
pub mod d1_notification_operations;
pub mod d1_notifications;
pub mod d1_outbound_mail_intents;
pub mod d1_profile_application;
pub mod d1_profile_coordinator;
pub mod d1_profile_generation_application;
pub mod d1_profile_generations;
pub mod d1_profiles;
pub mod d1_query;
pub mod d1_realtime_notifications;
pub mod device_generation_commit_runtime;
pub mod fake_mail_query;
mod gmail_mail_query;
pub mod gmail_mailbox;
pub mod gmail_oauth_provisioning;
pub mod gmail_outbound_mail;
pub mod gmail_send_capability;
mod gmail_send_credential;
pub mod imap_mailbox;
mod imap_query;
mod imap_session;
pub mod integration_event_queue;
pub mod mailbox_job_queue;
pub mod mailbox_provider;
pub mod microsoft_graph_authorization;
pub mod microsoft_graph_cursor;
pub mod microsoft_graph_delta;
pub mod microsoft_graph_delta_cursor;
#[cfg(test)]
mod microsoft_graph_evidence;
pub mod microsoft_graph_mail_query;
pub mod microsoft_graph_oauth_provisioning;
#[cfg(test)]
mod microsoft_graph_translation_evidence;
#[cfg(test)]
mod outbound_mail_evidence;
pub mod profile_coordinator;
pub mod r2_generation_objects;
pub mod r2_generation_upload_capability;
mod resolver_request;
pub mod smtp_outbound_mail;
mod smtp_send_credential;
mod smtp_session;
pub mod standards_mailbox_provisioning;

pub use mailbox_domain::MailboxProvider;

impl core::fmt::Display for imap_session::ImapTransportError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Authentication => "IMAP authentication failed",
            Self::ProviderPolicy => "IMAP provider policy rejected the operation",
            Self::DependencyUnavailable => "IMAP dependency is unavailable",
            Self::IntegrityFailure => "IMAP protocol integrity failure",
        })
    }
}

impl std::error::Error for imap_session::ImapTransportError {}
