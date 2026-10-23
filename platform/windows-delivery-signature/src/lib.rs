//! Windows detached delivery-signature verification only. No trust-store, enrollment or network API.
//! Application signer admission and replay policy stay in Profile Bridge.
#![deny(unsafe_code)]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::verify_detached_sha256;
