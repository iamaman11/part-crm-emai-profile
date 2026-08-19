//! Project-specific release policy namespace.
//!
//! AR-11 owns semantic activation. The module remains a local policy engine:
//! no provider credentials, network clients, deployment execution or mutation authority.

pub mod authority;
pub mod model;

pub const TARGET_COMMANDS: &[&str] = &["inspect", "verify", "compatibility"];
pub const ACTIVATION_OWNER: &str = "AR-11";
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
pub const NETWORK_AUTHORITY: bool = false;
