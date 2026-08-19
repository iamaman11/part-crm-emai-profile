use crate::release::model::ReleaseModelError;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const AUTHORITY_PATH: &str = "architecture/release-architecture-ar11.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentClosure {
    pub closure_id: String,
    pub required_components: BTreeSet<String>,
    pub required_bindings: BTreeSet<String>,
    pub required_resources: BTreeSet<String>,
    pub required_credentials: BTreeSet<String>,
    pub optional_or_disabled_resources: BTreeSet<String>,
}

pub fn load_closure(root: &Path, profile_id: &str) -> Result<DeploymentClosure, ReleaseModelError> {
    let path = root.join(AUTHORITY_PATH);
    let input = fs::read_to_string(&path).map_err(|error| {
        ReleaseModelError::new(format!(
            "release architecture unavailable at {}: {error}",
            path.display()
        ))
    })?;
    let value: Value = serde_json::from_str(&input).map_err(|error| {
        ReleaseModelError::new(format!("invalid release architecture JSON: {error}"))
    })?;
    let root_object = object(&value, "release architecture")?;
    if required_u64(root_object, "schema_version")? != 1
        || required_string(root_object, "kind")? != "AR11_RELEASE_ARCHITECTURE_SOURCE"
        || required_string(root_object, "canonical_projection")?
            != "architecture/inventory.json::release_architecture"
    {
        return Err(ReleaseModelError::new(
            "release architecture identity/version drifted",
        ));
    }
    if required_bool(root_object, "production_mutation")?
        || required_bool(root_object, "architecture_complete")?
        || required_bool(root_object, "production_ready")?
        || required_string(root_object, "production_core_gate")? != "BLOCKED"
    {
        return Err(ReleaseModelError::new(
            "AR-11 release architecture may not authorize production",
        ));
    }

    let closures = array(required(root_object, "deployment_closures")?, "deployment_closures")?;
    let mut by_id = BTreeMap::new();
    for value in closures {
        let item = object(value, "deployment closure")?;
        let closure_id = required_string(item, "closure_id")?;
        if closure_id.trim().is_empty() || by_id.contains_key(&closure_id) {
            return Err(ReleaseModelError::new(
                "deployment closure IDs must be unique and non-empty",
            ));
        }
        by_id.insert(
            closure_id.clone(),
            DeploymentClosure {
                closure_id,
                required_components: string_set(item, "required_components")?,
                required_bindings: string_set(item, "required_bindings")?,
                required_resources: string_set(item, "required_resources")?,
                required_credentials: string_set(item, "required_credentials")?,
                optional_or_disabled_resources: string_set(
                    item,
                    "optional_or_disabled_resources",
                )?,
            },
        );
    }
    by_id.remove(profile_id).ok_or_else(|| {
        ReleaseModelError::new(format!(
            "PROFILE_NOT_AUTHORIZED: no deployment closure for {profile_id}"
        ))
    })
}

fn string_set(
    object: &Map<String, Value>,
    field: &str,
) -> Result<BTreeSet<String>, ReleaseModelError> {
    let values = array(required(object, field)?, field)?;
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
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("missing release architecture field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?
        .as_bool()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a boolean")))
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be an object")))
}

fn array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be an array")))
}

#[cfg(test)]
mod tests {
    use super::load_closure;
    use std::path::PathBuf;

    #[test]
    fn production_core_closure_excludes_mail_operational_dependencies()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let closure = load_closure(&root, "production-core-v1")?;
        assert!(!closure.required_bindings.contains("MAILBOX_JOBS"));
        assert!(!closure.required_bindings.contains("MAILBOX_SECRET_RESOLVER"));
        assert!(!closure
            .required_credentials
            .contains("MAILBOX_RESOLVER_CALLER_AUTH_KEY"));
        assert!(closure
            .optional_or_disabled_resources
            .contains("mailbox_jobs"));
        Ok(())
    }
}
