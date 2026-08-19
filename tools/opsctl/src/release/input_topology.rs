use crate::release::authority::DEFAULT_AUTHORITY_PATH;
use crate::release::digest::sha256_reader_hex;
use crate::release::model::ReleaseModelError;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

const SUPPORTED_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInput {
    pub input_id: String,
    pub kind: String,
    pub semantic_owner: String,
    pub canonical_source: Option<String>,
    pub generated_projection: Option<String>,
    pub release_identity_source: String,
    pub compatibility_dimension: String,
    pub required_for_release_set: bool,
    pub generator: Option<String>,
    pub verification: Vec<String>,
    pub consumers: Vec<String>,
}

impl ReleaseInput {
    #[must_use]
    pub fn consumed_by(&self, consumer: &str) -> bool {
        self.consumers.iter().any(|value| value == consumer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReleaseInput {
    pub input: ReleaseInput,
    pub absolute_path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInputTopology {
    inputs: BTreeMap<String, ReleaseInput>,
}

impl ReleaseInputTopology {
    pub fn load(root: &Path) -> Result<Self, ReleaseModelError> {
        let path = root.join(DEFAULT_AUTHORITY_PATH);
        let input = fs::read_to_string(&path).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_INPUT_AUTHORITY_UNAVAILABLE: {}: {error}",
                path.display()
            ))
        })?;
        Self::parse_json(&input)
    }

    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!("invalid release input authority JSON: {error}"))
        })?;
        let root = object(&value, "release architecture root")?;
        if required_u64(root, "schema_version")? != SUPPORTED_SCHEMA_VERSION
            || required_string(root, "kind")? != "AR11_RELEASE_ARCHITECTURE_SOURCE"
        {
            return Err(ReleaseModelError::new(
                "release input topology authority identity/schema mismatch",
            ));
        }

        let rows = array(required(root, "release_inputs")?, "release_inputs")?;
        if rows.is_empty() {
            return Err(ReleaseModelError::new(
                "release_inputs must be present and non-empty",
            ));
        }

        let mut inputs = BTreeMap::new();
        let mut identity_paths = BTreeSet::new();
        for row in rows {
            let input = parse_input(row)?;
            if inputs
                .insert(input.input_id.clone(), input.clone())
                .is_some()
            {
                return Err(ReleaseModelError::new(format!(
                    "duplicate release input id: {}",
                    input.input_id
                )));
            }
            if !identity_paths.insert(input.release_identity_source.clone()) {
                return Err(ReleaseModelError::new(format!(
                    "duplicate release identity path: {}",
                    input.release_identity_source
                )));
            }
        }

        Ok(Self { inputs })
    }

    #[must_use]
    pub fn get(&self, input_id: &str) -> Option<&ReleaseInput> {
        self.inputs.get(input_id)
    }

    #[must_use]
    pub fn inputs_for_consumer(&self, consumer: &str) -> Vec<&ReleaseInput> {
        self.inputs
            .values()
            .filter(|input| input.consumed_by(consumer))
            .collect()
    }

    pub fn require(&self, input_id: &str) -> Result<&ReleaseInput, ReleaseModelError> {
        self.get(input_id).ok_or_else(|| {
            ReleaseModelError::new(format!("missing canonical release input: {input_id}"))
        })
    }

    pub fn resolve(&self, root: &Path) -> Result<Vec<ResolvedReleaseInput>, ReleaseModelError> {
        let root_metadata = fs::symlink_metadata(root).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_INPUT_ROOT_UNAVAILABLE: {}: {error}",
                root.display()
            ))
        })?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(ReleaseModelError::new(
                "RELEASE_INPUT_ROOT_INVALID: repository root must be a real directory",
            ));
        }
        let canonical_root = fs::canonicalize(root).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_INPUT_ROOT_INVALID: {}: {error}",
                root.display()
            ))
        })?;

        let mut resolved = Vec::with_capacity(self.inputs.len());
        for input in self.inputs.values() {
            let path = resolve_regular_repo_file(
                root,
                &canonical_root,
                &input.release_identity_source,
                &input.input_id,
            )?;
            if let Some(generator) = input.generator.as_deref() {
                resolve_regular_repo_file(root, &canonical_root, generator, "generator")?;
            }
            let metadata = fs::metadata(&path).map_err(|error| {
                ReleaseModelError::new(format!(
                    "RELEASE_INPUT_METADATA_FAILED: {}: {error}",
                    input.release_identity_source
                ))
            })?;
            let mut file = File::open(&path).map_err(|error| {
                ReleaseModelError::new(format!(
                    "RELEASE_INPUT_READ_FAILED: {}: {error}",
                    input.release_identity_source
                ))
            })?;
            let sha256 = sha256_reader_hex(&mut file).map_err(|error| {
                ReleaseModelError::new(format!(
                    "RELEASE_INPUT_HASH_FAILED: {}: {error}",
                    input.release_identity_source
                ))
            })?;
            resolved.push(ResolvedReleaseInput {
                input: input.clone(),
                absolute_path: path,
                sha256,
                size_bytes: metadata.len(),
            });
        }
        Ok(resolved)
    }
}

fn parse_input(value: &Value) -> Result<ReleaseInput, ReleaseModelError> {
    let object = object(value, "release input")?;
    reject_unknown_fields(
        object,
        &[
            "input_id",
            "kind",
            "semantic_owner",
            "canonical_source",
            "generated_projection",
            "release_identity_source",
            "compatibility_dimension",
            "required_for_release_set",
            "generator",
            "verification",
            "consumers",
        ],
        "release input",
    )?;

    let input_id = required_non_empty_string(object, "input_id")?;
    let kind = required_non_empty_string(object, "kind")?;
    let semantic_owner = required_non_empty_string(object, "semantic_owner")?;
    let canonical_source = optional_non_empty_string(object, "canonical_source")?;
    let generated_projection = optional_non_empty_string(object, "generated_projection")?;
    if canonical_source.is_some() == generated_projection.is_some() {
        return Err(ReleaseModelError::new(format!(
            "release input {input_id} must define exactly one of canonical_source or generated_projection"
        )));
    }
    let release_identity_source = required_non_empty_string(object, "release_identity_source")?;
    validate_relative_path(&release_identity_source, "release_identity_source")?;
    let selected_source = canonical_source
        .as_deref()
        .or(generated_projection.as_deref())
        .ok_or_else(|| ReleaseModelError::new("release input source disappeared"))?;
    if selected_source != release_identity_source {
        return Err(ReleaseModelError::new(format!(
            "release input {input_id} must bind release_identity_source to its canonical/generated source"
        )));
    }
    validate_relative_path(selected_source, "release input source")?;

    let compatibility_dimension = required_non_empty_string(object, "compatibility_dimension")?;
    let required_for_release_set = required_bool(object, "required_for_release_set")?;
    let generator = optional_non_empty_string(object, "generator")?;
    if let Some(generator) = generator.as_deref() {
        validate_relative_path(generator, "generator")?;
        if generated_projection.is_none() {
            return Err(ReleaseModelError::new(format!(
                "release input {input_id} may define generator only for generated_projection"
            )));
        }
    }
    if generated_projection.is_some() && generator.is_none() {
        return Err(ReleaseModelError::new(format!(
            "generated release input {input_id} must name its deterministic generator"
        )));
    }
    let verification = required_string_array(object, "verification")?;
    if verification.is_empty() {
        return Err(ReleaseModelError::new(format!(
            "release input {input_id} must define verification"
        )));
    }
    let consumers = required_string_array(object, "consumers")?;
    if consumers.is_empty() {
        return Err(ReleaseModelError::new(format!(
            "release input {input_id} must define at least one consumer"
        )));
    }

    Ok(ReleaseInput {
        input_id,
        kind,
        semantic_owner,
        canonical_source,
        generated_projection,
        release_identity_source,
        compatibility_dimension,
        required_for_release_set,
        generator,
        verification,
        consumers,
    })
}

fn resolve_regular_repo_file(
    root: &Path,
    canonical_root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, ReleaseModelError> {
    validate_relative_path(relative, label)?;
    let components = Path::new(relative).components().collect::<Vec<_>>();
    let mut cursor = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(part) = component else {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INPUT_PATH_INVALID: {label}={relative}"
            )));
        };
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_INPUT_MISSING: {relative}: {error}"
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INPUT_SYMLINK_FORBIDDEN: {}",
                cursor.display()
            )));
        }
        let is_last = index + 1 == components.len();
        if is_last && !metadata.is_file() {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INPUT_TYPE_INVALID: {relative} must be a regular file"
            )));
        }
        if !is_last && !metadata.is_dir() {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INPUT_PATH_INVALID: {} is not a directory",
                cursor.display()
            )));
        }
    }
    let canonical = fs::canonicalize(&cursor).map_err(|error| {
        ReleaseModelError::new(format!(
            "RELEASE_INPUT_CANONICALIZE_FAILED: {relative}: {error}"
        ))
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(ReleaseModelError::new(format!(
            "RELEASE_INPUT_PATH_ESCAPE: {relative}"
        )));
    }
    Ok(cursor)
}

fn validate_relative_path(value: &str, label: &str) -> Result<(), ReleaseModelError> {
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!("{label} must not be empty")));
    }
    let path = Path::new(value);
    if path.is_absolute() || path.components().next().is_none() {
        return Err(ReleaseModelError::new(format!(
            "RELEASE_INPUT_PATH_INVALID: {label}={value}"
        )));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_INPUT_PATH_INVALID: {label}={value}"
            )));
        }
    }
    Ok(())
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("missing release input field: {key}")))
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON object")))
}

fn array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseModelError::new(format!("{label} must be a JSON array")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
}

fn required_non_empty_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<String, ReleaseModelError> {
    let value = required_string(object, key)?;
    if value.trim().is_empty() {
        return Err(ReleaseModelError::new(format!(
            "field {key} must not be empty"
        )));
    }
    Ok(value)
}

fn optional_non_empty_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ReleaseModelError> {
    match object.get(key) {
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))?;
            if value.trim().is_empty() {
                return Err(ReleaseModelError::new(format!(
                    "field {key} must not be empty"
                )));
            }
            Ok(Some(value.to_owned()))
        }
        None => Ok(None),
    }
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?
        .as_bool()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a boolean")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn required_string_array(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ReleaseModelError> {
    let values = array(required(object, key)?, key)?;
    let mut result = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| ReleaseModelError::new(format!("{key} must contain strings")))?;
        if text.trim().is_empty() {
            return Err(ReleaseModelError::new(format!(
                "{key} must not contain empty strings"
            )));
        }
        if !seen.insert(text.to_owned()) {
            return Err(ReleaseModelError::new(format!(
                "{key} contains duplicate value {text}"
            )));
        }
        result.push(text.to_owned());
    }
    Ok(result)
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), ReleaseModelError> {
    for field in object.keys() {
        if !allowed.iter().any(|allowed_field| field == allowed_field) {
            return Err(ReleaseModelError::new(format!(
                "unknown {label} field {field}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ReleaseInputTopology;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn canonical_topology_resolves_and_owns_full_public_api_tree()
    -> Result<(), Box<dyn std::error::Error>> {
        let topology = ReleaseInputTopology::load(&root())?;
        let contracts = topology.inputs_for_consumer("release_set.contracts");
        assert_eq!(contracts.len(), 10);
        assert!(
            contracts
                .iter()
                .any(|input| input.release_identity_source == "openapi/v1/openapi.json")
        );
        assert!(contracts.iter().all(|input| {
            !input
                .release_identity_source
                .ends_with("control-plane.yaml")
        }));
        let resolved = topology.resolve(&root())?;
        assert_eq!(resolved.len(), 19);
        Ok(())
    }

    #[test]
    fn duplicate_release_identity_path_is_rejected() {
        let source = r#"{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "release_inputs":[
            {"input_id":"a","kind":"X","semantic_owner":"x","canonical_source":"Cargo.lock","release_identity_source":"Cargo.lock","compatibility_dimension":"x","required_for_release_set":true,"verification":["x"],"consumers":["x"]},
            {"input_id":"b","kind":"X","semantic_owner":"x","canonical_source":"Cargo.lock","release_identity_source":"Cargo.lock","compatibility_dimension":"x","required_for_release_set":true,"verification":["x"],"consumers":["x"]}
          ]
        }"#;
        assert!(ReleaseInputTopology::parse_json(source).is_err());
    }

    #[test]
    fn parent_escape_is_rejected_before_filesystem_access() {
        let source = r#"{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "release_inputs":[
            {"input_id":"escape","kind":"X","semantic_owner":"x","canonical_source":"../Cargo.lock","release_identity_source":"../Cargo.lock","compatibility_dimension":"x","required_for_release_set":true,"verification":["x"],"consumers":["x"]}
          ]
        }"#;
        assert!(ReleaseInputTopology::parse_json(source).is_err());
    }

    #[test]
    fn unknown_release_input_field_is_rejected() {
        let source = r#"{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "release_inputs":[
            {"input_id":"a","kind":"X","semantic_owner":"x","canonical_source":"Cargo.lock","release_identity_source":"Cargo.lock","compatibility_dimension":"x","required_for_release_set":true,"verification":["x"],"consumers":["x"],"unexpected":true}
          ]
        }"#;
        assert!(ReleaseInputTopology::parse_json(source).is_err());
    }
}
