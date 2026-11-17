#![cfg(windows)]
#![forbid(unsafe_code)]

// One signed read-only R2 GET. This adapter does NOT select an object,
// handle a key, or publish Firefox bytes. It streams bytes into an unpublished
// bounded writer owned by generation_reopen, which independently checks
// HTTP 200, exact authoritative length, SHA-256 and fsync before use.
use crate::generation_reopen::SignedGenerationObjectStreamGetPort;
use crate::windows_generation_put::valid_signed_r2_url;
use bridge_domain::BridgePortError;
use encrypted_generation_domain::MAX_STREAM_CONTAINER_BYTES;
use std::io::Write;

/// Signed GET URLs are short-lived bearer capabilities and must never appear
/// in logs or Debug. The native client owns the direct HTTPS request.
#[derive(Clone)]
pub struct WindowsSignedGenerationObjectGet {}

impl WindowsSignedGenerationObjectGet {
    pub fn from_system() -> Result<Self, BridgePortError> {
        Ok(Self {})
    }
}

impl SignedGenerationObjectStreamGetPort for WindowsSignedGenerationObjectGet {
    type Error = BridgePortError;

    fn get_exact_stream(
        &mut self,
        signed_url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<u16, Self::Error> {
        if max_bytes == 0
            || max_bytes > MAX_STREAM_CONTAINER_BYTES
            || !valid_signed_r2_url(signed_url)
        {
            return Err(BridgePortError::InvalidResponse);
        }
        use crate::native_transport::{
            HttpFailure, HttpOperation, completed, failure, note_failure,
        };
        let transport = crate::native_transport::shared()?;
        let started = std::time::Instant::now();
        transport.runtime.block_on(async {
            let mut response = transport
                .client
                .get(signed_url)
                .timeout(std::time::Duration::from_secs(240))
                .send()
                .await
                .map_err(|e| failure(HttpOperation::GenerationGet, None, &e, started))?;
            let status = response.status().as_u16();
            let mut total = 0_u64;
            if status != 200 {
                completed(HttpOperation::GenerationGet, None, status, 0, started);
                return Ok(status);
            }
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| failure(HttpOperation::GenerationGet, None, &e, started))?
            {
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or(BridgePortError::InvalidResponse)?;
                if total > max_bytes {
                    return Err(note_failure(
                        HttpOperation::GenerationGet,
                        None,
                        HttpFailure::Size,
                        Some(status),
                        Some(total),
                        started,
                    )
                    .into());
                }
                sink.write_all(&chunk).map_err(|_| {
                    let _ = note_failure(
                        HttpOperation::GenerationGet,
                        None,
                        HttpFailure::Disk,
                        Some(status),
                        Some(total),
                        started,
                    );
                    BridgePortError::InvalidResponse
                })?;
            }
            completed(HttpOperation::GenerationGet, None, status, total, started);
            Ok(status)
        })
    }
}
