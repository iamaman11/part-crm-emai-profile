use crate::release::digest::{canonical_json, sha256_hex};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_SCHEMA_VERSION: u64 = 1;
pub const RELEASE_SET_ID_PREFIX: &str = "release-set-v1-sha256-";
pub const EXPECTED_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const REQUIRED_COMPONENTS: [&str; 3] = ["control_plane", "secret_resolver", "runtime_bundle"];
const ALLOWED_COMPONENTS: [&str; 5] = [
    "control_plane",
    "frontend",
    "secret_resolver",
    "runtime_bundle",
    "profile_bridge",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityDecision {
    Compatible,
    Incompatible,
    Unknown,
}

impl CompatibilityDecision {
    pub fn parse(value: &str) -> Result<Self, ReleaseModelError> {
        match value {
            "COMPATIBLE" => Ok(Self::Compatible),
            "INCOMPATIBLE" => Ok(Self::Incompatible),
            "UNKNOWN" => Ok(Self::Unknown),
            other => Err(ReleaseModelError::new(format!(
                "unsupported compatibility decision: {other}"
            ))),
        }
    }

    #[must_use]
    pub const fn is_compatible(self) -> bool {
        matches!(self, Self::Compatible)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetSource {
    pub repository: String,
    pub commit_sha: String,
    pub accepted_main: bool,
    pub accepted_main_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseComponentIdentity {
    pub component_id: String,
    pub release_id: String,
    pub source_commit_sha: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub component_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseSetManifest {
    pub schema_version: u64,
    pub release_set_id: String,
    pub display_version: Option<String>,
    pub source: ReleaseSetSource,
    pub components: BTreeMap<String, ReleaseComponentIdentity>,
    pub contracts: Value,
    pub protocols: Value,
    pub schemas: Value,
    pub runtime_compatibility: Value,
    pub capability_profile_compatibility: Vec<String>,
    pub build_provenance: Value,
    pub artifact_inventory: Vec<ArtifactIdentity>,
    identity_payload: Value,
}

impl ReleaseSetManifest {
    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!("invalid release-set JSON: {error}"))
        })?;
        let root = object(&value, "release-set root")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "release_set_id",
                "display_version",
                "source",
                "components",
                "contracts",
                "protocols",
                "schemas",
                "runtime_compatibility",
                "capability_profile_compatibility",
                "build_provenance",
                "artifact_inventory",
            ],
            "release-set root",
        )?;

        let schema_version = required_u64(root, "schema_version")?;
        if schema_version != RELEASE_SET_SCHEMA_VERSION {
            return Err(ReleaseModelError::new(format!(
                "unsupported release-set schema_version: {schema_version}"
            )));
        }
        let release_set_id = required_string(root, "release_set_id")?;
        validate_release_set_id(&release_set_id)?;
        let display_version = optional_string(root, "display_version")?;

        let source = parse_source(required(root, "source")?)?;
        let components = parse_components(required(root, "components")?, &source.commit_sha)?;
        let artifact_inventory = parse_artifact_inventory(required(root, "artifact_inventory")?)?;
        validate_component_artifacts(&components, &artifact_inventory)?;

        let contracts = required_object_value(root, "contracts")?;
        let protocols = required_object_value(root, "protocols")?;
        let schemas = required_object_value(root, "schemas")?;
        let runtime_compatibility = required_object_value(root, "runtime_compatibility")?;
        let capability_profile_compatibility =
            required_string_array(root, "capability_profile_compatibility")?;
        if capability_profile_compatibility.is_empty() {
            return Err(ReleaseModelError::new(
                "capability_profile_compatibility must not be empty",
            ));
        }
        let build_provenance = required_object_value(root, "build_provenance")?;

        let mut identity_payload = value.clone();
        let payload = identity_payload
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("release-set root must remain an object"))?;
        payload.remove("release_set_id");
        payload.remove("display_version");

        let manifest = Self {
            schema_version,
            release_set_id,
            display_version,
            source,
            components,
            contracts,
            protocols,
            schemas,
            runtime_compatibility,
            capability_profile_compatibility,
            build_provenance,
            artifact_inventory,
            identity_payload,
        };
        manifest.verify_content_address()?;
        Ok(manifest)
    }

    pub fn verify_content_address(&self) -> Result<(), ReleaseModelError> {
        let canonical = canonical_json(&self.identity_payload).map_err(ReleaseModelError::new)?;
        let expected = format!(
            "{RELEASE_SET_ID_PREFIX}{}",
            sha256_hex(canonical.as_bytes())
        );
        if self.release_set_id != expected {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_IDENTITY_MISMATCH: expected {expected}, observed {}",
                self.release_set_id
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn component_ids(&self) -> Vec<&str> {
        self.components.keys().map(String::as_str).collect()
    }
}

fn parse_source(value: &Value) -> Result<ReleaseSetSource, ReleaseModelError> {
    let source = object(value, "source")?;
    reject_unknown_fields(
        source,
        &[
            "repository",
            "commit_sha",
            "accepted_main",
            "accepted_main_evidence_sha256",
        ],
        "source",
    )?;
    let repository = required_string(source, "repository")?;
    if repository != EXPECTED_REPOSITORY {
        return Err(ReleaseModelError::new(format!(
            "SOURCE_NOT_ACCEPTED: repository must be {EXPECTED_REPOSITORY}"
        )));
    }
    let commit_sha = required_string(source, "commit_sha")?;
    validate_git_sha(&commit_sha, "source.commit_sha")?;
    let accepted_main = required_bool(source, "accepted_main")?;
    if !accepted_main {
        return Err(ReleaseModelError::new(
            "SOURCE_NOT_ACCEPTED: accepted_main must be true",
        ));
    }
    let accepted_main_evidence_sha256 = required_string(source, "accepted_main_evidence_sha256")?;
    validate_sha256_like(
        &accepted_main_evidence_sha256,
        "source.accepted_main_evidence_sha256",
    )?;
    let accepted_main_identity = serde_json::json!({
        "authority": "accepted-main",
        "commit_sha": commit_sha,
        "repository": repository,
    });
    let canonical = canonical_json(&accepted_main_identity).map_err(ReleaseModelError::new)?;
    let expected_evidence = sha256_hex(canonical.as_bytes());
    if accepted_main_evidence_sha256 != expected_evidence {
        return Err(ReleaseModelError::new(
            "SOURCE_NOT_ACCEPTED: accepted-main evidence does not bind repository and commit SHA",
        ));
    }
    Ok(ReleaseSetSource {
        repository,
        commit_sha,
        accepted_main,
        accepted_main_evidence_sha256,
    })
}

fn parse_components(
    value: &Value,
    source_commit_sha: &str,
) -> Result<BTreeMap<String, ReleaseComponentIdentity>, ReleaseModelError> {
    let components = object(value, "components")?;
    if components.is_empty() {
        return Err(ReleaseModelError::new("components must not be empty"));
    }
    for required in REQUIRED_COMPONENTS {
        if !components.contains_key(required) {
            return Err(ReleaseModelError::new(format!(
                "missing required component: {required}"
            )));
        }
    }
    let mut result = BTreeMap::new();
    for (component_id, value) in components {
        if !ALLOWED_COMPONENTS.contains(&component_id.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown component: {component_id}"
            )));
        }
        let component = object(value, &format!("components.{component_id}"))?;
        reject_unknown_fields(
            component,
            &[
                "release_id",
                "source_commit_sha",
                "artifact_path",
                "artifact_sha256",
                "artifact_size_bytes",
                "component_manifest_sha256",
            ],
            &format!("components.{component_id}"),
        )?;
        let release_id = required_string(component, "release_id")?;
        if release_id.trim().is_empty() {
            return Err(ReleaseModelError::new(format!(
                "components.{component_id}.release_id must not be empty"
            )));
        }
        let component_source = required_string(component, "source_commit_sha")?;
        validate_git_sha(
            &component_source,
            &format!("components.{component_id}.source_commit_sha"),
        )?;
        if component_source != source_commit_sha {
            return Err(ReleaseModelError::new(format!(
                "SOURCE_IDENTITY_MISMATCH: component {component_id} source SHA differs from release source"
            )));
        }
        let artifact_path = required_string(component, "artifact_path")?;
        validate_artifact_path(&artifact_path)?;
        let artifact_sha256 = required_string(component, "artifact_sha256")?;
        validate_sha256_like(
            &artifact_sha256,
            &format!("components.{component_id}.artifact_sha256"),
        )?;
        let artifact_size_bytes = required_u64(component, "artifact_size_bytes")?;
        if artifact_size_bytes == 0 {
            return Err(ReleaseModelError::new(format!(
                "components.{component_id}.artifact_size_bytes must be positive"
            )));
        }
        let component_manifest_sha256 = required_string(component, "component_manifest_sha256")?;
        validate_sha256_like(
            &component_manifest_sha256,
            &format!("components.{component_id}.component_manifest_sha256"),
        )?;
        result.insert(
            component_id.clone(),
            ReleaseComponentIdentity {
                component_id: component_id.clone(),
                release_id,
                source_commit_sha: component_source,
                artifact_path,
                artifact_sha256,
                artifact_size_bytes,
                component_manifest_sha256,
            },
        );
    }
    Ok(result)
}

fn parse_artifact_inventory(value: &Value) -> Result<Vec<ArtifactIdentity>, ReleaseModelError> {
    let values = array(value, "artifact_inventory")?;
    if values.is_empty() {
        return Err(ReleaseModelError::new(
            "artifact_inventory must not be empty",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let item = object(value, "artifact_inventory entry")?;
        reject_unknown_fields(
            item,
            &["path", "sha256", "size_bytes", "kind"],
            "artifact_inventory entry",
        )?;
        let path = required_string(item, "path")?;
        validate_artifact_path(&path)?;
        if !paths.insert(path.clone()) {
            return Err(ReleaseModelError::new(format!(
                "duplicate artifact path: {path}"
            )));
        }
        let sha256 = required_string(item, "sha256")?;
        validate_sha256_like(&sha256, &format!("artifact_inventory.{path}.sha256"))?;
        let size_bytes = required_u64(item, "size_bytes")?;
        if size_bytes == 0 {
            return Err(ReleaseModelError::new(format!(
                "artifact {path} has zero size"
            )));
        }
        let kind = required_string(item, "kind")?;
        if !matches!(
            kind.as_str(),
            "component" | "contract" | "runtime" | "manifest" | "sbom"
        ) {
            return Err(ReleaseModelError::new(format!(
                "unknown artifact kind for {path}: {kind}"
            )));
        }
        result.push(ArtifactIdentity {
            path,
            sha256,
            size_bytes,
            kind,
        });
    }
    result.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(result)
}

fn validate_component_artifacts(
    components: &BTreeMap<String, ReleaseComponentIdentity>,
    inventory: &[ArtifactIdentity],
) -> Result<(), ReleaseModelError> {
    let by_path = inventory
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    for component in components.values() {
        let artifact = by_path
            .get(component.artifact_path.as_str())
            .ok_or_else(|| {
                ReleaseModelError::new(format!(
                    "component {} artifact is absent from artifact_inventory",
                    component.component_id
                ))
            })?;
        if artifact.kind != "component"
            || artifact.sha256 != component.artifact_sha256
            || artifact.size_bytes != component.artifact_size_bytes
        {
            return Err(ReleaseModelError::new(format!(
                "component {} artifact identity disagrees with artifact_inventory",
                component.component_id
            )));
        }
    }
    Ok(())
}

pub fn validate_artifact_path(path: &str) -> Result<(), ReleaseModelError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path.contains(':')
    {
        return Err(ReleaseModelError::new(format!(
            "unsafe artifact path: {path:?}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseModelError {
    message: String,
}

impl ReleaseModelError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReleaseModelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseModelError {}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| ReleaseModelError::new(format!("missing required field: {key}")))
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
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be a string")))
        })
        .transpose()
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

fn required_object_value(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Value, ReleaseModelError> {
    let value = required(object, key)?;
    if !value.is_object() {
        return Err(ReleaseModelError::new(format!(
            "field {key} must be an object"
        )));
    }
    Ok(value.clone())
}

fn required_string_array(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ReleaseModelError> {
    let values = array(required(object, key)?, key)?;
    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| ReleaseModelError::new(format!("field {key} must contain strings")))?;
        if !seen.insert(text.to_owned()) {
            return Err(ReleaseModelError::new(format!(
                "field {key} contains duplicate {text}"
            )));
        }
        output.push(text.to_owned());
    }
    Ok(output)
}

fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON object")))
}

fn array<'a>(value: &'a Value, context: &str) -> Result<&'a Vec<Value>, ReleaseModelError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON array")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown field in {context}: {key}"
            )));
        }
    }
    Ok(())
}

fn validate_release_set_id(value: &str) -> Result<(), ReleaseModelError> {
    let digest = value
        .strip_prefix(RELEASE_SET_ID_PREFIX)
        .ok_or_else(|| ReleaseModelError::new("release_set_id has an unsupported prefix"))?;
    validate_sha256_like(digest, "release_set_id digest")
}

fn validate_git_sha(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_sha256_like(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReleaseModelError::new(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityDecision, RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
    use crate::release::digest::{canonical_json, sha256_hex};
    use serde_json::{Value, json};

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn accepted_main_evidence() -> Result<String, String> {
        let identity = json!({
            "authority": "accepted-main",
            "commit_sha": GIT_SHA,
            "repository": REPOSITORY,
        });
        Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
    }

    fn fixture() -> Result<Value, String> {
        Ok(json!({
          "schema_version": 1,
          "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"),
          "display_version": "test",
          "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": accepted_main_evidence()?
          },
          "components": {
            "control_plane": component("control-plane-v1", "components/control-plane.tar", SHA_A, 10),
            "secret_resolver": component("resolver-v1", "components/resolver.tar", SHA_B, 11),
            "runtime_bundle": component("runtime-v1", "components/runtime.tar", SHA_C, 12)
          },
          "contracts": {"openapi_sha256": SHA_A},
          "protocols": {"bridge": "v1", "camouhost_ipc": "v1"},
          "schemas": {"catalog": {"min": 1, "max": 26, "target": 26}, "resolver": {"min": 1, "max": 2, "target": 2}},
          "runtime_compatibility": {"runtime_bundle": "v1", "profile_format": "v1"},
          "capability_profile_compatibility": ["rehearsal-core-v1"],
          "build_provenance": {"toolchain": "rust-1.97.1", "lockfile_sha256": SHA_A},
          "artifact_inventory": [
            {"path": "components/control-plane.tar", "sha256": SHA_A, "size_bytes": 10, "kind": "component"},
            {"path": "components/resolver.tar", "sha256": SHA_B, "size_bytes": 11, "kind": "component"},
            {"path": "components/runtime.tar", "sha256": SHA_C, "size_bytes": 12, "kind": "component"}
          ]
        }))
    }

    fn component(release_id: &str, path: &str, digest: &str, size: u64) -> Value {
        json!({
          "release_id": release_id,
          "source_commit_sha": GIT_SHA,
          "artifact_path": path,
          "artifact_sha256": digest,
          "artifact_size_bytes": size,
          "component_manifest_sha256": SHA_A
        })
    }

    fn signed_fixture() -> Result<String, String> {
        let mut value = fixture()?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| "fixture root must be object".to_owned())?;
        object.remove("release_set_id");
        object.remove("display_version");
        let digest = sha256_hex(canonical_json(&value)?.as_bytes());
        let mut complete = fixture()?;
        complete["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
        serde_json::to_string(&complete).map_err(|error| error.to_string())
    }

    #[test]
    fn parses_and_verifies_content_addressed_release_set() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = ReleaseSetManifest::parse_json(&signed_fixture()?)?;
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.source.commit_sha, GIT_SHA);
        assert_eq!(
            parsed.component_ids(),
            vec!["control_plane", "runtime_bundle", "secret_resolver"]
        );
        Ok(())
    }

    #[test]
    fn rejects_component_from_different_source_sha() -> Result<(), String> {
        let mut value: Value =
            serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["components"]["control_plane"]["source_commit_sha"] =
            Value::String("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned());
        let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        assert!(ReleaseSetManifest::parse_json(&text).is_err());
        Ok(())
    }

    #[test]
    fn rejects_wrong_release_set_digest() -> Result<(), String> {
        let mut value: Value =
            serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{SHA_A}"));
        let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        assert!(ReleaseSetManifest::parse_json(&text).is_err());
        Ok(())
    }

    #[test]
    fn rejects_unknown_top_level_field() -> Result<(), String> {
        let mut value: Value =
            serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["unexpected"] = Value::Bool(true);
        let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        assert!(ReleaseSetManifest::parse_json(&text).is_err());
        Ok(())
    }

    #[test]
    fn rejects_unsafe_artifact_path() -> Result<(), String> {
        let mut value: Value =
            serde_json::from_str(&signed_fixture()?).map_err(|error| error.to_string())?;
        value["artifact_inventory"][0]["path"] = Value::String("../secret".to_owned());
        let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        assert!(ReleaseSetManifest::parse_json(&text).is_err());
        Ok(())
    }

    #[test]
    fn unknown_compatibility_is_not_compatible() -> Result<(), Box<dyn std::error::Error>> {
        let decision = CompatibilityDecision::parse("UNKNOWN")?;
        assert!(!decision.is_compatible());
        Ok(())
    }
}
