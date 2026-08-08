#![forbid(unsafe_code)]

pub mod access_identity;
pub mod access_webcrypto;
pub mod coordinator_ingress;
pub mod d1_catalog;
pub mod d1_clients;
mod d1_command_identity;
pub mod d1_governed_commands;
pub mod d1_idempotency;
pub mod d1_identity_acl;
pub mod d1_identity_ceremonies;
mod d1_identity_failure;
pub mod d1_identity_governance;
pub mod d1_identity_queries;
pub mod d1_invitation_acceptance;
pub mod d1_mailbox_bindings;
pub mod d1_mailbox_jobs;
pub mod d1_mailboxes;
pub mod d1_profile_coordinator;
pub mod d1_profile_generation_application;
pub mod d1_profile_generations;
pub mod d1_profiles;
pub mod mailbox_provider;
pub mod profile_coordinator;

pub use mailbox_domain::MailboxProvider;
