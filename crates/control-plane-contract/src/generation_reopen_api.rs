use serde::{Deserialize, Serialize};

pub const BRIDGE_PROFILE_GENERATION_DOWNLOAD_CAPABILITY_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-reopen/download-capability";

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

#[cfg(test)]
mod tests {
    use super::{
        BridgeGenerationDownloadCapabilityRequest, BridgeGenerationDownloadCapabilityResponse,
    };

    #[test]
    fn request_contains_no_generation_or_object_selection()
    -> Result<(), Box<dyn std::error::Error>> {
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
            assert!(value.get(forbidden).is_none(), "forbidden selection field: {forbidden}");
        }
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_generation_selection_fields() {
        for body in [
            r#"{"coordinatorSessionId":"session_reopen_01","coordinatorFencingToken":"fence_reopen_01","coordinatorEpoch":4,"generationId":"generation_stale_01"}"#,
            r#"{"coordinatorSessionId":"session_reopen_01","coordinatorFencingToken":"fence_reopen_01","coordinatorEpoch":4,"clientClock":123}"#,
        ] {
            assert!(serde_json::from_str::<BridgeGenerationDownloadCapabilityRequest>(body).is_err());
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
}
