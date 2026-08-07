#![forbid(unsafe_code)]

pub mod clients;
pub mod error;
pub mod identity_acl;
pub mod mailboxes;
pub mod profiles;

pub use clients::{CreateClientCommand, decide_create_client};
pub use error::ApplicationError;
pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};
