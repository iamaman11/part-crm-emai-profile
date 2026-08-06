#![cfg(windows)]
#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

#[must_use]
pub fn encode_wide_argument(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::encode_wide_argument;
    use std::ffi::OsStr;

    #[test]
    fn windows_argument_encoding_is_nul_terminated_without_unsafe_code() {
        let encoded = encode_wide_argument(OsStr::new("profile-bridge"));
        assert_eq!(encoded.last(), Some(&0));
        assert!(encoded.len() > 1);
    }
}
