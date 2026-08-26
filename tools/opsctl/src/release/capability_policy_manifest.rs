use crate::canonical::canonical_json;
use opsctl_core::capability_policy::{self, CapabilityPolicySnapshotV1};
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
    render_snapshot_json(capability_policy::snapshot_v1())
}

fn render_snapshot_json(
    mut snapshot: CapabilityPolicySnapshotV1,
) -> Result<String, CapabilityPolicyManifestError> {
    snapshot
        .activation_units
        .sort_by_key(|entry| entry.unit.id());
    snapshot
        .profiles
        .sort_by_key(|entry| entry.profile_id.id());
    snapshot
        .runtime_surfaces
        .sort_by_key(|entry| entry.surface.id());

    let activation_units = snapshot
        .activation_units
        .into_iter()
        .map(|entry| {
            let mut dependencies = entry.dependencies;
            dependencies.sort_by_key(|unit| unit.id());
            let mut incompatible_with = entry.incompatible_with;
            incompatible_with.sort_by_key(|unit| unit.id());
            json!({
                "activation_unit": entry.unit.id(),
                "dependencies": dependencies.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "incompatible_with": incompatible_with.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "requires_windows_profile_bridge": entry.requires_windows_profile_bridge,
            })
        })
        .collect::<Vec<_>>();
    let profiles = snapshot
        .profiles
        .into_iter()
        .map(|entry| {
            let mut allowed_environments = entry.allowed_environments;
            allowed_environments.sort_by_key(|environment| environment.id());
            let mut enabled_activation_units = entry.enabled_activation_units;
            enabled_activation_units.sort_by_key(|unit| unit.id());
            let mut disabled_activation_units = entry.disabled_activation_units;
            disabled_activation_units.sort_by_key(|unit| unit.id());
            json!({
                "profile_id": entry.profile_id.id(),
                "profile_version": entry.profile_version,
                "semantic_digest": entry.semantic_digest.to_hex(),
                "allowed_environments": allowed_environments.into_iter().map(|environment| environment.id()).collect::<Vec<_>>(),
                "extends": entry.extends.map(|profile| profile.id()),
                "enabled_activation_units": enabled_activation_units.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
                "disabled_activation_units": disabled_activation_units.into_iter().map(|unit| unit.id()).collect::<Vec<_>>(),
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
    use super::{
        KIND, PROJECTION_SOURCE, SCHEMA_VERSION, SEMANTIC_OWNER, render_json, render_snapshot_json,
    };
    use crate::canonical::sha256_hex;
    use opsctl_core::capability_policy;
    use serde_json::Value;

    const GOLDEN_BYTES: &[u8] =
        include_bytes!("../../tests/fixtures/capability-policy-v1.golden");
    const GOLDEN_SHA256: &str =
        "2360b6e82760d126dd29e617223e39a43127d908d966cd7a4adace9247c56060";
    const GOLDEN_SIZE_BYTES: usize = 6159;

    #[test]
    fn manifest_matches_canonical_bytes_and_sha_golden_vector() -> Result<(), String> {
        let rendered = render_json().map_err(|error| error.to_string())?;
        assert_eq!(rendered.as_bytes(), GOLDEN_BYTES);
        assert_eq!(rendered.len(), GOLDEN_SIZE_BYTES);
        assert_eq!(sha256_hex(rendered.as_bytes()), GOLDEN_SHA256);
        Ok(())
    }

    #[test]
    fn manifest_is_deterministic_projection_only() -> Result<(), String> {
        let first = render_json().map_err(|error| error.to_string())?;
        let second = render_json().map_err(|error| error.to_string())?;
        assert_eq!(first, second);
        let value: Value = serde_json::from_str(&first).map_err(|error| error.to_string())?;
        assert_eq!(
            value.get("schema_version").and_then(Value::as_u64),
            Some(SCHEMA_VERSION)
        );
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

    #[test]
    fn set_and_catalog_order_do_not_change_manifest_bytes() -> Result<(), String> {
        let canonical_snapshot = capability_policy::snapshot_v1();
        let mut reordered = canonical_snapshot.clone();
        reordered.activation_units.reverse();
        for entry in &mut reordered.activation_units {
            entry.dependencies.reverse();
            entry.incompatible_with.reverse();
        }
        reordered.profiles.reverse();
        for entry in &mut reordered.profiles {
            entry.allowed_environments.reverse();
            entry.enabled_activation_units.reverse();
            entry.disabled_activation_units.reverse();
        }
        reordered.runtime_surfaces.reverse();

        assert_eq!(
            render_snapshot_json(canonical_snapshot).map_err(|error| error.to_string())?,
            render_snapshot_json(reordered).map_err(|error| error.to_string())?
        );
        Ok(())
    }

    #[test]
    fn semantic_projection_change_changes_manifest_bytes() -> Result<(), String> {
        let baseline = capability_policy::snapshot_v1();
        let mut changed = baseline.clone();
        let Some(first) = changed.activation_units.first_mut() else {
            return Err("typed capability snapshot unexpectedly has no activation units".to_owned());
        };
        first.requires_windows_profile_bridge = !first.requires_windows_profile_bridge;

        assert_ne!(
            render_snapshot_json(baseline).map_err(|error| error.to_string())?,
            render_snapshot_json(changed).map_err(|error| error.to_string())?
        );
        Ok(())
    }
}
