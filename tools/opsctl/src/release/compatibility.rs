use crate::release::authority::ReleaseArchitecture;
use crate::release::model::{
    CompatibilityDecision, RELEASE_SET_ID_PREFIX, ReleaseModelError, ReleaseSetManifest,
};
use crate::release::static_compatibility;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const SCHEMA_VERSION: u64 = 2;
const REQUIRED_DIMENSIONS: [&str; 3] = ["catalog_d1", "resolver_d1", "windows_profile_bridge"];

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
        reject_unknown_fields(
            root,
            &["schema_version", "kind", "release_set_id", "dimensions"],
        )?;
        if required_u64(root, "schema_version")? != SCHEMA_VERSION
            || required_string(root, "kind")? != "RELEASE_COMPATIBILITY_EVIDENCE"
        {
            return Err(ReleaseModelError::new(
                "unsupported compatibility evidence identity/version; only v2 is accepted",
            ));
        }
        let release_set_id = required_string(root, "release_set_id")?;
        if !release_set_id.starts_with(RELEASE_SET_ID_PREFIX) {
            return Err(ReleaseModelError::new(
                "compatibility evidence must target a Release Set v2 ID",
            ));
        }
        let dimensions_value = object(required(root, "dimensions")?, "dimensions")?;
        let observed = dimensions_value
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
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
            validate_sha256(
                &evidence_sha256,
                &format!("dimensions.{name}.evidence_sha256"),
            )?;
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
                    "{name} evidence must come from opsctl.d1.compatibility"
                )));
            }
            if name == "windows_profile_bridge"
                && policy_source != "external.windows.delivery"
            {
                return Err(ReleaseModelError::new(
                    "windows_profile_bridge evidence must come from external.windows.delivery",
                ));
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
    policy_root: &Path,
    source_root: &Path,
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
        return Ok(blocked("PROFILE_NOT_AUTHORIZED", current_release.is_some()));
    }
    let authority = ReleaseArchitecture::load(policy_root)
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
    blockers.extend(static_compatibility::evaluate(
        source_root,
        manifest,
        mailbox_admin,
    )?);
    let windows_delivery_present = authority
        .activation_units
        .values()
        .any(|unit| unit.requires_windows_profile_bridge && effective.is_enabled(&unit.id));
    let windows_required = environment == "production" && windows_delivery_present;
    let required_dimensions = required_external_dimensions(mailbox_admin, windows_required);
    for name in required_dimensions {
        let dimension = evidence.dimensions.get(name).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "missing external compatibility dimension after parse: {name}"
            ))
        })?;
        match dimension.decision {
            CompatibilityDecision::Compatible => {}
            CompatibilityDecision::Incompatible => {
                blockers.push(external_blocker_code(name, false).to_owned());
            }
            CompatibilityDecision::Unknown => {
                blockers.push(external_blocker_code(name, true).to_owned());
            }
        }
    }
    for name in REQUIRED_DIMENSIONS {
        if required_external_dimensions(mailbox_admin, windows_required).contains(name) {
            continue;
        }
        let dimension = evidence.dimensions.get(name).ok_or_else(|| {
            ReleaseModelError::new(format!(
                "missing external compatibility dimension after parse: {name}"
            ))
        })?;
        if !dimension.decision.is_compatible() {
            warnings.push(format!(
                "{name} is {} but outside the selected deployment closure",
                if dimension.decision == CompatibilityDecision::Unknown {
                    "UNKNOWN"
                } else {
                    "INCOMPATIBLE"
                }
            ));
        }
    }
    let rollback_compatibility = if current_release.is_some() {
        "EVALUATED_IN_PROMOTION_PREFLIGHT"
    } else {
        "NOT_APPLICABLE"
    }
    .to_owned();
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
    pub fn machine_json(
        &self,
        release_set_id: &str,
        profile_id: &str,
        environment: &str,
    ) -> Value {
        json!({
            "schema_version": 2,
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
            "static_policy_authority": "opsctl.release.compatibility",
            "external_dimensions": REQUIRED_DIMENSIONS,
            "mutation_executed": false
        })
    }
}

fn blocked(code: &str, has_current_release: bool) -> CompatibilityResult {
    CompatibilityResult {
        compatible: false,
        blockers: vec![code.to_owned()],
        warnings: Vec::new(),
        required_steps: Vec::new(),
        rollback_compatibility: if has_current_release {
            "EVALUATED_IN_PROMOTION_PREFLIGHT"
        } else {
            "NOT_APPLICABLE"
        }
        .to_owned(),
    }
}

fn required_external_dimensions(
    mailbox_admin: bool,
    windows_required: bool,
) -> BTreeSet<&'static str> {
    let mut required = ["catalog_d1"].into_iter().collect::<BTreeSet<_>>();
    if mailbox_admin {
        required.insert("resolver_d1");
    }
    if windows_required {
        required.insert("windows_profile_bridge");
    }
    required
}

fn external_blocker_code(name: &str, unknown: bool) -> &'static str {
    match (name, unknown) {
        ("catalog_d1" | "resolver_d1", true) => "SCHEMA_COMPATIBILITY_UNKNOWN",
        ("catalog_d1" | "resolver_d1", false) => "SCHEMA_INCOMPATIBLE",
        ("windows_profile_bridge", true) => "WINDOWS_DELIVERY_UNKNOWN",
        ("windows_profile_bridge", false) => "WINDOWS_DELIVERY_UNSATISFIED",
        (_, true) => "PROVIDER_STATE_UNKNOWN",
        _ => "RELEASE_INCOMPATIBLE",
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

fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, ReleaseModelError> {
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
    fn v1_evidence_is_rejected() {
        let input = r#"{
          "schema_version":1,
          "kind":"RELEASE_COMPATIBILITY_EVIDENCE",
          "release_set_id":"release-set-v1-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "dimensions":{}
        }"#;
        assert!(CompatibilityEvidence::parse_json(input).is_err());
    }
}
