//! Isolated WinCrypt FFI. Every input is a bounded borrowed slice; WinCrypt-owned handles are RAII.
//! APIs: CryptVerifyDetachedMessageSignature / CryptMsgGetParam (Microsoft wincrypt.h).
#![allow(unsafe_code)]

use sha2::{Digest, Sha256};
use std::ffi::{c_char, c_void};
use std::mem::{size_of, size_of_val};
use std::ptr;

const ENCODING: u32 = 0x0001_0001; // X509_ASN_ENCODING | PKCS_7_ASN_ENCODING
const SIGNER_HASH_ALGORITHM: u32 = 8; // CMSG_SIGNER_HASH_ALGORITHM_PARAM
const MAX_BYTES: usize = 1024 * 1024;
const SHA256_OID: &[u8] = b"2.16.840.1.101.3.4.2.1\0";

#[repr(C)]
struct Blob {
    size: u32,
    data: *mut u8,
}
#[repr(C)]
struct Algorithm {
    oid: *const c_char,
    parameters: Blob,
}
#[repr(C)]
struct Certificate {
    encoding: u32,
    encoded: *const u8,
    encoded_size: u32,
    info: *const c_void,
    store: *mut c_void,
}
#[repr(C)]
struct VerificationParameters {
    size: u32,
    encoding: u32,
    provider: usize,
    // A null callback uses only the signer's embedded certificate. No chain/trust evaluation.
    get_signer: *const c_void,
    argument: *mut c_void,
}

#[link(name = "Crypt32")]
unsafe extern "system" {
    fn CryptGetMessageSignerCount(encoding: u32, signature: *const u8, size: u32) -> i32;
    fn CryptVerifyDetachedMessageSignature(
        parameters: *const VerificationParameters,
        signer_index: u32,
        signature: *const u8,
        signature_size: u32,
        count: u32,
        contents: *const *const u8,
        sizes: *const u32,
        signer: *mut *const Certificate,
    ) -> i32;
    fn CertFreeCertificateContext(certificate: *const Certificate) -> i32;
    fn CryptMsgOpenToDecode(
        encoding: u32,
        flags: u32,
        kind: u32,
        provider: usize,
        recipient: *const c_void,
        stream: *const c_void,
    ) -> *mut c_void;
    fn CryptMsgUpdate(message: *mut c_void, data: *const u8, size: u32, final_part: i32) -> i32;
    fn CryptMsgGetParam(
        message: *mut c_void,
        parameter: u32,
        index: u32,
        output: *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn CryptMsgClose(message: *mut c_void) -> i32;
}

struct Message(*mut c_void);
impl Drop for Message {
    fn drop(&mut self) {
        // SAFETY: created by CryptMsgOpenToDecode, uniquely owned and released once.
        unsafe {
            CryptMsgClose(self.0);
        }
    }
}
struct Signer(*const Certificate);
impl Drop for Signer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: returned reference from CryptVerifyDetachedMessageSignature, released once.
            unsafe {
                CertFreeCertificateContext(self.0);
            }
        }
    }
}

/// Verify exactly one detached SHA-256 signer, with the caller-admitted certificate SHA-256.
/// Untrusted or malformed input always returns false. No private key or certificate store is used.
#[must_use]
pub fn verify_detached_sha256(manifest: &[u8], signature: &[u8], pin: &[u8; 32]) -> bool {
    if manifest.is_empty()
        || manifest.len() > MAX_BYTES
        || signature.is_empty()
        || signature.len() > MAX_BYTES
    {
        return false;
    }
    let Ok(signature_size) = u32::try_from(signature.len()) else {
        return false;
    };
    let Ok(content_size) = u32::try_from(manifest.len()) else {
        return false;
    };
    // SAFETY: buffers remain alive throughout every synchronous API call; lengths match slices.
    if unsafe { CryptGetMessageSignerCount(ENCODING, signature.as_ptr(), signature_size) } != 1
        || !uses_sha256(signature, signature_size)
    {
        return false;
    }
    let parameters = VerificationParameters {
        size: size_of::<VerificationParameters>() as u32,
        encoding: ENCODING,
        provider: 0,
        get_signer: ptr::null(),
        argument: ptr::null_mut(),
    };
    let content = manifest.as_ptr();
    let mut signer = Signer(ptr::null());
    // SAFETY: one pointer and matching length; output is a Crypt32-owned certificate reference.
    let valid = unsafe {
        CryptVerifyDetachedMessageSignature(
            &parameters,
            0,
            signature.as_ptr(),
            signature_size,
            1,
            &content,
            &content_size,
            &mut signer.0,
        )
    };
    if valid == 0 || signer.0.is_null() {
        return false;
    }
    // SAFETY: successful API returned a valid CERT_CONTEXT, held alive by Signer.
    let certificate = unsafe { &*signer.0 };
    if certificate.encoded.is_null()
        || certificate.encoded_size == 0
        || certificate.encoded_size as usize > MAX_BYTES
    {
        return false;
    }
    // SAFETY: certificate bytes/length are owned by that same live Crypt32 context.
    let bytes = unsafe {
        std::slice::from_raw_parts(certificate.encoded, certificate.encoded_size as usize)
    };
    Sha256::digest(bytes).as_slice() == pin
}

fn uses_sha256(signature: &[u8], size: u32) -> bool {
    // SAFETY: null optional parameters select the default decoder without a provider/store.
    let raw = unsafe { CryptMsgOpenToDecode(ENCODING, 0, 0, 0, ptr::null(), ptr::null()) };
    if raw.is_null() {
        return false;
    }
    let message = Message(raw);
    // SAFETY: message is live and signature points to size bytes.
    if unsafe { CryptMsgUpdate(message.0, signature.as_ptr(), size, 1) } == 0 {
        return false;
    }
    let mut required = 0u32;
    // SAFETY: null output asks only for the required buffer size.
    if unsafe {
        CryptMsgGetParam(
            message.0,
            SIGNER_HASH_ALGORITHM,
            0,
            ptr::null_mut(),
            &mut required,
        )
    } == 0
        || required as usize > MAX_BYTES
        || (required as usize) < size_of::<Algorithm>()
    {
        return false;
    }
    // Word allocation provides alignment for CRYPT_ALGORITHM_IDENTIFIER and its embedded pointers.
    let mut buffer = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    let capacity = size_of_val(buffer.as_slice());
    let mut written = required;
    // SAFETY: allocated aligned buffer is at least required bytes, with a live size output.
    if unsafe {
        CryptMsgGetParam(
            message.0,
            SIGNER_HASH_ALGORITHM,
            0,
            buffer.as_mut_ptr().cast(),
            &mut written,
        )
    } == 0
        || written as usize > capacity
        || (written as usize) < size_of::<Algorithm>()
    {
        return false;
    }
    // SAFETY: success populated an aligned Algorithm in this buffer; length checked above.
    let algorithm = unsafe { &*buffer.as_ptr().cast::<Algorithm>() };
    let begin = buffer.as_ptr() as usize;
    let Some(end) = begin.checked_add(written as usize) else {
        return false;
    };
    let oid = algorithm.oid as usize;
    if oid < begin
        || oid
            .checked_add(SHA256_OID.len())
            .is_none_or(|limit| limit > end)
    {
        return false;
    }
    // SAFETY: pointer and exact byte range were checked within the live returned buffer.
    (unsafe { std::slice::from_raw_parts(algorithm.oid.cast::<u8>(), SHA256_OID.len()) })
        == SHA256_OID
}

#[cfg(test)]
mod tests {
    use super::verify_detached_sha256;
    #[test]
    fn malformed_or_empty_cms_fails_without_processes() {
        for bytes in [b"".as_slice(), &[0x30, 0], b"not-cms"] {
            assert!(!verify_detached_sha256(b"manifest", bytes, &[0; 32]));
        }
        assert!(!verify_detached_sha256(&[], b"not-cms", &[0; 32]));
    }
}
