//! Reserved aggregate readiness policy namespace.
//!
//! AR-16 owns final whole-project readiness proof. AR-9 intentionally provides
//! no readiness CLI action because architecture and production authorization are
//! still independently blocked states.

pub const TARGET_COMMAND: &str = "readiness";
pub const ACTIVATION_OWNER: &str = "AR-16";
pub const PRODUCTION_AUTHORIZATION_AUTHORITY: bool = false;
