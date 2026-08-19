//! Reserved project-specific release policy namespace.
//!
//! AR-11 owns semantic activation. AR-9 intentionally exposes no `release`
//! CLI actions and grants this module no provider credential or mutation authority.

pub const TARGET_COMMANDS: &[&str] = &["inspect", "verify", "compatibility"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
