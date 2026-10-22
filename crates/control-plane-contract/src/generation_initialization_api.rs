use serde::{Deserialize, Serialize};

pub const INITIAL_GENERATION_PATH_TEMPLATE: &str =
    "/bridge/v1/tenants/{tenantId}/profiles/{profileId}/generation-initialization";

/// Closed v1 machine contract. Actor/device come only from the signed application session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InitialGenerationRequest {
    Prepare {
        generation_id: String,
        expected_profile_version: u64,
    },
    SealingMaterial {
        generation_id: String,
        expected_profile_version: u64,
        plaintext_digest: String,
    },
    UploadCapability {
        generation_id: String,
        expected_profile_version: u64,
        metadata_digest: String,
        container_digest: String,
        container_bytes: u64,
    },
    Commit {
        generation_id: String,
        expected_profile_version: u64,
        metadata_digest: String,
        container_digest: String,
        container_bytes: u64,
    },
}

impl InitialGenerationRequest {
    #[must_use]
    pub fn generation_id(&self) -> &str {
        match self {
            Self::Prepare { generation_id, .. }
            | Self::SealingMaterial { generation_id, .. }
            | Self::UploadCapability { generation_id, .. }
            | Self::Commit { generation_id, .. } => generation_id,
        }
    }
    #[must_use]
    pub const fn expected_profile_version(&self) -> u64 {
        match self {
            Self::Prepare {
                expected_profile_version,
                ..
            }
            | Self::SealingMaterial {
                expected_profile_version,
                ..
            }
            | Self::UploadCapability {
                expected_profile_version,
                ..
            }
            | Self::Commit {
                expected_profile_version,
                ..
            } => *expected_profile_version,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InitialGenerationPrepared {
    Initialize {
        generation_id: String,
        expected_profile_version: u64,
    },
    Launch {
        launch: crate::profile_launch_api::ProfileLaunchProjection,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitialGenerationCommitted {
    pub launch: crate::profile_launch_api::ProfileLaunchProjection,
    pub generation_id: String,
    pub profile_version: u64,
    pub replayed: bool,
}
