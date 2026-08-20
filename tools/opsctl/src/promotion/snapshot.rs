use crate::release::model::ReleaseModelError;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const DEPLOYMENT_SNAPSHOT_SCHEMA_VERSION: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentSnapshot {
    pub environment: String,
    pub collected_at: String,
    pub release_set_id: Option<String>,
    pub capability_profile_id: Option<String>,
    pub component_release_ids: Vec<(String, String)>,
    pub logical_resources: BTreeSet<String>,
    pub logical_bindings: BTreeSet<String>,
    pub logical_credentials: BTreeSet<String>,
    pub catalog_ledger_sha256: Option<String>,
    pub catalog_schema_revision: Option<String>,
    pub resolver_ledger_sha256: Option<String>,
    pub resolver_schema_revision: Option<String>,
}

impl DeploymentSnapshot {
    pub fn load(path: &Path) -> Result<Self, ReleaseModelError> {
        let input = fs::read_to_string(path).map_err(|error| {
            ReleaseModelError::new(format!(
                "PROVIDER_STATE_UNKNOWN: cannot read snapshot {}: {error}",
                path.display()
            ))
        })?;
        Self::parse_json(&input)
    }

    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!("invalid DeploymentSnapshot JSON: {error}"))
        })?;
        reject_secret_material(&value, "snapshot")?;
        let root = object(&value, "DeploymentSnapshot")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "kind",
                "environment",
                "collected_at",
                "release_set_id",
                "capability_profile_id",
                "component_release_ids",
                "workers",
                "d1",
                "r2",
                "queues",
                "dlqs",
                "durable_objects",
                "service_bindings",
                "routes",
                "schedules",
                "credential_metadata",
                "observed_logical_resources",
                "observed_logical_bindings",
                "observed_logical_credentials",
            ],
            "DeploymentSnapshot",
        )?;
        if required_u64(root, "schema_version")? != DEPLOYMENT_SNAPSHOT_SCHEMA_VERSION
            || required_string(root, "kind")? != "DEPLOYMENT_SNAPSHOT"
        {
            return Err(ReleaseModelError::new(
                "unsupported DeploymentSnapshot identity/version; only v2 is accepted before production",
            ));
        }
        let environment = required_string(root, "environment")?;
        if !matches!(environment.as_str(), "rehearsal" | "staging" | "production") {
            return Err(ReleaseModelError::new(format!(
                "unsupported snapshot environment: {environment}"
            )));
        }
        let collected_at = required_string(root, "collected_at")?;
        if collected_at.trim().is_empty() {
            return Err(ReleaseModelError::new("collected_at must not be empty"));
        }
        let release_set_id = optional_string(root, "release_set_id")?;
        let capability_profile_id = optional_string(root, "capability_profile_id")?;
        let component_release_ids = string_map(root, "component_release_ids")?;
        validate_metadata_arrays(root)?;
        let logical_resources = string_set(root, "observed_logical_resources")?;
        let logical_bindings = string_set(root, "observed_logical_bindings")?;
        let logical_credentials = string_set(root, "observed_logical_credentials")?;
        let d1 = parse_d1(root)?;
        Ok(Self {
            environment,
            collected_at,
            release_set_id,
            capability_profile_id,
            component_release_ids,
            logical_resources,
            logical_bindings,
            logical_credentials,
            catalog_ledger_sha256: d1.catalog_ledger_sha256,
            catalog_schema_revision: d1.catalog_schema_revision,
            resolver_ledger_sha256: d1.resolver_ledger_sha256,
            resolver_schema_revision: d1.resolver_schema_revision,
        })
    }
}

#[derive(Default)]
struct D1State {
    catalog_ledger_sha256: Option<String>,
    catalog_schema_revision: Option<String>,
    resolver_ledger_sha256: Option<String>,
    resolver_schema_revision: Option<String>,
}

fn validate_metadata_arrays(root: &Map<String, Value>) -> Result<(), ReleaseModelError> {
    for field in [
        "workers",
        "r2",
        "queues",
        "dlqs",
        "durable_objects",
        "service_bindings",
        "credential_metadata",
    ] {
        let values = array(required(root, field)?, field)?;
        for value in values {
            if !value.is_object() {
                return Err(ReleaseModelError::new(format!(
                    "{field} entries must be metadata objects"
                )));
            }
        }
    }
    for field in ["routes", "schedules"] {
        let values = array(required(root, field)?, field)?;
        if values.iter().any(|value| value.as_str().is_none()) {
            return Err(ReleaseModelError::new(format!(
                "{field} entries must be strings"
            )));
        }
    }
    Ok(())
}

fn parse_d1(root: &Map<String, Value>) -> Result<D1State, ReleaseModelError> {
    let rows = array(required(root, "d1")?, "d1")?;
    let mut state = D1State::default();
    for row in rows {
        let item = object(row, "d1 entry")?;
        reject_unknown_fields(
            item,
            &["component", "binding", "database_id", "ledger_sha256", "schema_revision"],
            "d1 entry",
        )?;
        let component = required_string(item, "component")?;
        if !matches!(component.as_str(), "catalog" | "resolver") {
            return Err(ReleaseModelError::new(format!(
                "unknown D1 component in snapshot: {component}"
            )));
        }
        let _binding = required_string(item, "binding")?;
        let _database_id = required_string(item, "database_id")?;
        let ledger = required_string(item, "ledger_sha256")?;
        validate_sha256(&ledger, "d1.ledger_sha256")?;
        let revision = required_string(item, "schema_revision")?;
        validate_schema_revision(&revision, "d1.schema_revision")?;
        let (ledger_target, revision_target) = if component == "catalog" {
            (&mut state.catalog_ledger_sha256, &mut state.catalog_schema_revision)
        } else {
            (&mut state.resolver_ledger_sha256, &mut state.resolver_schema_revision)
        };
        if ledger_target.replace(ledger).is_some() || revision_target.replace(revision).is_some() {
            return Err(ReleaseModelError::new(format!("duplicate {component} D1 snapshot")));
        }
    }
    Ok(state)
}

fn reject_secret_material(value: &Value, path: &str) -> Result<(), ReleaseModelError> {
    match value {
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_secret_material(child, &format!("{path}[{index}]"))?;
            }
        }
        Value::Object(values) => {
            for (key, child) in values {
                let normalized = key.to_ascii_lowercase();
                if matches!(
                    normalized.as_str(),
                    "secret" | "secret_value" | "token" | "access_token" | "refresh_token" | "password" | "private_key" | "credential_value"
                ) {
                    return Err(ReleaseModelError::new(format!(
                        "secret material is forbidden in DeploymentSnapshot: {path}.{key}"
                    )));
                }
                reject_secret_material(child, &format!("{path}.{key}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn string_map(root: &Map<String, Value>, field: &str) -> Result<Vec<(String, String)>, ReleaseModelError> {
    let value = object(required(root, field)?, field)?;
    let mut result = Vec::with_capacity(value.len());
    for (key, value) in value {
        let release_id = value.as_str().ok_or_else(|| {
            ReleaseModelError::new(format!("{field}.{key} must be a string"))
        })?;
        if key.trim().is_empty() || release_id.trim().is_empty() {
            return Err(ReleaseModelError::new(format!(
                "{field} keys/values must not be empty"
            )));
        }
        result.push((key.clone(), release_id.to_owned()));
    }
    result.sort();
    Ok(result)
}

fn string_set(root: &Map<String, Value>, field: &str) -> Result<BTreeSet<String>, ReleaseModelError> {
    let values = array(required(root, field)?, field)?;
    let mut result = BTreeSet::new();
    for value in values {
        let text = value.as_str().ok_or_else(|| {
            ReleaseModelError::new(format!("{field} must contain strings"))
        })?;
        if text.trim().is_empty() || !result.insert(text.to_owned()) {
            return Err(ReleaseModelError::new(format!(
                "{field} contains an empty or duplicate value"
            )));
        }
    }
    Ok(result)
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object.get(key).ok_or_else(|| ReleaseModelError::new(format!("missing snapshot field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        ReleaseModelError::new(format!("snapshot field {key} must be a string"))
    })
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, ReleaseModelError> {
    match object.get(key) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => value.as_str().map(|text| Some(text.to_owned())).ok_or_else(|| {
            ReleaseModelError::new(format!("snapshot field {key} must be string/null"))
        }),
    }
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?.as_u64().ok_or_else(|| {
        ReleaseModelError::new(format!("snapshot field {key} must be unsigned integer"))
    })
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value.as_object().ok_or_else(|| ReleaseModelError::new(format!("{label} must be an object")))
}

fn array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value.as_array().ok_or_else(|| ReleaseModelError::new(format!("{label} must be an array")))
}

fn reject_unknown_fields(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!("unknown {label} field: {key}")));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_schema_revision(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() < 9
        || !value.as_bytes()[0..4].iter().all(u8::is_ascii_digit)
        || !value.ends_with(".sql")
        || value.contains(['/', '\\'])
    {
        return Err(ReleaseModelError::new(format!("{field} is not a canonical migration revision")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::DeploymentSnapshot;

    fn fixture() -> String {
        r#"{
          "schema_version":2,
          "kind":"DEPLOYMENT_SNAPSHOT",
          "environment":"staging",
          "collected_at":"2026-08-21T00:00:00Z",
          "release_set_id":null,
          "capability_profile_id":null,
          "component_release_ids":{},
          "workers":[],
          "d1":[{"component":"catalog","binding":"CATALOG_DB","database_id":"db","ledger_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","schema_revision":"0001_initial.sql"}],
          "r2":[],"queues":[],"dlqs":[],"durable_objects":[],"service_bindings":[],"routes":[],"schedules":[],"credential_metadata":[],
          "observed_logical_resources":["catalog_d1"],
          "observed_logical_bindings":["CATALOG_DB"],
          "observed_logical_credentials":[]
        }"#
        .to_owned()
    }

    #[test]
    fn parses_metadata_only_snapshot_v2() -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = DeploymentSnapshot::parse_json(&fixture())?;
        assert_eq!(snapshot.environment, "staging");
        assert_eq!(snapshot.catalog_schema_revision.as_deref(), Some("0001_initial.sql"));
        Ok(())
    }

    #[test]
    fn rejects_v1_snapshot() {
        assert!(DeploymentSnapshot::parse_json(&fixture().replace("\"schema_version\":2", "\"schema_version\":1")).is_err());
    }

    #[test]
    fn rejects_secret_material() {
        let input = fixture().replace(
            "\"credential_metadata\":[]",
            "\"credential_metadata\":[{\"secret_value\":\"forbidden\"}]",
        );
        assert!(DeploymentSnapshot::parse_json(&input).is_err());
    }
}
