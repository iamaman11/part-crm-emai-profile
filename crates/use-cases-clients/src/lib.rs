#![forbid(unsafe_code)]

pub mod client_grants;
pub mod clients;
pub mod contacts;
pub mod error;
pub mod lifecycle;
pub mod merge;
pub mod registry;

pub use clients::{CreateClientCommand, decide_create_client};
pub use error::ApplicationError;
pub use merge::{
    ClientMergeApplicationError, ClientMergeOutcome, MergeClientCommand, execute_merge_client,
};
