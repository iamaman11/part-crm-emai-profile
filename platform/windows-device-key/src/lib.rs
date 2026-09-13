#![deny(unsafe_code)]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    KeyDisposition, P256_SPKI_DER_BYTES, P1363_SIGNATURE_BYTES, PersistedP256Key,
    WindowsDeviceKeyError,
};
