#![allow(unsafe_code)]

use core::ffi::c_void;
use std::ptr;

const ERROR_SUCCESS: i32 = 0;
const NTE_BAD_SIGNATURE: i32 = 0x8009_0006_u32 as i32;
const NTE_EXISTS: i32 = 0x8009_000F_u32 as i32;
const NTE_BAD_KEYSET: i32 = 0x8009_0016_u32 as i32;
const NTE_BUFFER_TOO_SMALL: i32 = 0x8009_0028_u32 as i32;
const NCRYPT_PERSIST_FLAG: u32 = 0x8000_0000;

const PROVIDER_NAME: &str = "Microsoft Software Key Storage Provider";
const ECDSA_P256_ALGORITHM: &str = "ECDSA_P256";
const SHA256_ALGORITHM: &str = "SHA256";
const EXPORT_POLICY_PROPERTY: &str = "Export Policy";
const ECC_PUBLIC_BLOB: &str = "ECCPUBLICBLOB";
const ECC_PRIVATE_BLOB: &str = "ECCPRIVATEBLOB";

const MAX_ECC_BLOB_BYTES: usize = 256;
const MAX_SIGNATURE_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Status(i32);

impl Status {
    pub(super) const fn code(self) -> i32 {
        self.0
    }

    pub(super) const fn is_bad_keyset(self) -> bool {
        self.0 == NTE_BAD_KEYSET
    }

    pub(super) const fn is_exists(self) -> bool {
        self.0 == NTE_EXISTS
    }
}

pub(super) fn sha256(input: &[u8]) -> Result<[u8; 32], Status> {
    let algorithm = wide(SHA256_ALGORITHM);
    let mut handle = 0_usize;
    let status =
        unsafe { BCryptOpenAlgorithmProvider(&mut handle, algorithm.as_ptr(), ptr::null(), 0) };
    require_success(status)?;
    let algorithm_handle = AlgorithmHandle(handle);
    let mut digest = [0_u8; 32];
    let input_len = u32::try_from(input.len()).map_err(|_| Status(NTE_BUFFER_TOO_SMALL))?;
    let status = unsafe {
        BCryptHash(
            algorithm_handle.0,
            ptr::null_mut(),
            0,
            input.as_ptr().cast_mut(),
            input_len,
            digest.as_mut_ptr(),
            digest.len() as u32,
        )
    };
    require_success(status)?;
    Ok(digest)
}

struct AlgorithmHandle(usize);

impl Drop for AlgorithmHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = unsafe { BCryptCloseAlgorithmProvider(self.0, 0) };
        }
    }
}

pub(super) struct ProviderHandle(usize);

impl ProviderHandle {
    pub(super) fn open() -> Result<Self, Status> {
        let provider_name = wide(PROVIDER_NAME);
        let mut handle = 0_usize;
        let status = unsafe { NCryptOpenStorageProvider(&mut handle, provider_name.as_ptr(), 0) };
        require_success(status)?;
        Ok(Self(handle))
    }
}

impl Drop for ProviderHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = unsafe { NCryptFreeObject(self.0) };
        }
    }
}

pub(super) struct KeyHandle(usize);

impl KeyHandle {
    pub(super) fn open(provider: &ProviderHandle, key_name: &str) -> Result<Self, Status> {
        let key_name = wide(key_name);
        let mut handle = 0_usize;
        let status = unsafe { NCryptOpenKey(provider.0, &mut handle, key_name.as_ptr(), 0, 0) };
        require_success(status)?;
        Ok(Self(handle))
    }

    pub(super) fn create(provider: &ProviderHandle, key_name: &str) -> Result<Self, Status> {
        let algorithm = wide(ECDSA_P256_ALGORITHM);
        let key_name = wide(key_name);
        let mut handle = 0_usize;
        let status = unsafe {
            NCryptCreatePersistedKey(
                provider.0,
                &mut handle,
                algorithm.as_ptr(),
                key_name.as_ptr(),
                0,
                0,
            )
        };
        require_success(status)?;
        Ok(Self(handle))
    }

    pub(super) fn set_export_policy_disabled(&self) -> Result<(), Status> {
        let property = wide(EXPORT_POLICY_PROPERTY);
        let mut policy = 0_u32;
        let status = unsafe {
            NCryptSetProperty(
                self.0,
                property.as_ptr(),
                (&mut policy as *mut u32).cast::<u8>(),
                4,
                NCRYPT_PERSIST_FLAG,
            )
        };
        require_success(status)
    }

    pub(super) fn finalize(&self) -> Result<(), Status> {
        require_success(unsafe { NCryptFinalizeKey(self.0, 0) })
    }

    pub(super) fn export_policy(&self) -> Result<u32, Status> {
        let property = wide(EXPORT_POLICY_PROPERTY);
        let mut bytes = [0_u8; 4];
        let mut actual = 0_u32;
        let status = unsafe {
            NCryptGetProperty(
                self.0,
                property.as_ptr(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut actual,
                0,
            )
        };
        require_success(status)?;
        if actual != bytes.len() as u32 {
            return Err(Status(NTE_BUFFER_TOO_SMALL));
        }
        Ok(u32::from_le_bytes(bytes))
    }

    pub(super) fn export_public_blob(&self) -> Result<Vec<u8>, Status> {
        self.export_blob(ECC_PUBLIC_BLOB)
    }

    pub(super) fn export_private_blob(&self) -> Result<Vec<u8>, Status> {
        self.export_blob(ECC_PRIVATE_BLOB)
    }

    fn export_blob(&self, blob_type: &str) -> Result<Vec<u8>, Status> {
        let blob_type = wide(blob_type);
        let mut output = [0_u8; MAX_ECC_BLOB_BYTES];
        let mut actual = 0_u32;
        let status = unsafe {
            NCryptExportKey(
                self.0,
                0,
                blob_type.as_ptr(),
                ptr::null_mut(),
                output.as_mut_ptr(),
                output.len() as u32,
                &mut actual,
                0,
            )
        };
        require_success(status)?;
        let actual = usize::try_from(actual).map_err(|_| Status(NTE_BUFFER_TOO_SMALL))?;
        if actual > output.len() {
            return Err(Status(NTE_BUFFER_TOO_SMALL));
        }
        Ok(output[..actual].to_vec())
    }

    pub(super) fn sign_hash(&self, digest: &[u8; 32]) -> Result<Vec<u8>, Status> {
        let mut signature = [0_u8; MAX_SIGNATURE_BYTES];
        let mut actual = 0_u32;
        let status = unsafe {
            NCryptSignHash(
                self.0,
                ptr::null_mut(),
                digest.as_ptr().cast_mut(),
                digest.len() as u32,
                signature.as_mut_ptr(),
                signature.len() as u32,
                &mut actual,
                0,
            )
        };
        require_success(status)?;
        let actual = usize::try_from(actual).map_err(|_| Status(NTE_BUFFER_TOO_SMALL))?;
        if actual > signature.len() {
            return Err(Status(NTE_BUFFER_TOO_SMALL));
        }
        Ok(signature[..actual].to_vec())
    }

    pub(super) fn verify_hash(
        &self,
        digest: &[u8; 32],
        signature: &[u8; 64],
    ) -> Result<bool, Status> {
        let status = unsafe {
            NCryptVerifySignature(
                self.0,
                ptr::null_mut(),
                digest.as_ptr().cast_mut(),
                digest.len() as u32,
                signature.as_ptr().cast_mut(),
                signature.len() as u32,
                0,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(true)
        } else if status == NTE_BAD_SIGNATURE {
            Ok(false)
        } else {
            Err(Status(status))
        }
    }

    pub(super) fn delete(mut self) -> Result<(), Status> {
        let status = unsafe { NCryptDeleteKey(self.0, 0) };
        require_success(status)?;
        self.0 = 0;
        Ok(())
    }
}

impl Drop for KeyHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = unsafe { NCryptFreeObject(self.0) };
        }
    }
}

fn require_success(status: i32) -> Result<(), Status> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Status(status))
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

#[link(name = "ncrypt")]
unsafe extern "system" {
    fn NCryptOpenStorageProvider(
        ph_provider: *mut usize,
        psz_provider_name: *const u16,
        dw_flags: u32,
    ) -> i32;
    fn NCryptOpenKey(
        h_provider: usize,
        ph_key: *mut usize,
        psz_key_name: *const u16,
        dw_legacy_key_spec: u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptCreatePersistedKey(
        h_provider: usize,
        ph_key: *mut usize,
        psz_alg_id: *const u16,
        psz_key_name: *const u16,
        dw_legacy_key_spec: u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptSetProperty(
        h_object: usize,
        psz_property: *const u16,
        pb_input: *mut u8,
        cb_input: u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptGetProperty(
        h_object: usize,
        psz_property: *const u16,
        pb_output: *mut u8,
        cb_output: u32,
        pcb_result: *mut u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptFinalizeKey(h_key: usize, dw_flags: u32) -> i32;
    fn NCryptExportKey(
        h_key: usize,
        h_export_key: usize,
        psz_blob_type: *const u16,
        p_parameter_list: *mut c_void,
        pb_output: *mut u8,
        cb_output: u32,
        pcb_result: *mut u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptSignHash(
        h_key: usize,
        p_padding_info: *mut c_void,
        pb_hash_value: *mut u8,
        cb_hash_value: u32,
        pb_signature: *mut u8,
        cb_signature: u32,
        pcb_result: *mut u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptVerifySignature(
        h_key: usize,
        p_padding_info: *mut c_void,
        pb_hash_value: *mut u8,
        cb_hash_value: u32,
        pb_signature: *mut u8,
        cb_signature: u32,
        dw_flags: u32,
    ) -> i32;
    fn NCryptDeleteKey(h_key: usize, dw_flags: u32) -> i32;
    fn NCryptFreeObject(h_object: usize) -> i32;
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptOpenAlgorithmProvider(
        ph_algorithm: *mut usize,
        psz_alg_id: *const u16,
        psz_implementation: *const u16,
        dw_flags: u32,
    ) -> i32;
    fn BCryptHash(
        h_algorithm: usize,
        pb_secret: *mut u8,
        cb_secret: u32,
        pb_input: *mut u8,
        cb_input: u32,
        pb_output: *mut u8,
        cb_output: u32,
    ) -> i32;
    fn BCryptCloseAlgorithmProvider(h_algorithm: usize, dw_flags: u32) -> i32;
}
