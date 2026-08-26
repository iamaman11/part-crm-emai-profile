use crate::canonical::canonical_json;
use opsctl_core::capability_policy;
use serde_json::{Value, json};
use std::fmt::{Display, Formatter};

pub const CAPABILITY_POLICY_MANIFEST_PATH: &str = "capability-policy-v1.json";
pub const CAPABILITY_POLICY_ARTIFACT_KIND: &str = "capability-policy";
const SCHEMA_VERSION: u64 = 1;
const KIND: &str = "CAPABILITY_POLICY_MANIFEST";
const SEMANTIC_OWNER: &str = "crates/capability-policy";
const PROJECTION_SOURCE: &str = "capability-policy::snapshot_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityPolicyManifestError {
    message: String,
}

impl CapabilityPolicyManifestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CapabilityPolicyManifestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CapabilityPolicyManifestError {}

/// Render the versioned durable projection of the typed capability-policy owner.
///
/// This adapter performs representation only. The JSON is generated output and is never read back
/// as policy input; all graph/profile/surface semantics come directly from `snapshot_v1()`.
pub fn render_json() -> Result<String, CapabilityPolicyManifestError> {
    let snapshot = capability_policy::snapshot_v1();
    let activation_units = snapshot
        .activation_units
        .into_iter()
        .map(|entry| {
            json!({
                "activation_unit": entry.unit.id(),
                "dependencies": entry.dependencies.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "incompatible_with": entry.incompatible_with.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "requires_windows_profile_bridge": entry.requires_windows_profile_bridge,
            })
        })
        .collect::<Vec<_>>();
    let profiles = snapshot
        .profiles
        .into_iter()
        .map(|entry| {
            json!({
                "profile_id": entry.profile_id.id(),
                "profile_version": entry.profile_version,
                "semantic_digest": entry.semantic_digest.to_hex(),
                "allowed_environments": entry.allowed_environments.into_iter().map(|environment| environment.id()).collect::<Vec<_>>(),
                "extends": entry.extends.map(|profile| profile.id()),
                "enabled_activation_units": entry.enabled_activation_units.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "disabled_activation_units": entry.disabled_activation_units.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "activation_gate": entry.activation_gate.id(),
                "production_authorization_required": entry.production_authorization_required,
            })
        })
        .collect::<Vec<_>>();
    let runtime_surfaces = snapshot
        .runtime_surfaces
        .into_iter()
        .map(|entry| {
            json!({
                "runtime_surface": entry.surface.id(),
                "activation_unit": entry.activation_unit.id(),
            })
        })
        .collect::<Vec<_>>();
    let document = json!({
        "schema_version": SCHEMA_VERSION,
        "kind": KIND,
        "semantic_owner": SEMANTIC_OWNER,
        "projection_source": PROJECTION_SOURCE,
        "activation_units": activation_units,
        "profiles": profiles,
        "runtime_surfaces": runtime_surfaces,
    });
    canonical_json(&document).map_err(|error| {
        CapabilityPolicyManifestError::new(format!(
            "cannot canonicalize capability policy manifest: {error}"
        ))
    })
}

pub fn render_bytes() -> Result<Vec<u8>, CapabilityPolicyManifestError> {
    render_json().map(String::into_bytes)
}

#[cfg(test)]
mod tests {
    use super::{KIND, PROJECTION_SOURCE, SCHEMA_VERSION, SEMANTIC_OWNER, render_json};
    use opsctl_core::capability_policy;
    use serde_json::Value;

    #[test]
    fn manifest_is_deterministic_projection_only() -> Result<(), String> {
        let first = render_json().map_err(|error| error.to_string())?;
        let second = render_json().map_err(|error| error.to_string())?;
        assert_eq!(first, second);
        let value: Value = serde_json::from_str(&first).map_err(|error| error.to_string())?;
        assert_eq!(value.get("schema_version").and_then(Value::as_u64), Some(SCHEMA_VERSION));
        assert_eq!(value.get("kind").and_then(Value::as_str), Some(KIND));
        assert_eq!(
            value.get("semantic_owner").and_then(Value::as_str),
            Some(SEMANTIC_OWNER)
        );
        assert_eq!(
            value.get("projection_source").and_then(Value::as_str),
            Some(PROJECTION_SOURCE)
        );
        let snapshot = capability_policy::snapshot_v1();
        assert_eq!(
            value
                .get("activation_units")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(snapshot.activation_units.len())
        );
        assert_eq!(
            value
                .get("profiles")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(snapshot.profiles.len())
        );
        assert_eq!(
            value
                .get("runtime_surfaces")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(snapshot.runtime_surfaces.len())
        );
        Ok(())
    }
}
