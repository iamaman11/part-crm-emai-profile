//! Reserved project-specific promotion policy namespace.
//!
//! AR-11 owns semantic activation. GitHub Environments remain approval and
//! orchestration authority; provider executors remain outside this policy module.

pub const TARGET_COMMANDS: &[&str] = &["plan", "preflight", "verify"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const GITHUB_APPROVAL_AUTHORITY: bool = false;
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
