#![forbid(unsafe_code)]

pub mod clients;
pub mod error;
pub mod generations;
pub mod identity_acl;
pub mod mailbox_jobs;
pub mod mailboxes;
pub mod profile_assignments;
pub mod profiles;

pub use clients::{CreateClientCommand, decide_create_client};
pub use error::ApplicationError;
pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};
