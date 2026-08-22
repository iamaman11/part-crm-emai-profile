use super::ReleaseFinalizeError;
use crate::canonical::{
    DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_JSON_DEPTH, parse_strict_json_with_limits,
};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const RUNTIME_LOCK_SCHEMA_VERSION: u64 = 1;

pub(super) struct RuntimeLockFacts {
    pub(super) camouhost_ipc_version: u64,
    pub(super) runtime_role: String,
    pub(super) profile_format: String,
    pub(super) browser_identity_policy: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeLockV1 {
    schema_version: u64,
    #[serde(rename = "browser")]
    _browser: RuntimeBrowserV1,
    camouhost_ipc_version: u64,
    #[serde(rename = "components")]
    _components: RuntimeComponentsV1,
    fingerprint_config_schema: String,
    fingerprint_policy_version: String,
    #[serde(rename = "python")]
    _python: String,
    #[serde(rename = "python_source")]
    _python_source: RuntimePythonSourceV1,
    runtime_role: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBrowserV1 {
    #[serde(rename = "release_commit")]
    _release_commit: String,
    #[serde(rename = "repository")]
    _repository: String,
    #[serde(rename = "version")]
    _version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeComponentsV1 {
    #[serde(rename = "browserforge")]
    _browserforge: String,
    #[serde(rename = "camoufox_python")]
    _camoufox_python: String,
    #[serde(rename = "playwright")]
    _playwright: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimePythonSourceV1 {
    #[serde(rename = "commit")]
    _commit: String,
    #[serde(rename = "repository")]
    _repository: String,
}

pub(super) fn load(path: &Path) -> Result<RuntimeLockFacts, ReleaseFinalizeError> {
    let input = fs::read_to_string(path).map_err(|error| {
        ReleaseFinalizeError::new(format!("cannot read canonical runtime lock: {error}"))
    })?;
    let value =
        parse_strict_json_with_limits(&input, DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_JSON_DEPTH)
            .map_err(|error| {
                ReleaseFinalizeError::new(format!("invalid runtime lock JSON: {error}"))
            })?;
    let lock: RuntimeLockV1 = serde_json::from_value(value)
        .map_err(|error| ReleaseFinalizeError::new(format!("invalid runtime lock DTO: {error}")))?;
    if lock.schema_version != RUNTIME_LOCK_SCHEMA_VERSION {
        return Err(ReleaseFinalizeError::new(format!(
            "unsupported runtime lock schema_version: {}",
            lock.schema_version
        )));
    }
    Ok(RuntimeLockFacts {
        camouhost_ipc_version: lock.camouhost_ipc_version,
        runtime_role: lock.runtime_role,
        profile_format: lock.fingerprint_config_schema,
        browser_identity_policy: lock.fingerprint_policy_version,
    })
}
