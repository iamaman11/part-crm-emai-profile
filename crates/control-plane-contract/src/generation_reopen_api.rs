use core::fmt;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-reopen/download-capability";
pub const BRIDGE_PROFILE_GENERATION_OPENING_MATERIAL_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-reopen/opening-material";

/// Machine request for authoritative reopen. Generation identity and object metadata are
/// intentionally absent: the server re-reads the one active VERIFIED generation from Catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationDownloadCapabilityRequest {
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl BridgeGenerationDownloadCapabilityRequest {
    #[must_use]
    pub fn new(
        coordinator_session_id: impl Into<String>,
        coordinator_fencing_token: impl Into<String>,
        coordinator_epoch: u64,
    ) -> Self {
        Self {
            coordinator_session_id: coordinator_session_id.into(),
            coordinator_fencing_token: coordinator_fencing_token.into(),
            coordinator_epoch,
        }
    }

    #[must_use]
    pub fn coordinator_session_id(&self) -> &str {
        &self.coordinator_session_id
    }

    #[must_use]
    pub fn coordinator_fencing_token(&self) -> &str {
        &self.coordinator_fencing_token
    }

    #[must_use]
    pub const fn coordinator_epoch(&self) -> u64 {
        self.coordinator_epoch
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationDownloadCapabilityResponse {
    pub generation_id: String,
    pub object_key: String,
    pub metadata_digest: String,
    pub container_digest: String,
    pub container_bytes: u64,
    pub method: String,
    pub url: String,
    pub expires_seconds: u32,
}

impl BridgeGenerationDownloadCapabilityResponse {
    #[must_use]
    pub fn new(
        generation_id: impl Into<String>,
        object_key: impl Into<String>,
        metadata_digest: impl Into<String>,
        container_digest: impl Into<String>,
        container_bytes: u64,
        url: impl Into<String>,
        expires_seconds: u32,
    ) -> Self {
        Self {
            generation_id: generation_id.into(),
            object_key: object_key.into(),
            metadata_digest: metadata_digest.into(),
            container_digest: container_digest.into(),
            container_bytes,
            method: "GET".to_owned(),
            url: url.into(),
            expires_seconds,
        }
    }
}

/// Content evidence for opening the server-selected active generation. The caller cannot provide
/// generation/key/digest selectors independently: it sends only the exact bounded canonical BPGC
/// metadata prelude already obtained from the descriptor-bound immutable object plus the live
/// coordinator witness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationOpeningMaterialRequest {
    metadata_prelude_hex: String,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl BridgeGenerationOpeningMaterialRequest {
    #[must_use]
    pub fn new(
        metadata_prelude_hex: impl Into<String>,
        coordinator_session_id: impl Into<String>,
        coordinator_fencing_token: impl Into<String>,
        coordinator_epoch: u64,
    ) -> Self {
        Self {
            metadata_prelude_hex: metadata_prelude_hex.into(),
            coordinator_session_id: coordinator_session_id.into(),
            coordinator_fencing_token: coordinator_fencing_token.into(),
            coordinator_epoch,
        }
    }

    #[must_use]
    pub fn metadata_prelude_hex(&self) -> &str {
        &self.metadata_prelude_hex
    }

    #[must_use]
    pub fn coordinator_session_id(&self) -> &str {
        &self.coordinator_session_id
    }

    #[must_use]
    pub fn coordinator_fencing_token(&self) -> &str {
        &self.coordinator_fencing_token
    }

    #[must_use]
    pub const fn coordinator_epoch(&self) -> u64 {
        self.coordinator_epoch
    }
}

/// Ephemeral historical generation DEK. The response is machine-only, redacted from Debug and
/// zeroized on drop. Key identity is returned only so the Bridge can compare it to the canonical
/// inspected prelude before opening the immutable container.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationOpeningMaterialResponse {
    key_id: String,
    dek_hex: String,
}

impl BridgeGenerationOpeningMaterialResponse {
    #[must_use]
    pub fn new(key_id: impl Into<String>, dek_hex: impl Into<String>) -> Self {
        Self {
            key_id: key_id.into(),
            dek_hex: dek_hex.into(),
        }
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    #[must_use]
    pub fn dek_hex(&self) -> &str {
        &self.dek_hex
    }

    #[must_use]
    pub fn into_parts(mut self) -> (String, Zeroizing<String>) {
        let key_id = core::mem::take(&mut self.key_id);
        let dek_hex = Zeroizing::new(core::mem::take(&mut self.dek_hex));
        (key_id, dek_hex)
    }
}

impl fmt::Debug for BridgeGenerationOpeningMaterialResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeGenerationOpeningMaterialResponse")
            .field("key_id", &self.key_id)
            .field("dek_hex", &"[REDACTED]")
            .finish()
    }
}

impl Drop for BridgeGenerationOpeningMaterialResponse {
    fn drop(&mut self) {
        self.dek_hex.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
        BridgeGenerationOpeningMaterialRequest, BridgeGenerationOpeningMaterialResponse,
    };

    #[test]
    fn request_contains_no_generation_or_object_selection() -> Result<(), Box<dyn std::error::Error>>
    {
        let request = BridgeGenerationDownloadCapabilityRequest::new(
            "session_reopen_01",
            "fence_reopen_01",
            4,
        );
        let value = serde_json::to_value(&request)?;
        assert_eq!(value["coordinatorSessionId"], "session_reopen_01");
        for forbidden in [
            "tenantId",
            "profileId",
            "deviceId",
            "generationId",
            "objectKey",
            "metadataDigest",
            "containerDigest",
            "containerBytes",
            "rootKeyVersion",
            "serverVersion",
            "clientClock",
        ] {
            assert!(
                value.get(forbidden).is_none(),
                "forbidden selection field: {forbidden}"
            );
        }
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_generation_selection_fields() {
        for body in [
            r#"{"coordinatorSessionId":"session_reopen_01","coordinatorFencingToken":"fence_reopen_01","coordinatorEpoch":4,"generationId":"generation_stale_01"}"#,
            r#"{"coordinatorSessionId":"session_reopen_01","coordinatorFencingToken":"fence_reopen_01","coordinatorEpoch":4,"clientClock":123}"#,
        ] {
            assert!(
                serde_json::from_str::<BridgeGenerationDownloadCapabilityRequest>(body).is_err()
            );
        }
    }

    #[test]
    fn response_is_explicit_exact_descriptor_and_get_capability()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = BridgeGenerationDownloadCapabilityResponse::new(
            "generation_reopen_01",
            "tenants/tenant_reopen_01/profiles/profile_reopen_01/generations/generation_reopen_01.bpgc",
            "a".repeat(64),
            "b".repeat(64),
            4096,
            "https://example.invalid/signed",
            300,
        );
        let value = serde_json::to_value(&response)?;
        assert_eq!(value["method"], "GET");
        assert_eq!(value["containerBytes"], 4096);
        assert_eq!(value["generationId"], "generation_reopen_01");
        Ok(())
    }

    #[test]
    fn opening_request_has_only_prelude_and_live_authority_witness()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = BridgeGenerationOpeningMaterialRequest::new(
            "42504743303030310000000100",
            "session_reopen_open_01",
            "fence_reopen_open_01",
            7,
        );
        let value = serde_json::to_value(&request)?;
        assert!(value.get("metadataPreludeHex").is_some());
        assert_eq!(value["coordinatorEpoch"], 7);
        for forbidden in [
            "tenantId",
            "profileId",
            "deviceId",
            "generationId",
            "keyId",
            "plaintextDigest",
            "noncePrefix",
            "metadataDigest",
            "containerDigest",
            "rootKeyVersion",
            "serverVersion",
            "clientClock",
        ] {
            assert!(
                value.get(forbidden).is_none(),
                "opening request accepted caller selector: {forbidden}"
            );
        }
        Ok(())
    }

    #[test]
    fn opening_request_rejects_independent_crypto_selectors() {
        let base = r#"{"metadataPreludeHex":"42504743303030310000000100","coordinatorSessionId":"session_reopen_open_01","coordinatorFencingToken":"fence_reopen_open_01","coordinatorEpoch":7}"#;
        assert!(serde_json::from_str::<BridgeGenerationOpeningMaterialRequest>(base).is_ok());
        for forbidden in ["generationId", "keyId", "plaintextDigest", "noncePrefix", "metadataDigest"] {
            let tampered = base.replacen('}', &format!(r#", "{forbidden}": "x"}}"#), 1);
            assert!(serde_json::from_str::<BridgeGenerationOpeningMaterialRequest>(&tampered).is_err());
        }
    }

    #[test]
    fn opening_secret_response_is_redacted_and_zeroizing() {
        let response = BridgeGenerationOpeningMaterialResponse::new(
            "profile-generation-root-v1-1",
            "ab".repeat(32),
        );
        let debug = format!("{response:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&"ab".repeat(32)));
        let (key_id, dek_hex) = response.into_parts();
        assert_eq!(key_id, "profile-generation-root-v1-1");
        assert_eq!(dek_hex.as_str(), "ab".repeat(32));
    }
}
