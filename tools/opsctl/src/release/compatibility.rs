use crate::release::authority::ReleaseArchitecture;
use crate::release::model::{CompatibilityDecision, ReleaseModelError, ReleaseSetManifest};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const SCHEMA_VERSION: u64 = 1;
const REQUIRED_DIMENSIONS: [&str; 11] = [
    "catalog_d1",
    "resolver_d1",
    "public_api",
    "frontend_api",
    "resolver_protocol",
    "bridge_protocol",
    "camouhost_ipc",
    "runtime_bundle",
    "profile_format",
    "browser_identity_policy",
    "windows_profile_bridge",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityDimension {
    pub decision: CompatibilityDecision,
    pub evidence_sha256: String,
    pub policy_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityEvidence {
    pub release_set_id: String,
    pub dimensions: BTreeMap<String, CompatibilityDimension>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityResult {
    pub compatible: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub required_steps: Vec<String>,
    pub rollback_compatibility: String,
}

impl CompatibilityEvidence {
    pub fn load(path: &Path) -> Result<Self, ReleaseModelError> {
        let input = fs::read_to_string(path).map_err(|error| {
            ReleaseModelError::new(format!(
                "COMPATIBILITY_EVIDENCE_UNAVAILABLE: {}: {error}",
                path.display()
            ))
        })?;
        Self::parse_json(&input)
    }

    pub fn parse_json(input: &str) -> Result<Self, ReleaseModelError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseModelError::new(format!("invalid compatibility evidence JSON: {error}"))
        })?;
        let root = object(&value, "compatibility evidence root")?;
        reject_unknown_fields(root, &["schema_version", "kind", "release_set_id", "dimensions"])?;
        if required_u64(root, "schema_version")? != SCHEMA_VERSION
            || required_string(root, "kind")? != "RELEASE_COMPATIBILITY_EVIDENCE"
        {
            return Err(ReleaseModelError::new(
                "unsupported compatibility evidence identity/version",
            ));
        }
        let release_set_id = required_string(root, "release_set_id")?;
        let dimensions_value = object(required(root, "dimensions")?, "dimensions")?;
        let observed = dimensions_value.keys().map(String::as_str).collect::<BTreeSet<_>>();
        let expected = REQUIRED_DIMENSIONS.into_iter().collect::<BTreeSet<_>>();
        if observed != expected {
            return Err(ReleaseModelError::new(format!(
                "compatibility dimension set mismatch: expected={expected:?} observed={observed:?}"
            )));
        }
        let mut dimensions = BTreeMap::new();
        for (name, value) in dimensions_value {
            let item = object(value, &format!("dimensions.{name}"))?;
            reject_unknown_fields(item, &["decision", "evidence_sha256", "policy_source"])?;
            let decision = CompatibilityDecision::parse(&required_string(item, "decision")?)?;
            let evidence_sha256 = required_string(item, "evidence_sha256")?;
            validate_sha256(&evidence_sha256, &format!("dimensions.{name}.evidence_sha256"))?;
            let policy_source = required_string(item, "policy_source")?;
            if policy_source.trim().is_empty() {
                return Err(ReleaseModelError::new(format!(
                    "dimensions.{name}.policy_source must not be empty"
                )));
            }
            if matches!(name.as_str(), "catalog_d1" | "resolver_d1")
                && policy_source != "opsctl.d1.compatibility"
            {
                return Err(ReleaseModelError::new(format!(
                    "{name} evidence must come from accepted opsctl.d1.compatibility policy"
                )));
            }
            dimensions.insert(
                name.clone(),
                CompatibilityDimension {
                    decision,
                    evidence_sha256,
                    policy_source,
                },
            );
        }
        Ok(Self {
            release_set_id,
            dimensions,
        })
    }
}

pub fn evaluate(
    root: &Path,
    manifest: &ReleaseSetManifest,
    evidence: &CompatibilityEvidence,
    profile_id: &str,
    environment: &str,
    current_release: Option<&ReleaseSetManifest>,
) -> Result<CompatibilityResult, ReleaseModelError> {
    if evidence.release_set_id != manifest.release_set_id {
        return Err(ReleaseModelError::new(
            "RELEASE_IDENTITY_MISMATCH: compatibility evidence targets another Release Set",
        ));
    }
    if !manifest
        .capability_profile_compatibility
        .iter()
        .any(|profile| profile == profile_id)
    {
        return Ok(blocked("PROFILE_NOT_AUTHORIZED"));
    }

    let authority = ReleaseArchitecture::load(root)
        .map_err(|error| ReleaseModelError::new(format!("release authority invalid: {error}")))?;
    let effective = authority
        .effective_profile(profile_id, environment)
        .map_err(|error| ReleaseModelError::new(format!("PROFILE_NOT_AUTHORIZED: {error}")))?;
    let profile = authority
        .profiles
        .get(profile_id)
        .ok_or_else(|| ReleaseModelError::new("PROFILE_NOT_AUTHORIZED: profile disappeared"))?;

    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut required_steps = Vec::new();
    if environment == "production" && profile.current_authorization != "AUTHORIZED" {
        blockers.push("PROFILE_NOT_AUTHORIZED".to_owned());
        required_steps.push(format!(
            "complete activation gate {} before production execution",
            profile.activation_gate
        ));
    }

    let mailbox_admin = effective.is_enabled("mailbox_admin");
    let windows_required = authority
        .activation_units
        .values()
        .any(|unit| unit.requires_windows_profile_bridge && effective.is_enabled(&unit.id));
    let required_dimensions = required_dimensions(mailbox_admin, windows_required);
    for name in required_dimensions {
        let dimension = evidence.dimensions.get(name).ok_or_else(|| {
            ReleaseModelError::new(format!("missing compatibility dimension after parse: {name}"))
        })?;
        match dimension.decision {
            CompatibilityDecision::Compatible => {}
            CompatibilityDecision::Incompatible => {
                blockers.push(format!("{}", blocker_code(name, false)));
            }
            CompatibilityDecision::Unknown => {
                blockers.push(format!("{}", blocker_code(name, true)));
            }
        }
    }

    for (name, dimension) in &evidence.dimensions {
        if !required_dimensions.contains(&name.as_str())
            && !dimension.decision.is_compatible()
        {
            warnings.push(format!(
                "{name} is {} but is outside the selected deployment closure",
                if dimension.decision == CompatibilityDecision::Unknown {
                    "UNKNOWN"
                } else {
                    "INCOMPATIBLE"
                }
            ));
        }
    }

    let rollback_compatibility = match current_release {
        Some(current) if current.release_set_id == manifest.release_set_id => "NO_CHANGE",
        Some(current) => rollback_compatibility(current, evidence, profile_id),
        None => "UNKNOWN",
    }
    .to_owned();
    if rollback_compatibility == "UNKNOWN" {
        warnings.push("rollback compatibility is unknown without current release context".to_owned());
    }

    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    required_steps.sort();
    required_steps.dedup();
    Ok(CompatibilityResult {
        compatible: blockers.is_empty(),
        blockers,
        warnings,
        required_steps,
        rollback_compatibility,
    })
}

impl CompatibilityResult {
    #[must_use]
    pub fn machine_json(&self, release_set_id: &str, profile_id: &str, environment: &str) -> Value {
        json!({
            "schema_version": 1,
            "command": "release.compatibility",
            "decision": if self.compatible { "COMPATIBLE" } else { "INCOMPATIBLE" },
            "compatible": self.compatible,
            "release_set_id": release_set_id,
            "capability_profile_id": profile_id,
            "environment": environment,
            "blockers": self.blockers,
            "warnings": self.warnings,
            "required_steps": self.required_steps,
            "rollback_compatibility": self.rollback_compatibility,
            "mutation_executed": false
        })
    }
}

fn blocked(code: &str) -> CompatibilityResult {
    CompatibilityResult {
        compatible: false,
        blockers: vec![code.to_owned()],
        warnings: Vec::new(),
        required_steps: Vec::new(),
        rollback_compatibility: "UNKNOWN".to_owned(),
    }
}

fn required_dimensions(mailbox_admin: bool, windows_required: bool) -> BTreeSet<&'static str> {
    let mut required = [
        "catalog_d1",
        "public_api",
        "frontend_api",
        "bridge_protocol",
        "camouhost_ipc",
        "runtime_bundle",
        "profile_format",
        "browser_identity_policy",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if mailbox_admin {
        required.insert("resolver_d1");
        required.insert("resolver_protocol");
    }
    if windows_required {
        required.insert("windows_profile_bridge");
    }
    required
}

fn blocker_code(name: &str, unknown: bool) -> &'static str {
    match (name, unknown) {
        ("catalog_d1" | "resolver_d1", _) => "SCHEMA_INCOMPATIBLE",
        ("public_api" | "frontend_api" | "resolver_protocol" | "bridge_protocol" | "camouhost_ipc", _) => {
            "PROTOCOL_INCOMPATIBLE"
        }
        ("runtime_bundle" | "profile_format" | "browser_identity_policy", _) => {
            "RUNTIME_INCOMPATIBLE"
        }
        ("windows_profile_bridge", _) => "WINDOWS_DELIVERY_UNSATISFIED",
        (_, true) => "PROVIDER_STATE_UNKNOWN",
        _ => "RELEASE_INCOMPATIBLE",
    }
}

fn rollback_compatibility(
    current: &ReleaseSetManifest,
    evidence: &CompatibilityEvidence,
    profile_id: &str,
) -> &'static str {
    if current
        .capability_profile_compatibility
        .iter()
        .any(|profile| profile == profile_id)
        && evidence
            .dimensions
            .get("catalog_d1")
            .is_some_and(|value| value.decision.is_compatible())
    {
        "COMPATIBLE"
    } else {
        "UNKNOWN"
    }
}

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
        .ok_or_else(|| ReleaseModelError::new(format!("{context} must be an object")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ReleaseModelError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ReleaseModelError::new(format!(
                "unknown compatibility evidence field: {key}"
            )));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), ReleaseModelError> {
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
    use super::CompatibilityEvidence;

    #[test]
    fn rejects_unknown_dimension_state() {
        let input = r#"{
          "schema_version":1,
          "kind":"RELEASE_COMPATIBILITY_EVIDENCE",
          "release_set_id":"release-set-v1-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "dimensions":{
            "catalog_d1":{"decision":"MAYBE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"opsctl.d1.compatibility"},
            "resolver_d1":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"opsctl.d1.compatibility"},
            "public_api":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "frontend_api":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "resolver_protocol":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "bridge_protocol":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "camouhost_ipc":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "runtime_bundle":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "profile_format":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "browser_identity_policy":{"decision":"COMPATIBLE","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"},
            "windows_profile_bridge":{"decision":"UNKNOWN","evidence_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_source":"saved"}
          }
        }"#;
        assert!(CompatibilityEvidence::parse_json(input).is_err());
    }
}
