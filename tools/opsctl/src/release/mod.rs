//! Project-specific release policy namespace.
//!
//! AR-11 owns semantic activation. The module remains a local policy engine:
//! no provider credentials, network clients, deployment execution or mutation authority.

pub mod artifact;
pub mod authority;
pub mod commands;
pub mod compatibility;
pub mod component_manifest;
pub mod digest;
pub mod input_topology;
pub mod model;
pub mod source;
pub mod static_compatibility;
pub mod v3_dto;

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

pub const TARGET_COMMANDS: &[&str] = &["inspect", "verify", "compatibility"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
pub const NETWORK_AUTHORITY: bool = false;
