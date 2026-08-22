use super::ReleaseFinalizeError;
use crate::canonical::{
    DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_JSON_DEPTH, parse_strict_json_with_limits,
};
use serde::Deserialize;
use std::collections::BTreeMap;

const TRANSPORT_SCHEMA_VERSION: u64 = 1;
const TRANSPORT_KIND: &str = "RELEASE_FINALIZE_REQUEST";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReleaseFinalizeRequestV1 {
    pub(super) schema_version: u64,
    kind: String,
    pub(super) source: SourceObservationV1,
    pub(super) components: BTreeMap<String, ComponentObservationV1>,
    pub(super) protocols: ProtocolObservationV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceObservationV1 {
    pub(super) repository: String,
    pub(super) commit_sha: String,
    pub(super) accepted_main: bool,
    pub(super) accepted_main_evidence_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentObservationV1 {
    pub(super) component_id: String,
    pub(super) release_id: String,
    pub(super) source_commit_sha: String,
    pub(super) artifact_path: String,
    pub(super) artifact_sha256: String,
    pub(super) artifact_size_bytes: u64,
    pub(super) component_manifest_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProtocolObservationV1 {
    pub(super) profile_bridge_protocol_version: u64,
    pub(super) resolver_protocol: String,
}

pub(super) fn parse(input: &str) -> Result<ReleaseFinalizeRequestV1, ReleaseFinalizeError> {
    let value =
        parse_strict_json_with_limits(input, DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_JSON_DEPTH)
            .map_err(|error| {
                ReleaseFinalizeError::new(format!("invalid ReleaseFinalizeRequestV1 JSON: {error}"))
            })?;
    let request: ReleaseFinalizeRequestV1 = serde_json::from_value(value).map_err(|error| {
        ReleaseFinalizeError::new(format!("invalid ReleaseFinalizeRequestV1 DTO: {error}"))
    })?;
    if request.schema_version != TRANSPORT_SCHEMA_VERSION || request.kind != TRANSPORT_KIND {
        return Err(ReleaseFinalizeError::new(format!(
            "unsupported ReleaseFinalizeRequest transport: kind={} schema_version={}",
            request.kind, request.schema_version
        )));
    }
    Ok(request)
}
