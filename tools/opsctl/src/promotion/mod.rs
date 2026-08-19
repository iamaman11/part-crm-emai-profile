//! Project-specific promotion policy namespace.
//!
//! AR-11 owns semantic activation. GitHub Environments remain approval and
//! orchestration authority; provider executors remain outside this policy module.

pub mod authority;
pub mod commands;
pub mod model;
pub mod plan;
pub mod preflight;
pub mod snapshot;
pub mod verify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionAction {
    Plan,
    Preflight,
    Verify,
}

impl PromotionAction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Preflight => "preflight",
            Self::Verify => "verify",
        }
    }
}

pub const TARGET_COMMANDS: &[&str] = &["plan", "preflight", "verify"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const GITHUB_APPROVAL_AUTHORITY: bool = false;
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
pub const NETWORK_AUTHORITY: bool = false;
pub const PRODUCTION_MUTATION_AUTHORIZED_DURING_AR11: bool = false;
