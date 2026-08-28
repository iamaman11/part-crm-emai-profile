#![forbid(unsafe_code)]

pub mod coordinator_ingress;
pub mod error;
pub mod generations;
pub mod identity_acl;
pub mod profile_assignments;
pub mod profile_grants;
pub mod profile_launch;
pub mod profile_launch_authority;
pub mod profiles;

pub use error::ApplicationError;
pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};
