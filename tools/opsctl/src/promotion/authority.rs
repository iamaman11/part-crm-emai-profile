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

#[derive(Debug, Clone)]
struct RawClosure {
    closure_id: String,
    extends: Option<String>,
    required_components: BTreeSet<String>,
    required_bindings: BTreeSet<String>,
    required_resources: BTreeSet<String>,
    required_credentials: BTreeSet<String>,
    optional_or_disabled_resources: BTreeSet<String>,
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

    let closures = array(
        required(root_object, "deployment_closures")?,
        "deployment_closures",
    )?;
    let mut by_id = BTreeMap::new();
    for value in closures {
        let item = object(value, "deployment closure")?;
        reject_unknown_fields(
            item,
            &[
                "closure_id",
                "profile_id",
                "extends",
                "required_components",
                "required_bindings",
                "required_resources",
                "required_credentials",
                "optional_or_disabled_resources",
                "binding_probe_scope",
            ],
        )?;
        let closure_id = required_string(item, "closure_id")?;
        let bound_profile = required_string(item, "profile_id")?;
        if closure_id.trim().is_empty()
            || bound_profile != closure_id
            || by_id.contains_key(&closure_id)
        {
            return Err(ReleaseModelError::new(
                "deployment closure/profile IDs must be unique, equal and non-empty",
            ));
        }
        if required_string(item, "binding_probe_scope")? != "REQUIRED_CLOSURE_ONLY" {
            return Err(ReleaseModelError::new(
                "deployment closure binding probe scope must remain REQUIRED_CLOSURE_ONLY",
            ));
        }
        by_id.insert(
            closure_id.clone(),
            RawClosure {
                closure_id,
                extends: optional_string(item, "extends")?,
                required_components: optional_string_set(item, "required_components")?,
                required_bindings: optional_string_set(item, "required_bindings")?,
                required_resources: optional_string_set(item, "required_resources")?,
                required_credentials: optional_string_set(item, "required_credentials")?,
                optional_or_disabled_resources: optional_string_set(
                    item,
                    "optional_or_disabled_resources",
                )?,
            },
        );
    }

    let mut visiting = BTreeSet::new();
    resolve_closure(profile_id, &by_id, &mut visiting)
}

fn resolve_closure(
    closure_id: &str,
    by_id: &BTreeMap<String, RawClosure>,
    visiting: &mut BTreeSet<String>,
) -> Result<DeploymentClosure, ReleaseModelError> {
    let raw = by_id.get(closure_id).ok_or_else(|| {
        ReleaseModelError::new(format!(
            "PROFILE_NOT_AUTHORIZED: no deployment closure for {closure_id}"
        ))
    })?;
    if !visiting.insert(closure_id.to_owned()) {
        return Err(ReleaseModelError::new(format!(
            "deployment closure inheritance cycle at {closure_id}"
        )));
    }

    let mut result = match raw.extends.as_deref() {
        Some(parent) => resolve_closure(parent, by_id, visiting)?,
        None => DeploymentClosure {
            closure_id: raw.closure_id.clone(),
            required_components: BTreeSet::new(),
            required_bindings: BTreeSet::new(),
            required_resources: BTreeSet::new(),
            required_credentials: BTreeSet::new(),
            optional_or_disabled_resources: BTreeSet::new(),
        },
    };
    visiting.remove(closure_id);

    result.closure_id = raw.closure_id.clone();
    result
        .required_components
        .extend(raw.required_components.iter().cloned());
    result
        .required_bindings
        .extend(raw.required_bindings.iter().cloned());
    result
        .required_resources
        .extend(raw.required_resources.iter().cloned());
    result
        .required_credentials
        .extend(raw.required_credentials.iter().cloned());
    result
        .optional_or_disabled_resources
        .extend(raw.optional_or_disabled_resources.iter().cloned());
    for resource in &result.required_resources {
        result.optional_or_disabled_resources.remove(resource);
    }
    Ok(result)
}

fn optional_string_set(
    object: &Map<String, Value>,
    field: &str,
) -> Result<BTreeSet<String>, ReleaseModelError> {
    let Some(value) = object.get(field) else {
        return Ok(BTreeSet::new());
    };
    let values = array(value, field)?;
    let mut result = BTreeSet::new();
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| ReleaseModelError::new(format!("{field} must contain strings")))?;
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

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ReleaseModelError> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string"))),
    }
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

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown deployment closure field: {key}"
            )));
        }
    }
    Ok(())
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
        assert!(
            !closure
                .required_bindings
                .contains("MAILBOX_SECRET_RESOLVER")
        );
        assert!(
            !closure
                .required_credentials
                .contains("MAILBOX_RESOLVER_CALLER_AUTH_KEY")
        );
        assert!(
            closure
                .optional_or_disabled_resources
                .contains("mailbox_jobs")
        );
        Ok(())
    }

    #[test]
    fn mailbox_jobs_closure_inherits_core_and_mail_dependencies()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let closure = load_closure(&root, "production-mailbox-jobs-v1")?;
        assert!(closure.required_bindings.contains("CATALOG_DB"));
        assert!(
            closure
                .required_bindings
                .contains("MAILBOX_SECRET_RESOLVER")
        );
        assert!(closure.required_bindings.contains("MAILBOX_JOBS"));
        assert!(closure.required_resources.contains("mailbox_jobs"));
        assert!(
            !closure
                .optional_or_disabled_resources
                .contains("mailbox_jobs")
        );
        Ok(())
    }
}
