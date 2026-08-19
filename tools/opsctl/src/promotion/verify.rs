use crate::promotion::authority::load_closure;
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::model::{CompatibilityDecision, ReleaseModelError, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationDecision {
    Verified,
    Drifted,
    Incomplete,
    Unknown,
}

impl VerificationDecision {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Drifted => "DRIFTED",
            Self::Incomplete => "INCOMPLETE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

pub struct VerifyRequest<'a> {
    pub root: &'a Path,
    pub target: &'a ReleaseSetManifest,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub decision: VerificationDecision,
    pub blockers: Vec<String>,
}

impl VerifyResult {
    #[must_use]
    pub fn machine_json(
        &self,
        target_release_set_id: &str,
        target_profile_id: &str,
        environment: &str,
    ) -> Value {
        json!({
            "schema_version": 1,
            "command": "promotion.verify",
            "decision": self.decision.name(),
            "verified": matches!(self.decision, VerificationDecision::Verified),
            "environment": environment,
            "target_release_set_id": target_release_set_id,
            "target_capability_profile_id": target_profile_id,
            "blockers": self.blockers,
            "mutation_executed": false
        })
    }
}

pub fn verify(request: VerifyRequest<'_>) -> Result<VerifyResult, ReleaseModelError> {
    if request.snapshot.environment != request.environment {
        return Ok(result(
            VerificationDecision::Drifted,
            vec!["ENVIRONMENT_IDENTITY_MISMATCH".to_owned()],
        ));
    }
    if request.compatibility_evidence.release_set_id != request.target.release_set_id {
        return Ok(result(
            VerificationDecision::Unknown,
            vec!["COMPATIBILITY_EVIDENCE_RELEASE_MISMATCH".to_owned()],
        ));
    }

    let closure = load_closure(request.root, request.target_profile_id)?;
    let mut drift = Vec::new();
    let mut incomplete = Vec::new();
    let mut unknown = Vec::new();

    match request.snapshot.release_set_id.as_deref() {
        Some(observed) if observed == request.target.release_set_id => {}
        Some(_) => drift.push("DEPLOYED_RELEASE_SET_MISMATCH".to_owned()),
        None => incomplete.push("DEPLOYED_RELEASE_SET_ID_MISSING".to_owned()),
    }
    match request.snapshot.capability_profile_id.as_deref() {
        Some(observed) if observed == request.target_profile_id => {}
        Some(_) => drift.push("ACTIVE_CAPABILITY_PROFILE_MISMATCH".to_owned()),
        None => incomplete.push("ACTIVE_CAPABILITY_PROFILE_ID_MISSING".to_owned()),
    }

    let expected_components = request
        .target
        .components
        .iter()
        .filter(|(id, _)| closure.required_components.contains(*id))
        .map(|(id, component)| (id.clone(), component.release_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let observed_components = request
        .snapshot
        .component_release_ids
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();
    for (id, expected) in &expected_components {
        match observed_components.get(id) {
            Some(observed) if observed == expected => {}
            Some(_) => drift.push(format!("COMPONENT_RELEASE_MISMATCH:{id}")),
            None => incomplete.push(format!("COMPONENT_RELEASE_MISSING:{id}")),
        }
    }

    compare_exact_set(
        "RESOURCE_CLOSURE",
        &closure.required_resources,
        &request.snapshot.logical_resources,
        &mut drift,
    );
    compare_exact_set(
        "BINDING_CLOSURE",
        &closure.required_bindings,
        &request.snapshot.logical_bindings,
        &mut drift,
    );
    compare_exact_set(
        "CREDENTIAL_METADATA_CLOSURE",
        &closure.required_credentials,
        &request.snapshot.logical_credentials,
        &mut drift,
    );

    if request.snapshot.catalog_ledger_sha256.is_none() {
        incomplete.push("CATALOG_D1_LEDGER_MISSING".to_owned());
    }
    if closure.required_resources.contains("resolver_d1")
        && request.snapshot.resolver_ledger_sha256.is_none()
    {
        incomplete.push("RESOLVER_D1_LEDGER_MISSING".to_owned());
    }

    for (name, dimension) in &request.compatibility_evidence.dimensions {
        let required = dimension_required(name, &closure, request.environment);
        if !required {
            continue;
        }
        match dimension.decision {
            CompatibilityDecision::Compatible => {}
            CompatibilityDecision::Incompatible => {
                drift.push(format!("COMPATIBILITY_INCOMPATIBLE:{name}"));
            }
            CompatibilityDecision::Unknown => {
                unknown.push(format!("COMPATIBILITY_UNKNOWN:{name}"));
            }
        }
    }

    let (decision, mut blockers) = if !unknown.is_empty() {
        (VerificationDecision::Unknown, unknown)
    } else if !incomplete.is_empty() {
        (VerificationDecision::Incomplete, incomplete)
    } else if !drift.is_empty() {
        (VerificationDecision::Drifted, drift)
    } else {
        (VerificationDecision::Verified, Vec::new())
    };
    blockers.sort();
    blockers.dedup();
    Ok(result(decision, blockers))
}

fn dimension_required(
    name: &str,
    closure: &crate::promotion::authority::DeploymentClosure,
    environment: &str,
) -> bool {
    match name {
        "resolver_d1" | "resolver_protocol" => closure.required_resources.contains("resolver_d1"),
        "windows_profile_bridge" => {
            environment == "production"
                && (closure.required_components.contains("profile_bridge")
                    || closure
                        .required_resources
                        .contains("windows_profile_bridge"))
        }
        _ => true,
    }
}

fn compare_exact_set(
    label: &str,
    expected: &BTreeSet<String>,
    observed: &BTreeSet<String>,
    drift: &mut Vec<String>,
) {
    for missing in expected.difference(observed) {
        drift.push(format!("{label}_MISSING:{missing}"));
    }
    for extra in observed.difference(expected) {
        drift.push(format!("{label}_UNEXPECTED:{extra}"));
    }
}

fn result(decision: VerificationDecision, blockers: Vec<String>) -> VerifyResult {
    VerifyResult { decision, blockers }
}

#[cfg(test)]
mod tests {
    use super::VerificationDecision;

    #[test]
    fn only_verified_is_success() {
        assert_eq!(VerificationDecision::Verified.name(), "VERIFIED");
        assert_ne!(VerificationDecision::Unknown.name(), "VERIFIED");
        assert_ne!(VerificationDecision::Incomplete.name(), "VERIFIED");
        assert_ne!(VerificationDecision::Drifted.name(), "VERIFIED");
    }
}
