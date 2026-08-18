//! Reserved project-specific recovery policy namespace.
//!
//! AR-14 owns recovery rehearsal and semantic activation. AR-9 must not expose
//! restore/destructive recovery actions or turn Time Travel into automatic rollback.

pub const TARGET_COMMANDS: &[&str] = &["inspect", "plan", "verify"];
pub const ACTIVATION_OWNER: &str = "AR-14";
pub const AUTOMATIC_RESTORE_AUTHORITY: bool = false;
pub const PROVIDER_MUTATION_AUTHORITY: bool = false;
