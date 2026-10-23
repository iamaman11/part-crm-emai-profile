#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::windows_delivery::DetachedSignatureVerifier;
use std::convert::Infallible;

/// Native synchronous verification. Signer admission/replay remain in delivery policy.
/// No process, scratch credential file, certificate store or external trust service.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WindowsCmsSignatureVerifier;

impl DetachedSignatureVerifier for WindowsCmsSignatureVerifier {
    type Error = Infallible;

    fn verify_cms(
        &mut self,
        manifest: &[u8],
        cms_der: &[u8],
        expected_certificate_sha256: &str,
    ) -> Result<bool, Self::Error> {
        if manifest.is_empty()
            || !has_exact_der_sequence_envelope(cms_der)
            || !is_lower_hex(expected_certificate_sha256, 64)
        {
            return Ok(false);
        }
        let mut pin = [0u8; 32];
        for (index, pair) in expected_certificate_sha256
            .as_bytes()
            .chunks_exact(2)
            .enumerate()
        {
            let digit = |value: u8| {
                if value.is_ascii_digit() {
                    value - b'0'
                } else {
                    value - b'a' + 10
                }
            };
            pin[index] = (digit(pair[0]) << 4) | digit(pair[1]);
        }
        Ok(windows_delivery_signature::verify_detached_sha256(
            manifest, cms_der, &pin,
        ))
    }
}

fn has_exact_der_sequence_envelope(value: &[u8]) -> bool {
    if value.len() < 2 || value[0] != 0x30 {
        return false;
    }

    let first_length = value[1];
    let (header_length, content_length) = if first_length & 0x80 == 0 {
        (2usize, usize::from(first_length))
    } else {
        let length_bytes = usize::from(first_length & 0x7f);
        if length_bytes == 0
            || length_bytes > std::mem::size_of::<usize>()
            || value.len() < 2 + length_bytes
            || value[2] == 0
        {
            return false;
        }
        let mut content_length = 0usize;
        for byte in &value[2..2 + length_bytes] {
            let Some(next) = content_length
                .checked_mul(256)
                .and_then(|current| current.checked_add(usize::from(*byte)))
            else {
                return false;
            };
            content_length = next;
        }
        if content_length < 128 {
            return false;
        }
        (2 + length_bytes, content_length)
    };

    header_length.checked_add(content_length) == Some(value.len())
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_cms_and_invalid_certificate_pin_fail_closed() -> Result<(), Infallible> {
        let mut verifier = WindowsCmsSignatureVerifier;
        assert!(!verifier.verify_cms(b"manifest", b"not-cms", &"a".repeat(64))?);
        assert!(!verifier.verify_cms(b"manifest", &[0x30, 0], &"a".repeat(64))?);
        assert!(!verifier.verify_cms(b"manifest", b"not-cms", "ABC")?);
        Ok(())
    }
    #[test]
    fn der_envelope_precheck_is_strict_and_bounded() {
        assert!(!has_exact_der_sequence_envelope(b"not-cms"));
        assert!(!has_exact_der_sequence_envelope(&[0x30, 0x80, 0x00, 0x00]));
        assert!(!has_exact_der_sequence_envelope(&[0x30, 0x81, 0x01, 0x00]));
        assert!(has_exact_der_sequence_envelope(&[0x30, 0x01, 0x00]));
        let mut long_form = vec![0x30, 0x81, 0x80];
        long_form.extend(std::iter::repeat_n(0, 128));
        assert!(has_exact_der_sequence_envelope(&long_form));
    }
}
