use super::model::D1Error;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn ensure_unique(values: &[String], label: &str) -> Result<(), D1Error> {
    let mut observed = HashSet::with_capacity(values.len());
    for value in values {
        if !observed.insert(value.as_str()) {
            return Err(D1Error::new(format!(
                "{label} contains duplicate entry: {value}"
            )));
        }
    }
    Ok(())
}

pub(super) fn required_value_string(
    value: &Value,
    key: &str,
    label: &str,
) -> Result<String, D1Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("{label} {key} is missing")))
}

pub(super) fn required_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, D1Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("required string field {key} is missing")))
}

pub(super) fn required_bool(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<bool, D1Error> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| D1Error::new(format!("required boolean field {key} is missing")))
}

pub(super) fn required_string_array(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, D1Error> {
    let values = object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new(format!("required array field {key} is missing")))?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let item = value
            .as_str()
            .filter(|item| !item.is_empty())
            .ok_or_else(|| D1Error::new(format!("{key} must contain non-empty strings")))?;
        result.push(item.to_owned());
    }
    ensure_unique(&result, key)?;
    Ok(result)
}

pub(super) fn resolve_input(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

pub(super) fn read_json(path: &Path, label: &str) -> Result<Value, D1Error> {
    let text = fs::read_to_string(path).map_err(|error| {
        D1Error::new(format!("cannot read {label} {}: {error}", path.display()))
    })?;
    serde_json::from_str(&text)
        .map_err(|error| D1Error::new(format!("cannot parse {label} {}: {error}", path.display())))
}
