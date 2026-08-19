use serde_json::{Map, Value};
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_SCHEMA_VERSION: u64 = 1;
pub const RELEASE_SET_ID_PREFIX: &str = "release-set-v1-sha256-";

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseComponentIdentity {
    pub component_id: String,
    pub source_commit_sha: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetIdentity {
    pub schema_version: u64,
    pub release_set_id: String,
    pub source: ReleaseSetSource,
    pub components: Vec<ReleaseComponentIdentity>,
}

impl ReleaseSetIdentity {
    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input)
            .map_err(|error| ReleaseModelError::new(format!("invalid release-set JSON: {error}")))?;
        let root = object(&value, "release-set root")?;

        let schema_version = required_u64(root, "schema_version")?;
        if schema_version != RELEASE_SET_SCHEMA_VERSION {
            return Err(ReleaseModelError::new(format!(
                "unsupported release-set schema_version: {schema_version}"
            )));
        }

        let release_set_id = required_string(root, "release_set_id")?;
        validate_release_set_id(&release_set_id)?;

        let source_value = required(root, "source")?;
        let source_object = object(source_value, "source")?;
        let repository = required_string(source_object, "repository")?;
        if repository.trim().is_empty() {
            return Err(ReleaseModelError::new("source.repository must not be empty"));
        }
        let commit_sha = required_string(source_object, "commit_sha")?;
        validate_sha256_like(&commit_sha, "source.commit_sha")?;

        let components_value = required(root, "components")?;
        let components_object = object(components_value, "components")?;
        if components_object.is_empty() {
            return Err(ReleaseModelError::new(
                "release-set must contain at least one component",
            ));
        }

        let mut components = Vec::with_capacity(components_object.len());
        for (component_id, component_value) in components_object {
            if component_id.trim().is_empty() {
                return Err(ReleaseModelError::new("component id must not be empty"));
            }
            let component_object = object(component_value, component_id)?;
            let component_source_sha = required_string(component_object, "source_commit_sha")?;
            validate_sha256_like(
                &component_source_sha,
                &format!("components.{component_id}.source_commit_sha"),
            )?;
            if component_source_sha != commit_sha {
                return Err(ReleaseModelError::new(format!(
                    "component {component_id} source SHA differs from release source SHA"
                )));
            }
            let artifact_sha256 = required_string(component_object, "artifact_sha256")?;
            validate_sha256_like(
                &artifact_sha256,
                &format!("components.{component_id}.artifact_sha256"),
            )?;
            components.push(ReleaseComponentIdentity {
                component_id: component_id.clone(),
                source_commit_sha: component_source_sha,
                artifact_sha256,
            });
        }
        components.sort_by(|left, right| left.component_id.cmp(&right.component_id));

        Ok(Self {
            schema_version,
            release_set_id,
            source: ReleaseSetSource {
                repository,
                commit_sha,
            },
            components,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseModelError {
    message: String,
}

impl ReleaseModelError {
    fn new(message: impl Into<String>) -> Self {
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

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?
        .as_u64()
        .ok_or_else(|| ReleaseModelError::new(format!("field {key} must be an unsigned integer")))
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be a JSON object")))
}

fn validate_release_set_id(value: &str) -> Result<(), ReleaseModelError> {
    let digest = value
        .strip_prefix(RELEASE_SET_ID_PREFIX)
        .ok_or_else(|| ReleaseModelError::new("release_set_id has an unsupported prefix"))?;
    validate_sha256_like(digest, "release_set_id digest")
}

fn validate_sha256_like(value: &str, field: &str) -> Result<(), ReleaseModelError> {
    let valid = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(ReleaseModelError::new(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityDecision, ReleaseSetIdentity};

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn release_set(component_source: &str) -> String {
        format!(
            r#"{{
  "schema_version": 1,
  "release_set_id": "release-set-v1-sha256-{SHA_B}",
  "source": {{
    "repository": "iamaman11/part-crm-emai-profile",
    "commit_sha": "{SHA_A}"
  }},
  "components": {{
    "control_plane": {{
      "source_commit_sha": "{component_source}",
      "artifact_sha256": "{SHA_B}"
    }}
  }}
}}"#
        )
    }

    #[test]
    fn parses_release_identity_and_sorts_components() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = ReleaseSetIdentity::parse_json(&release_set(SHA_A))?;
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.source.commit_sha, SHA_A);
        assert_eq!(parsed.components.len(), 1);
        assert_eq!(parsed.components[0].component_id, "control_plane");
        Ok(())
    }

    #[test]
    fn rejects_component_from_different_source_sha() {
        let result = ReleaseSetIdentity::parse_json(&release_set(SHA_B));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_compatibility_is_not_compatible() -> Result<(), Box<dyn std::error::Error>> {
        let decision = CompatibilityDecision::parse("UNKNOWN")?;
        assert!(!decision.is_compatible());
        Ok(())
    }

    #[test]
    fn rejects_unknown_compatibility_spelling() {
        assert!(CompatibilityDecision::parse("MAYBE").is_err());
    }
}
