#![forbid(unsafe_code)]

// Temporary CI synchronization marker.
pub mod access_identity;
pub mod access_webcrypto;
pub mod d1_catalog;
mod d1_command_identity;
pub mod d1_governed_commands;
pub mod d1_idempotency;
pub mod d1_identity_acl;
pub mod d1_identity_queries;
pub mod d1_invitation_acceptance;
pub mod d1_mailboxes;
pub mod d1_profile_coordinator;
pub mod d1_profile_generations;
pub mod mailbox_provider;
pub mod profile_coordinator;

pub use mailbox_domain::MailboxProvider;
