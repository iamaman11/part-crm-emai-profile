#![deny(unsafe_code)]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    KeyDisposition, PersistedP256Key, WindowsDeviceKeyError, P1363_SIGNATURE_BYTES,
    P256_SPKI_DER_BYTES,
};
