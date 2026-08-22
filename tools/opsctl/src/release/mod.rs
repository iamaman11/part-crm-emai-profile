//! Project-specific release contract and policy namespace.
//!
//! Release Set semantics are owned by the typed pure core. This outer module owns
//! local filesystem/JSON adapters only; it has no provider credentials, network
//! clients, deployment execution or production mutation authority.

pub mod artifact;
pub mod authority;
pub mod commands;
pub mod compatibility;
pub mod component_manifest;
pub mod digest;
pub mod finalize;
pub mod input_topology;
pub mod model;
pub mod source;
pub mod static_compatibility;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAction {
    Inspect,
    Verify,
    Compatibility,
}

impl ReleaseAction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Verify => "verify",
            Self::Compatibility => "compatibility",
        }
    }
}

pub const TARGET_COMMANDS: &[&str] = &["finalize", "inspect", "verify", "compatibility"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
pub const NETWORK_AUTHORITY: bool = false;
