use core::fmt;

#[allow(unsafe_code)]
mod ffi;

pub const P256_SPKI_DER_BYTES: usize = 91;
pub const P1363_SIGNATURE_BYTES: usize = 64;

const P256_PUBLIC_BLOB_BYTES: usize = 72;
const P256_COORDINATE_BYTES: usize = 32;
const BCRYPT_ECDSA_PUBLIC_P256_MAGIC: u32 = 0x3153_4345;
const P256_SPKI_PREFIX: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyDisposition {
    Created,
    OpenedExisting,
}

pub struct PersistedP256Key {
    _provider: ffi::ProviderHandle,
    key: ffi::KeyHandle,
    disposition: KeyDisposition,
}

impl PersistedP256Key {
    pub fn open_or_create(key_name: &str) -> Result<Self, WindowsDeviceKeyError> {
        validate_key_name(key_name)?;
        let provider = ffi::ProviderHandle::open()
            .map_err(|status| WindowsDeviceKeyError::cng("NCryptOpenStorageProvider", status))?;

        let (key, disposition) = match ffi::KeyHandle::open(&provider, key_name) {
            Ok(key) => (key, KeyDisposition::OpenedExisting),
            Err(status) if status.is_bad_keyset() => {
                match ffi::KeyHandle::create(&provider, key_name) {
                    Ok(key) => {
                        key.set_export_policy_disabled().map_err(|status| {
                            WindowsDeviceKeyError::cng("NCryptSetProperty(Export Policy)", status)
                        })?;
                        key.finalize().map_err(|status| {
                            WindowsDeviceKeyError::cng("NCryptFinalizeKey", status)
                        })?;
                        (key, KeyDisposition::Created)
                    }
                    Err(status) if status.is_exists() => {
                        let key = ffi::KeyHandle::open(&provider, key_name).map_err(|status| {
                            WindowsDeviceKeyError::cng("NCryptOpenKey(after NTE_EXISTS)", status)
                        })?;
                        (key, KeyDisposition::OpenedExisting)
                    }
                    Err(status) => {
                        return Err(WindowsDeviceKeyError::cng(
                            "NCryptCreatePersistedKey",
                            status,
                        ));
                    }
                }
            }
            Err(status) => {
                return Err(WindowsDeviceKeyError::cng("NCryptOpenKey", status));
            }
        };

        let result = Self {
            _provider: provider,
            key,
            disposition,
        };
        result.require_non_exportable()?;
        result.public_key_spki_der()?;
        Ok(result)
    }

    #[must_use]
    pub const fn disposition(&self) -> KeyDisposition {
        self.disposition
    }

    pub fn public_key_spki_der(&self) -> Result<[u8; P256_SPKI_DER_BYTES], WindowsDeviceKeyError> {
        let blob = self
            .key
            .export_public_blob()
            .map_err(|status| WindowsDeviceKeyError::cng("NCryptExportKey(public)", status))?;
        if blob.len() != P256_PUBLIC_BLOB_BYTES {
            return Err(WindowsDeviceKeyError::UnexpectedPublicBlob);
        }
        let magic = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        let coordinate_bytes = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        if magic != BCRYPT_ECDSA_PUBLIC_P256_MAGIC
            || coordinate_bytes != P256_COORDINATE_BYTES as u32
        {
            return Err(WindowsDeviceKeyError::UnexpectedPublicBlob);
        }

        let mut spki = [0_u8; P256_SPKI_DER_BYTES];
        spki[..P256_SPKI_PREFIX.len()].copy_from_slice(&P256_SPKI_PREFIX);
        spki[P256_SPKI_PREFIX.len()..].copy_from_slice(&blob[8..]);
        Ok(spki)
    }

    pub fn sign_sha256_message(
        &self,
        message: &[u8],
    ) -> Result<[u8; P1363_SIGNATURE_BYTES], WindowsDeviceKeyError> {
        let digest = ffi::sha256(message)
            .map_err(|status| WindowsDeviceKeyError::cng("BCryptHash(SHA256)", status))?;
        self.sign_sha256_digest(&digest)
    }

    pub fn verify_sha256_message(
        &self,
        message: &[u8],
        signature: &[u8; P1363_SIGNATURE_BYTES],
    ) -> Result<bool, WindowsDeviceKeyError> {
        let digest = ffi::sha256(message)
            .map_err(|status| WindowsDeviceKeyError::cng("BCryptHash(SHA256)", status))?;
        self.verify_sha256_digest(&digest, signature)
    }

    pub fn sign_sha256_digest(
        &self,
        digest: &[u8; 32],
    ) -> Result<[u8; P1363_SIGNATURE_BYTES], WindowsDeviceKeyError> {
        let signature = self
            .key
            .sign_hash(digest)
            .map_err(|status| WindowsDeviceKeyError::cng("NCryptSignHash", status))?;
        let actual = signature.len();
        let signature: [u8; P1363_SIGNATURE_BYTES] = signature
            .try_into()
            .map_err(|_| WindowsDeviceKeyError::UnexpectedSignatureLength { actual })?;
        Ok(signature)
    }

    pub fn verify_sha256_digest(
        &self,
        digest: &[u8; 32],
        signature: &[u8; P1363_SIGNATURE_BYTES],
    ) -> Result<bool, WindowsDeviceKeyError> {
        self.key
            .verify_hash(digest, signature)
            .map_err(|status| WindowsDeviceKeyError::cng("NCryptVerifySignature", status))
    }

    pub fn private_key_export_blocked(&self) -> Result<bool, WindowsDeviceKeyError> {
        let policy = self.key.export_policy().map_err(|status| {
            WindowsDeviceKeyError::cng("NCryptGetProperty(Export Policy)", status)
        })?;
        if policy != 0 {
            return Err(WindowsDeviceKeyError::UnexpectedExportPolicy { actual: policy });
        }
        Ok(self.key.export_private_blob().is_err())
    }

    pub fn require_non_exportable(&self) -> Result<(), WindowsDeviceKeyError> {
        if self.private_key_export_blocked()? {
            Ok(())
        } else {
            Err(WindowsDeviceKeyError::PrivateKeyExportable)
        }
    }

    pub fn delete(self) -> Result<(), WindowsDeviceKeyError> {
        self.key
            .delete()
            .map_err(|status| WindowsDeviceKeyError::cng("NCryptDeleteKey", status))
    }
}

fn validate_key_name(key_name: &str) -> Result<(), WindowsDeviceKeyError> {
    if key_name.is_empty()
        || key_name.len() > 128
        || !key_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(WindowsDeviceKeyError::InvalidKeyName);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDeviceKeyError {
    InvalidKeyName,
    Cng {
        operation: &'static str,
        status: i32,
    },
    UnexpectedExportPolicy {
        actual: u32,
    },
    UnexpectedPublicBlob,
    PrivateKeyExportable,
    UnexpectedSignatureLength {
        actual: usize,
    },
}

impl WindowsDeviceKeyError {
    const fn cng(operation: &'static str, status: ffi::Status) -> Self {
        Self::Cng {
            operation,
            status: status.code(),
        }
    }
}

impl fmt::Display for WindowsDeviceKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyName => formatter.write_str("Windows device key name is invalid"),
            Self::Cng { operation, status } => {
                write!(
                    formatter,
                    "{operation} failed with CNG status 0x{:08X}",
                    *status as u32
                )
            }
            Self::UnexpectedExportPolicy { actual } => {
                write!(
                    formatter,
                    "Windows device key export policy is not disabled: 0x{actual:08X}"
                )
            }
            Self::UnexpectedPublicBlob => {
                formatter.write_str("Windows device key public blob is not canonical P-256 ECDSA")
            }
            Self::PrivateKeyExportable => {
                formatter.write_str("Windows device private key export unexpectedly succeeded")
            }
            Self::UnexpectedSignatureLength { actual } => {
                write!(
                    formatter,
                    "Windows device signature length is {actual}, expected 64"
                )
            }
        }
    }
}

impl std::error::Error for WindowsDeviceKeyError {}
