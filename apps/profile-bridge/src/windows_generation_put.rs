#![cfg(windows)]
#![forbid(unsafe_code)]

use crate::generation_object_spool::VerifiedGenerationObjectFile;
use crate::shipping_generation_save::{
    SignedGenerationObjectPutPort, SignedGenerationObjectStreamPutPort,
    SignedGenerationUploadCapability,
};
use bridge_domain::BridgePortError;
use encrypted_generation_domain::{MAX_GENERATION_CONTAINER_BYTES, MAX_STREAM_CONTAINER_BYTES};
use std::collections::BTreeSet;
use std::io::Read;

const MAX_SIGNED_GENERATION_URL_BYTES: usize = 8_192;
const MAX_SIGNED_UPLOAD_HEADERS: usize = 16;
const MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES: usize = 128;
const MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES: usize = 1_024;
const MAX_PUT_RESPONSE_BYTES: usize = 1_024;

/// Effect-only Windows adapter for one server-issued immutable generation PUT capability.
///
/// This adapter owns no R2 credentials, machine certificate, descriptor semantics or verification.
/// It sends exactly the backend-issued signed headers and container bytes only to the canonical
/// direct R2 HTTPS authority, never follows redirects, and treats every non-2xx response as a
/// pre-commit failure. Exact object verification remains server-owned and is required again before
/// successor commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsSignedGenerationObjectPut;

impl WindowsSignedGenerationObjectPut {
    pub fn from_system() -> Result<Self, BridgePortError> {
        Ok(Self)
    }

    fn request(
        &self,
        capability: &SignedGenerationUploadCapability,
        body: reqwest::Body,
        expected: u64,
    ) -> Result<reqwest::RequestBuilder, BridgePortError> {
        if expected == 0 || expected > MAX_STREAM_CONTAINER_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        let url = capability
            .url()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if !valid_signed_r2_url(url) {
            return Err(BridgePortError::InvalidResponse);
        }
        let headers = capability
            .headers()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if headers.is_empty() || headers.len() > MAX_SIGNED_UPLOAD_HEADERS {
            return Err(BridgePortError::InvalidResponse);
        }
        let mut names = BTreeSet::new();
        let mut request = crate::native_transport::shared()?
            .client
            .put(url)
            .timeout(std::time::Duration::from_secs(120))
            .body(body);
        for (name, value) in headers {
            if !valid_header_name(name) || !valid_header_value(value) || !names.insert(name) {
                return Err(BridgePortError::InvalidResponse);
            }
            // Framing is bounded and cannot be overridden by a capability.
            if name == "transfer-encoding"
                || (name == "content-length" && value.parse::<u64>().ok() != Some(expected))
            {
                return Err(BridgePortError::InvalidResponse);
            }
            let mut header = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| BridgePortError::InvalidResponse)?;
            header.set_sensitive(true);
            request = request.header(name, header);
        }
        if !names.contains("content-length") {
            request = request.header(reqwest::header::CONTENT_LENGTH, expected);
        }
        Ok(request)
    }

    fn send(request: reqwest::RequestBuilder) -> Result<(), BridgePortError> {
        let transport = crate::native_transport::shared()?;
        let started = std::time::Instant::now();
        transport.runtime.block_on(async {
            use crate::native_transport::{
                HttpFailure, HttpOperation, completed, failure, note_failure,
            };
            let mut response = request
                .send()
                .await
                .map_err(|e| failure(HttpOperation::GenerationPut, None, &e, started))?;
            let status = response.status().as_u16();
            let mut received = 0usize;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| failure(HttpOperation::GenerationPut, None, &e, started))?
            {
                received = received
                    .checked_add(chunk.len())
                    .ok_or(BridgePortError::InvalidResponse)?;
                if received > MAX_PUT_RESPONSE_BYTES {
                    return Err(note_failure(
                        HttpOperation::GenerationPut,
                        None,
                        HttpFailure::Size,
                        Some(status),
                        Some(received as u64),
                        started,
                    )
                    .into());
                }
            }
            completed(
                HttpOperation::GenerationPut,
                None,
                status,
                received as u64,
                started,
            );
            if !(200..300).contains(&status) || received != 0 {
                return Err(BridgePortError::InvalidResponse);
            }
            Ok(())
        })
    }
}

impl SignedGenerationObjectPutPort for WindowsSignedGenerationObjectPut {
    type Error = BridgePortError;
    fn put_exact(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        container: &[u8],
    ) -> Result<(), Self::Error> {
        if container.is_empty() || container.len() > MAX_GENERATION_CONTAINER_BYTES {
            return Err(BridgePortError::InvalidResponse);
        }
        let request = self.request(
            capability,
            reqwest::Body::from(container.to_vec()),
            container.len() as u64,
        )?;
        Self::send(request)
    }
}

// Same open file identity: never reopen a path. The mutex is a revocation
// barrier: after take(), no body poll can touch the shared file cursor.
struct Source {
    file: std::fs::File,
    remaining: u64,
    failed: bool,
}
type SourceGate = std::sync::Arc<std::sync::Mutex<Option<Source>>>;

fn body_from_gate(gate: SourceGate) -> reqwest::Body {
    let stream = futures_util::stream::poll_fn(move |_| {
        use std::task::Poll;
        let Ok(mut slot) = gate.lock() else {
            return Poll::Ready(Some(Err::<Vec<u8>, _>(std::io::Error::other(
                "source gate poisoned",
            ))));
        };
        let Some(source) = slot.as_mut() else {
            return Poll::Ready(None);
        };
        if source.remaining == 0 || source.failed {
            return Poll::Ready(None);
        }
        let size = source.remaining.min(65536) as usize;
        let mut bytes = vec![0u8; size];
        match source.file.read_exact(&mut bytes) {
            Ok(()) => {
                source.remaining -= size as u64;
                Poll::Ready(Some(Ok(bytes)))
            }
            Err(error) => {
                source.failed = true;
                Poll::Ready(Some(Err(error)))
            }
        }
    });
    reqwest::Body::wrap_stream(stream)
}

impl SignedGenerationObjectStreamPutPort for WindowsSignedGenerationObjectPut {
    fn put_verified_stream(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        object: &mut VerifiedGenerationObjectFile,
    ) -> Result<(), Self::Error> {
        object
            .recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let expected = object.expected_bytes();
        let gate = std::sync::Arc::new(std::sync::Mutex::new(Some(Source {
            file: object
                .reader()
                .try_clone()
                .map_err(|_| BridgePortError::Unavailable)?,
            remaining: expected,
            failed: false,
        })));
        let result = self
            .request(capability, body_from_gate(gate.clone()), expected)
            .and_then(Self::send);
        // Revoke even when send returns early. Acquiring the lock waits for a
        // currently executing synchronous read; no later poll can read the file.
        let source = gate
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?
            .take()
            .ok_or(BridgePortError::Unavailable)?;
        let complete = source.remaining == 0 && !source.failed;
        drop(source);
        object
            .recheck_and_rewind()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        result?;
        if !complete {
            return Err(BridgePortError::InvalidResponse);
        }
        eprintln!("R2_NATIVE_PUT_SOURCE_VERIFIED bytes={expected}");
        Ok(())
    }
}

pub(crate) fn valid_signed_r2_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_SIGNED_GENERATION_URL_BYTES
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.contains('#')
    {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path_and_query)) = rest.split_once('/') else {
        return false;
    };
    const R2_SUFFIX: &str = ".r2.cloudflarestorage.com";
    let Some(account_id) = authority.strip_suffix(R2_SUFFIX) else {
        return false;
    };
    if account_id.len() != 32
        || !account_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || authority.contains('@')
        || authority.contains(':')
    {
        return false;
    }
    let Some((path, query)) = path_and_query.split_once('?') else {
        return false;
    };
    if path.is_empty() || query.is_empty() {
        return false;
    }
    let required = [
        "X-Amz-Algorithm=AWS4-HMAC-SHA256",
        "X-Amz-Credential=",
        "X-Amz-Date=",
        "X-Amz-Expires=",
        "X-Amz-SignedHeaders=",
        "X-Amz-Signature=",
    ];
    required.iter().all(|needle| query.contains(needle))
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES
        && !value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
}

#[cfg(test)]
mod tests {
    use super::{valid_header_name, valid_header_value, valid_signed_r2_url};

    #[test]
    fn signed_put_url_requires_direct_canonical_r2_authority() {
        let good = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/profile-generations/tenants/tenant_01/profiles/profile_01/generations/generation_01.bpgc?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=access%2F20260830%2Fauto%2Fs3%2Faws4_request&X-Amz-Date=20260830T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=content-type%3Bhost&X-Amz-Signature=abc";
        assert!(valid_signed_r2_url(good));
        for bad in [
            good.replacen("https://", "http://", 1),
            good.replacen(".r2.cloudflarestorage.com", ".example.com", 1),
            format!("{good}#fragment"),
            good.replacen("X-Amz-Signature=abc", "signature=abc", 1),
            good.replacen(
                "0123456789abcdef0123456789abcdef",
                "ABCDEF0123456789ABCDEF0123456789",
                1,
            ),
        ] {
            assert!(!valid_signed_r2_url(&bad));
        }
    }

    #[test]
    fn signed_put_headers_reject_injection_and_duplicates_are_checked_by_adapter() {
        assert!(valid_header_name("x-amz-checksum-sha256"));
        assert!(!valid_header_name("X-Amz-Checksum-Sha256"));
        assert!(valid_header_value("abc+/=="));
        assert!(!valid_header_value("value\r\ninjected: true"));
    }
}
