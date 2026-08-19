use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

pub const DEFAULT_AUTHORITY_PATH: &str = "architecture/release-architecture-ar11.json";
const SUPPORTED_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationUnit {
    pub id: String,
    pub dependencies: Vec<String>,
    pub source_present: bool,
    pub accepted: bool,
    pub activation_gate: String,
    pub requires_windows_profile_bridge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseProfile {
    pub id: String,
    pub allowed_environments: Vec<String>,
    pub extends: Option<String>,
    pub enabled_activation_units: Vec<String>,
    pub disabled_activation_units: Vec<String>,
    pub activation_gate: String,
    pub current_authorization: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseArchitecture {
    pub activation_units: BTreeMap<String, ActivationUnit>,
    pub profiles: BTreeMap<String, ReleaseProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProfile {
    pub profile_id: String,
    pub environment: String,
    pub activation_units: BTreeMap<String, bool>,
}

impl EffectiveProfile {
    #[must_use]
    pub fn is_enabled(&self, activation_unit: &str) -> bool {
        self.activation_units
            .get(activation_unit)
            .copied()
            .unwrap_or(false)
    }
}

impl ReleaseArchitecture {
    pub fn load(root: &Path) -> Result<Self, ReleaseAuthorityError> {
        let path = root.join(DEFAULT_AUTHORITY_PATH);
        let input = fs::read_to_string(&path).map_err(|error| {
            ReleaseAuthorityError::new(format!(
                "failed to read {}: {error}",
                path.display()
            ))
        })?;
        Self::parse_json(&input)
    }

    pub fn parse_json(input: &str) -> Result<Self, ReleaseAuthorityError> {
        let value: Value = serde_json::from_str(input).map_err(|error| {
            ReleaseAuthorityError::new(format!("invalid release architecture JSON: {error}"))
        })?;
        let root = object(&value, "release architecture root")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "kind",
                "status",
                "owning_slice",
                "owning_issue",
                "canonical_projection",
                "source_path",
                "production_mutation",
                "architecture_complete",
                "production_core_gate",
                "production_ready",
                "principles",
                "activation_units",
                "release_profiles",
                "execution_surfaces",
                "deployment_closures",
                "artifact_authority",
                "promotion_policy",
                "effective_state_model",
                "component_release_owners",
                "compatibility_dimensions",
                "release_set",
            ],
            "release architecture root",
        )?;
        if required_u64(root, "schema_version")? != SUPPORTED_SCHEMA_VERSION {
            return Err(ReleaseAuthorityError::new(
                "unsupported release architecture schema_version",
            ));
        }
        if required_string(root, "kind")? != "AR11_RELEASE_ARCHITECTURE_SOURCE"
            || required_string(root, "owning_slice")? != "AR-11"
            || required_string(root, "canonical_projection")?
                != "architecture/inventory.json::release_architecture"
        {
            return Err(ReleaseAuthorityError::new(
                "release architecture identity/ownership drifted",
            ));
        }
        if required_bool(root, "production_mutation")?
            || required_bool(root, "architecture_complete")?
            || required_bool(root, "production_ready")?
            || required_string(root, "production_core_gate")? != "BLOCKED"
        {
            return Err(ReleaseAuthorityError::new(
                "AR-11 authority may not authorize production",
            ));
        }

        let units_array = array(required(root, "activation_units")?, "activation_units")?;
        if units_array.is_empty() {
            return Err(ReleaseAuthorityError::new(
                "activation_units must not be empty",
            ));
        }
        let mut activation_units = BTreeMap::new();
        for value in units_array {
            let unit = parse_activation_unit(value)?;
            if activation_units.insert(unit.id.clone(), unit).is_some() {
                return Err(ReleaseAuthorityError::new("duplicate activation_unit"));
            }
        }
        validate_dependency_graph(&activation_units)?;

        let profiles_array = array(required(root, "release_profiles")?, "release_profiles")?;
        if profiles_array.is_empty() {
            return Err(ReleaseAuthorityError::new(
                "release_profiles must not be empty",
            ));
        }
        let mut profiles = BTreeMap::new();
        for value in profiles_array {
            let profile = parse_profile(value)?;
            if profiles.insert(profile.id.clone(), profile).is_some() {
                return Err(ReleaseAuthorityError::new("duplicate release profile"));
            }
        }

        let architecture = Self {
            activation_units,
            profiles,
        };
        architecture.validate_profiles()?;
        Ok(architecture)
    }

    pub fn effective_profile(
        &self,
        profile_id: &str,
        environment: &str,
    ) -> Result<EffectiveProfile, ReleaseAuthorityError> {
        let profile = self.profiles.get(profile_id).ok_or_else(|| {
            ReleaseAuthorityError::new(format!(
                "PROFILE_NOT_AUTHORIZED: unknown profile {profile_id}"
            ))
        })?;
        if !profile
            .allowed_environments
            .iter()
            .any(|value| value == environment)
        {
            return Err(ReleaseAuthorityError::new(format!(
                "PROFILE_NOT_AUTHORIZED: {profile_id} is not allowed in {environment}"
            )));
        }

        let mut visiting = BTreeSet::new();
        let mut states = BTreeMap::new();
        self.apply_profile(profile_id, environment, &mut visiting, &mut states)?;
        self.validate_enabled_dependencies(&states)?;

        Ok(EffectiveProfile {
            profile_id: profile_id.to_owned(),
            environment: environment.to_owned(),
            activation_units: states,
        })
    }

    fn validate_profiles(&self) -> Result<(), ReleaseAuthorityError> {
        for profile in self.profiles.values() {
            if let Some(parent) = profile.extends.as_deref() {
                if parent == profile.id || !self.profiles.contains_key(parent) {
                    return Err(ReleaseAuthorityError::new(format!(
                        "invalid profile inheritance for {}",
                        profile.id
                    )));
                }
            }
            for unit in profile
                .enabled_activation_units
                .iter()
                .chain(profile.disabled_activation_units.iter())
            {
                if !self.activation_units.contains_key(unit) {
                    return Err(ReleaseAuthorityError::new(format!(
                        "profile {} references unknown activation unit {unit}",
                        profile.id
                    )));
                }
            }
            let enabled: BTreeSet<&str> = profile
                .enabled_activation_units
                .iter()
                .map(String::as_str)
                .collect();
            if profile
                .disabled_activation_units
                .iter()
                .any(|unit| enabled.contains(unit.as_str()))
            {
                return Err(ReleaseAuthorityError::new(format!(
                    "profile {} enables and disables the same activation unit",
                    profile.id
                )));
            }
            for environment in &profile.allowed_environments {
                self.effective_profile_without_environment_recheck(&profile.id, environment)?;
            }
        }
        Ok(())
    }

    fn effective_profile_without_environment_recheck(
        &self,
        profile_id: &str,
        environment: &str,
    ) -> Result<EffectiveProfile, ReleaseAuthorityError> {
        let mut visiting = BTreeSet::new();
        let mut states = BTreeMap::new();
        self.apply_profile(profile_id, environment, &mut visiting, &mut states)?;
        self.validate_enabled_dependencies(&states)?;
        Ok(EffectiveProfile {
            profile_id: profile_id.to_owned(),
            environment: environment.to_owned(),
            activation_units: states,
        })
    }

    fn apply_profile(
        &self,
        profile_id: &str,
        environment: &str,
        visiting: &mut BTreeSet<String>,
        states: &mut BTreeMap<String, bool>,
    ) -> Result<(), ReleaseAuthorityError> {
        if !visiting.insert(profile_id.to_owned()) {
            return Err(ReleaseAuthorityError::new("profile inheritance cycle"));
        }
        let profile = self
            .profiles
            .get(profile_id)
            .ok_or_else(|| ReleaseAuthorityError::new("profile disappeared during evaluation"))?;
        if !profile
            .allowed_environments
            .iter()
            .any(|value| value == environment)
        {
            visiting.remove(profile_id);
            return Err(ReleaseAuthorityError::new(format!(
                "PROFILE_NOT_AUTHORIZED: {profile_id} is not allowed in {environment}"
            )));
        }
        if let Some(parent) = profile.extends.as_deref() {
            self.apply_profile(parent, environment, visiting, states)?;
        } else {
            for unit in self.activation_units.keys() {
                states.insert(unit.clone(), false);
            }
        }
        for unit in &profile.enabled_activation_units {
            states.insert(unit.clone(), true);
        }
        for unit in &profile.disabled_activation_units {
            states.insert(unit.clone(), false);
        }
        visiting.remove(profile_id);
        Ok(())
    }

    fn validate_enabled_dependencies(
        &self,
        states: &BTreeMap<String, bool>,
    ) -> Result<(), ReleaseAuthorityError> {
        for (unit_id, enabled) in states {
            if !enabled {
                continue;
            }
            let unit = self.activation_units.get(unit_id).ok_or_else(|| {
                ReleaseAuthorityError::new(format!("unknown activation unit {unit_id}"))
            })?;
            if !unit.source_present || !unit.accepted {
                return Err(ReleaseAuthorityError::new(format!(
                    "CAPABILITY_DEPENDENCY_UNSATISFIED: {unit_id} is not accepted/source-present"
                )));
            }
            for dependency in &unit.dependencies {
                if !states.get(dependency).copied().unwrap_or(false) {
                    return Err(ReleaseAuthorityError::new(format!(
                        "CAPABILITY_DEPENDENCY_UNSATISFIED: {unit_id} requires {dependency}"
                    )));
                }
            }
        }
        Ok(())
    }
}

fn parse_activation_unit(value: &Value) -> Result<ActivationUnit, ReleaseAuthorityError> {
    let object = object(value, "activation_unit")?;
    let id = required_string(object, "activation_unit")?;
    if id.trim().is_empty() {
        return Err(ReleaseAuthorityError::new(
            "activation_unit id must not be empty",
        ));
    }
    Ok(ActivationUnit {
        id,
        dependencies: string_array(object, "dependencies")?,
        source_present: required_bool(object, "source_present")?,
        accepted: required_bool(object, "accepted")?,
        activation_gate: required_string(object, "activation_gate")?,
        requires_windows_profile_bridge: required_bool(
            object,
            "requires_windows_profile_bridge",
        )?,
    })
}

fn parse_profile(value: &Value) -> Result<ReleaseProfile, ReleaseAuthorityError> {
    let object = object(value, "release_profile")?;
    let id = required_string(object, "profile_id")?;
    if id.trim().is_empty() {
        return Err(ReleaseAuthorityError::new("profile_id must not be empty"));
    }
    Ok(ReleaseProfile {
        id,
        allowed_environments: string_array(object, "allowed_environments")?,
        extends: optional_string(object, "extends")?,
        enabled_activation_units: string_array(object, "enabled_activation_units")?,
        disabled_activation_units: string_array(object, "disabled_activation_units")?,
        activation_gate: required_string(object, "activation_gate")?,
        current_authorization: required_string(object, "current_authorization")?,
    })
}

fn validate_dependency_graph(
    units: &BTreeMap<String, ActivationUnit>,
) -> Result<(), ReleaseAuthorityError> {
    for (id, unit) in units {
        for dependency in &unit.dependencies {
            if dependency == id || !units.contains_key(dependency) {
                return Err(ReleaseAuthorityError::new(format!(
                    "invalid activation dependency {id} -> {dependency}"
                )));
            }
        }
    }
    for id in units.keys() {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        visit_dependency(id, units, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency(
    id: &str,
    units: &BTreeMap<String, ActivationUnit>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), ReleaseAuthorityError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(ReleaseAuthorityError::new(
            "activation dependency graph contains a cycle",
        ));
    }
    let unit = units
        .get(id)
        .ok_or_else(|| ReleaseAuthorityError::new("activation unit disappeared"))?;
    for dependency in &unit.dependencies {
        visit_dependency(dependency, units, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.to_owned());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAuthorityError {
    message: String,
}

impl ReleaseAuthorityError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReleaseAuthorityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseAuthorityError {}

fn required<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Value, ReleaseAuthorityError> {
    object
        .get(field)
        .ok_or_else(|| ReleaseAuthorityError::new(format!("missing required field {field}")))
}

fn object<'a>(
    value: &'a Value,
    label: &str,
) -> Result<&'a Map<String, Value>, ReleaseAuthorityError> {
    value
        .as_object()
        .ok_or_else(|| ReleaseAuthorityError::new(format!("{label} must be an object")))
}

fn array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>, ReleaseAuthorityError> {
    value
        .as_array()
        .ok_or_else(|| ReleaseAuthorityError::new(format!("{label} must be an array")))
}

fn required_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<String, ReleaseAuthorityError> {
    required(object, field)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ReleaseAuthorityError::new(format!("{field} must be a string")))
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, ReleaseAuthorityError> {
    match object.get(field) {
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| ReleaseAuthorityError::new(format!("{field} must be a string"))),
        None => Ok(None),
    }
}

fn required_bool(
    object: &Map<String, Value>,
    field: &str,
) -> Result<bool, ReleaseAuthorityError> {
    required(object, field)?
        .as_bool()
        .ok_or_else(|| ReleaseAuthorityError::new(format!("{field} must be a boolean")))
}

fn required_u64(
    object: &Map<String, Value>,
    field: &str,
) -> Result<u64, ReleaseAuthorityError> {
    required(object, field)?
        .as_u64()
        .ok_or_else(|| ReleaseAuthorityError::new(format!("{field} must be an unsigned integer")))
}

fn string_array(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Vec<String>, ReleaseAuthorityError> {
    let values = array(required(object, field)?, field)?;
    let mut result = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| ReleaseAuthorityError::new(format!("{field} must contain strings")))?;
        if !seen.insert(text.to_owned()) {
            return Err(ReleaseAuthorityError::new(format!(
                "{field} contains duplicate value {text}"
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
) -> Result<(), ReleaseAuthorityError> {
    for field in object.keys() {
        if !allowed.iter().any(|allowed_field| field == allowed_field) {
            return Err(ReleaseAuthorityError::new(format!(
                "unknown {label} field {field}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ReleaseArchitecture;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn production_core_is_fail_closed_for_mail_capabilities()
    -> Result<(), Box<dyn std::error::Error>> {
        let architecture = ReleaseArchitecture::load(&root())?;
        let profile = architecture.effective_profile("production-core-v1", "production")?;
        assert!(profile.is_enabled("identity"));
        assert!(profile.is_enabled("clients"));
        assert!(profile.is_enabled("browser_profiles"));
        assert!(profile.is_enabled("profile_runtime"));
        assert!(profile.is_enabled("camoufox"));
        assert!(!profile.is_enabled("mailbox_admin"));
        assert!(!profile.is_enabled("mailbox_jobs"));
        assert!(!profile.is_enabled("outbound_mail"));
        Ok(())
    }

    #[test]
    fn dependent_capability_cannot_be_enabled_without_dependencies() {
        let source = r#"{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "status":"fixture",
          "owning_slice":"AR-11",
          "owning_issue":372,
          "canonical_projection":"architecture/inventory.json::release_architecture",
          "source_path":"fixture",
          "production_mutation":false,
          "architecture_complete":false,
          "production_core_gate":"BLOCKED",
          "production_ready":false,
          "principles":{},
          "activation_units":[
            {"activation_unit":"foundation","architecture_owner":"foundation","application_owner":"foundation","source_present":true,"accepted":true,"dependencies":[],"incompatible_with":[],"activation_gate":"FOUNDATION","requires_windows_profile_bridge":false},
            {"activation_unit":"outbound_mail","architecture_owner":"mail","application_owner":"mail","source_present":true,"accepted":true,"dependencies":["foundation"],"incompatible_with":[],"activation_gate":"PC-4","requires_windows_profile_bridge":false}
          ],
          "release_profiles":[
            {"profile_id":"bad","profile_version":1,"allowed_environments":["production"],"enabled_activation_units":["outbound_mail"],"disabled_activation_units":["foundation"],"activation_gate":"PC-4","current_authorization":"BLOCKED"}
          ],
          "execution_surfaces":[],
          "deployment_closures":[],
          "artifact_authority":{},
          "promotion_policy":{},
          "effective_state_model":{},
          "component_release_owners":[],
          "compatibility_dimensions":[],
          "release_set":{}
        }"#;
        let result = ReleaseArchitecture::parse_json(source);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_top_level_state_is_rejected() {
        let source = r#"{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "status":"fixture",
          "owning_slice":"AR-11",
          "owning_issue":372,
          "canonical_projection":"architecture/inventory.json::release_architecture",
          "source_path":"fixture",
          "production_mutation":false,
          "architecture_complete":false,
          "production_core_gate":"BLOCKED",
          "production_ready":false,
          "principles":{},
          "activation_units":[],
          "release_profiles":[],
          "execution_surfaces":[],
          "deployment_closures":[],
          "artifact_authority":{},
          "promotion_policy":{},
          "effective_state_model":{},
          "component_release_owners":[],
          "compatibility_dimensions":[],
          "release_set":{},
          "unexpected":true
        }"#;
        let result = ReleaseArchitecture::parse_json(source);
        assert!(result.is_err());
    }
}
