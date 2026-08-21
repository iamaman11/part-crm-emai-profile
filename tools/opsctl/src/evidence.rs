//! Canonical `opsctl evidence` module entrypoint.
//!
//! The typed offline policy lives below this module to keep the permanent command
//! authority at the stable `tools/opsctl/src/evidence.rs` path.

#[path = "evidence/mod.rs"]
mod policy;

pub use policy::*;
