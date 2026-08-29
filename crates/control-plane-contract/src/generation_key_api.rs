use core::fmt;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const BRIDGE_GENERATION_SEALING_MATERIAL_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-successor/sealing-material";
pub const GENERATION_SEALING_CHUNK_BYTES: u32 = 65_536;

/// Canonical authenticated machine request for content-bound successor sealing material.
/// Tenant, actor, Profile and device identity are path/auth-derived. Root-key version, key ID,
/// coordinator version/sequence and client clocks are server-owned and intentionally absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationSealingMaterialRequest {
    base_generation_id: String,
    generation_id: String,
    plaintext_digest: String,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl BridgeGenerationSealingMaterialRequest {
    #[must_use]
    pub fn base_generation_id(&self) -> &str {
        &self.base_generation_id
    }

    #[must_use]
    pub fn generation_id(&self) -> &str {
        &self.generation_id
    }

    #[must_use]
    pub fn plaintext_digest(&self) -> &str {
        &self.plaintext_digest
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

/// Ephemeral machine-secret response. `dek_hex` is deliberately private, redacted from Debug and
/// zeroized on drop. It is never a browser/frontend/public DTO.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGenerationSealingMaterialResponse {
    key_id: String,
    dek_hex: String,
    nonce_prefix_hex: String,
    chunk_size: u32,
}

impl BridgeGenerationSealingMaterialResponse {
    #[must_use]
    pub fn new(
        key_id: impl Into<String>,
        dek_hex: impl Into<String>,
        nonce_prefix_hex: impl Into<String>,
        chunk_size: u32,
    ) -> Self {
        Self {
            key_id: key_id.into(),
            dek_hex: dek_hex.into(),
            nonce_prefix_hex: nonce_prefix_hex.into(),
            chunk_size,
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
    pub fn nonce_prefix_hex(&self) -> &str {
        &self.nonce_prefix_hex
    }

    #[must_use]
    pub const fn chunk_size(&self) -> u32 {
        self.chunk_size
    }

    #[must_use]
    pub fn into_parts(mut self) -> (String, Zeroizing<String>, String, u32) {
        let key_id = core::mem::take(&mut self.key_id);
        let dek_hex = Zeroizing::new(core::mem::take(&mut self.dek_hex));
        let nonce_prefix_hex = core::mem::take(&mut self.nonce_prefix_hex);
        (key_id, dek_hex, nonce_prefix_hex, self.chunk_size)
    }
}

impl fmt::Debug for BridgeGenerationSealingMaterialResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeGenerationSealingMaterialResponse")
            .field("key_id", &self.key_id)
            .field("dek_hex", &"[REDACTED]")
            .field("nonce_prefix_hex", &self.nonce_prefix_hex)
            .field("chunk_size", &self.chunk_size)
            .finish()
    }
}

impl Drop for BridgeGenerationSealingMaterialResponse {
    fn drop(&mut self) {
        self.dek_hex.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeGenerationSealingMaterialRequest, BridgeGenerationSealingMaterialResponse,
        GENERATION_SEALING_CHUNK_BYTES,
    };

    #[test]
    fn request_rejects_caller_selected_trusted_state() {
        let base = r#"{
            "baseGenerationId":"generation_key_base_01",
            "generationId":"generation_key_next_01",
            "plaintextDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "coordinatorSessionId":"session_key_01",
            "coordinatorFencingToken":"fence_key_01",
            "coordinatorEpoch":3
        }"#;
        assert!(serde_json::from_str::<BridgeGenerationSealingMaterialRequest>(base).is_ok());
        for forbidden in [
            "tenantId",
            "actorId",
            "profileId",
            "deviceId",
            "rootKeyVersion",
            "keyId",
            "expectedProfileVersion",
            "coordinatorVersion",
            "coordinatorSequence",
            "observedAt",
            "clientClock",
        ] {
            let tampered = base.replacen('}', &format!(r#", "{forbidden}": 1}}"#), 1);
            assert!(
                serde_json::from_str::<BridgeGenerationSealingMaterialRequest>(&tampered).is_err(),
                "caller-selected trusted field unexpectedly accepted: {forbidden}"
            );
        }
    }

    #[test]
    fn secret_response_debug_is_redacted_and_parts_are_zeroizing() {
        let response = BridgeGenerationSealingMaterialResponse::new(
            "profile-generation-root-v1-2",
            "ab".repeat(32),
            "cd".repeat(16),
            GENERATION_SEALING_CHUNK_BYTES,
        );
        let debug = format!("{response:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&"ab".repeat(32)));
        let (key_id, dek_hex, nonce_prefix_hex, chunk_size) = response.into_parts();
        assert_eq!(key_id, "profile-generation-root-v1-2");
        assert_eq!(dek_hex.as_str(), "ab".repeat(32));
        assert_eq!(nonce_prefix_hex, "cd".repeat(16));
        assert_eq!(chunk_size, GENERATION_SEALING_CHUNK_BYTES);
    }
}
