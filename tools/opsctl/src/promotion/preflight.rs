use crate::promotion::plan::{PlanRequest, PromotionPlan, build};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::model::{CompatibilityDecision, ReleaseModelError, ReleaseSetManifest};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

const DEPLOY_OWNED_RESOURCES: [&str; 4] = [
    "control_plane_worker",
    "profile_coordinator",
    "notification_hub",
    "control_plane_schedule",
];

pub struct PreflightRequest<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
    pub target: &'a ReleaseSetManifest,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
    pub current_release: Option<&'a ReleaseSetManifest>,
    pub known_good_release: Option<&'a ReleaseSetManifest>,
    pub expected_current_release_set_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreflightResult {
    pub ready: bool,
    pub promotion_id: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub required_steps: Vec<String>,
    pub rollback_compatibility: String,
}

impl PreflightResult {
    #[must_use]
    pub fn machine_json(
        &self,
        target_release_set_id: &str,
        target_profile_id: &str,
        environment: &str,
    ) -> Value {
        json!({
            "schema_version": 1,
            "command": "promotion.preflight",
            "decision": if self.ready { "READY" } else { "BLOCKED" },
            "ready": self.ready,
            "promotion_id": self.promotion_id,
            "environment": environment,
            "target_release_set_id": target_release_set_id,
            "target_capability_profile_id": target_profile_id,
            "rollback_compatibility": self.rollback_compatibility,
            "blockers": self.blockers,
            "warnings": self.warnings,
            "required_steps": self.required_steps,
            "credential_values_accessed": false,
            "provider_mutation_executed": false,
            "mutation_executed": false
        })
    }
}

pub fn evaluate(request: PreflightRequest<'_>) -> Result<PreflightResult, ReleaseModelError> {
    let plan = build(PlanRequest {
        root: request.root,
        source_root: request.source_root,
        target: request.target,
        target_profile_id: request.target_profile_id,
        environment: request.environment,
        snapshot: request.snapshot,
        compatibility_evidence: request.compatibility_evidence,
        current_release: request.current_release,
        expected_current_release_set_id: request.expected_current_release_set_id,
    })?;
    preflight_from_plan(request, plan)
}

fn preflight_from_plan(
    request: PreflightRequest<'_>,
    plan: PromotionPlan,
) -> Result<PreflightResult, ReleaseModelError> {
    let mut blockers = plan.blockers.clone();
    let mut warnings = plan.warnings.clone();
    let mut required_steps = plan.compatibility.required_steps.clone();

    let missing_resources = difference(
        &plan.closure.required_resources,
        &request.snapshot.logical_resources,
    );
    let missing_external_resources = missing_resources
        .iter()
        .filter(|resource| !DEPLOY_OWNED_RESOURCES.contains(&resource.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let missing_deploy_owned = missing_resources
        .iter()
        .filter(|resource| DEPLOY_OWNED_RESOURCES.contains(&resource.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let missing_bindings = difference(
        &plan.closure.required_bindings,
        &request.snapshot.logical_bindings,
    );
    let missing_credentials = difference(
        &plan.closure.required_credentials,
        &request.snapshot.logical_credentials,
    );
    if !missing_external_resources.is_empty() {
        blockers.push("REQUIRED_RESOURCES_NOT_READY".to_owned());
        required_steps.push(format!(
            "provision/identify required external resources: {}",
            missing_external_resources.join(",")
        ));
    }
    if !missing_deploy_owned.is_empty() {
        warnings.push(format!(
            "exact deployment will create/update deploy-owned resources: {}",
            missing_deploy_owned.join(",")
        ));
    }
    if !missing_bindings.is_empty() {
        blockers.push("REQUIRED_BINDINGS_NOT_READY".to_owned());
        required_steps.push(format!(
            "prepare required bindings: {}",
            missing_bindings.join(",")
        ));
    }
    if !missing_credentials.is_empty() {
        blockers.push("REQUIRED_CREDENTIAL_METADATA_NOT_READY".to_owned());
        required_steps.push(format!(
            "prepare credential metadata identities: {}",
            missing_credentials.join(",")
        ));
    }

    let rollback_compatibility = if request.snapshot.release_set_id.is_some() {
        match (request.current_release, request.known_good_release) {
            (Some(_), Some(known_good)) => {
                let result = evaluate_rollback_candidate(
                    known_good,
                    request.snapshot,
                    request.target_profile_id,
                    plan.closure.required_resources.contains("resolver_d1"),
                    request.environment == "production",
                );
                match result {
                    CompatibilityDecision::Compatible => "COMPATIBLE".to_owned(),
                    CompatibilityDecision::Incompatible => {
                        blockers.push("ROLLBACK_INCOMPATIBLE".to_owned());
                        "INCOMPATIBLE".to_owned()
                    }
                    CompatibilityDecision::Unknown => {
                        blockers.push("ROLLBACK_COMPATIBILITY_UNKNOWN".to_owned());
                        "UNKNOWN".to_owned()
                    }
                }
            }
            (None, _) => {
                blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
                "UNKNOWN".to_owned()
            }
            (_, None) => {
                blockers.push("ROLLBACK_CANDIDATE_UNAVAILABLE".to_owned());
                "UNKNOWN".to_owned()
            }
        }
    } else {
        warnings.push(
            "fresh environment has no previous Release Set; rollback artifact is not applicable"
                .to_owned(),
        );
        "NOT_APPLICABLE".to_owned()
    };

    if request.snapshot.catalog_ledger_sha256.is_none()
        || request.snapshot.catalog_schema_revision.is_none()
    {
        blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
        required_steps.push("collect Catalog D1 ledger + schema revision evidence".to_owned());
    }
    if plan.closure.required_resources.contains("resolver_d1")
        && (request.snapshot.resolver_ledger_sha256.is_none()
            || request.snapshot.resolver_schema_revision.is_none())
    {
        blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
        required_steps.push("collect Resolver D1 ledger + schema revision evidence".to_owned());
    }

    if request.environment == "production" {
        blockers.push("PRODUCTION_EXECUTION_BLOCKED_DURING_AR11".to_owned());
        required_steps
            .push("AR-17 authorization and PC-1 workflow authority are required".to_owned());
    }
    if plan.decision == "NO_CHANGE" {
        warnings.push("target already converged; provider mutation is unnecessary".to_owned());
    }

    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    required_steps.sort();
    required_steps.dedup();
    Ok(PreflightResult {
        ready: blockers.is_empty(),
        promotion_id: plan.promotion_id,
        blockers,
        warnings,
        required_steps,
        rollback_compatibility,
    })
}

/// Evaluate whether an immutable previously verified Release Set can run against the
/// actually observed current deployment state. Missing required observation is UNKNOWN
/// and therefore blocks mutation. Exact equality is intentionally strict for protocol and
/// runtime dimensions until an explicit compatibility window is owned by a later authority.
fn evaluate_rollback_candidate(
    known_good: &ReleaseSetManifest,
    snapshot: &DeploymentSnapshot,
    profile_id: &str,
    resolver_required: bool,
    windows_delivery_required: bool,
) -> CompatibilityDecision {
    if !known_good
        .capability_profile_compatibility
        .iter()
        .any(|value| value == profile_id)
    {
        return CompatibilityDecision::Incompatible;
    }

    let Some(catalog_revision) = snapshot.catalog_schema_revision.as_deref() else {
        return CompatibilityDecision::Unknown;
    };
    if !known_good.schemas.catalog.supports(catalog_revision) {
        return CompatibilityDecision::Incompatible;
    }

    if resolver_required {
        let Some(resolver_revision) = snapshot.resolver_schema_revision.as_deref() else {
            return CompatibilityDecision::Unknown;
        };
        if !known_good.schemas.resolver.supports(resolver_revision) {
            return CompatibilityDecision::Incompatible;
        }
        let Some(resolver_protocol) = snapshot.resolver_protocol.as_deref() else {
            return CompatibilityDecision::Unknown;
        };
        if known_good.protocols.resolver_protocol != resolver_protocol {
            return CompatibilityDecision::Incompatible;
        }
    }

    let (
        Some(contracts_sha256),
        Some(camouhost_ipc_version),
        Some(profile_bridge_protocol_version),
        Some(runtime_role),
        Some(profile_format),
        Some(browser_identity_policy),
    ) = (
        snapshot.contracts_sha256.as_deref(),
        snapshot.camouhost_ipc_version,
        snapshot.profile_bridge_protocol_version,
        snapshot.runtime_role.as_deref(),
        snapshot.profile_format.as_deref(),
        snapshot.browser_identity_policy.as_deref(),
    )
    else {
        return CompatibilityDecision::Unknown;
    };

    if known_good.contracts.sha256 != contracts_sha256
        || known_good.protocols.camouhost_ipc_version != camouhost_ipc_version
        || known_good.protocols.profile_bridge_protocol_version != profile_bridge_protocol_version
        || known_good.runtime_compatibility.runtime_role != runtime_role
        || known_good.runtime_compatibility.profile_format != profile_format
        || known_good.runtime_compatibility.browser_identity_policy != browser_identity_policy
    {
        return CompatibilityDecision::Incompatible;
    }

    if windows_delivery_required {
        // AR-15 owns signed Windows delivery compatibility. AR-11 must never invent it.
        return CompatibilityDecision::Unknown;
    }

    CompatibilityDecision::Compatible
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::evaluate_rollback_candidate;
    use crate::promotion::snapshot::DeploymentSnapshot;
    use crate::release::digest::{canonical_json, sha256_hex};
    use crate::release::model::{
        CompatibilityDecision, RELEASE_SET_ID_PREFIX, ReleaseModelError, ReleaseSetManifest,
        parse_json,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REPO: &str = "iamaman11/part-crm-emai-profile";

    fn release() -> Result<ReleaseSetManifest, Box<dyn std::error::Error>> {
        let accepted = sha256_hex(
            canonical_json(
                &json!({"authority":"accepted-main","commit_sha":GIT,"repository":REPO}),
            )?
            .as_bytes(),
        );
        let schema = |component: &str| json!({"database_component":component,"target_schema_revision":"0001_initial.sql","supported_schema_min":"0001_initial.sql","supported_schema_max":"0001_initial.sql","migration_history_digest":SHA,"compatibility_policy_digest":SHA});
        let component = |id: &str, path: &str| json!({"release_id":id,"source_commit_sha":GIT,"artifact_path":path,"artifact_sha256":SHA,"artifact_size_bytes":1,"component_manifest_sha256":SHA});
        let mut value = json!({
            "schema_version":3,
            "release_set_id":format!("{RELEASE_SET_ID_PREFIX}{SHA}"),
            "source":{"repository":REPO,"commit_sha":GIT,"accepted_main":true,"accepted_main_evidence_sha256":accepted},
            "components":{
                "control_plane":component("cp","components/control-plane.tar"),
                "secret_resolver":component("rs","components/secret-resolver.tar"),
                "runtime_bundle":component("rt","components/runtime-bundle.tar")
            },
            "contracts":{"files":[{"path":"openapi/v1/openapi.json","sha256":SHA,"size_bytes":1}],"sha256":SHA},
            "protocols":{"public_api_contract_sha256":SHA,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
            "schemas":{"d1_repository_identity_sha256":SHA,"catalog":schema("catalog"),"resolver":schema("resolver")},
            "runtime_compatibility":{"runtime_lock_sha256":SHA,"runtime_role":"real_camoufox","profile_format":"v1","browser_identity_policy":"v1"},
            "capability_profile_compatibility":["rehearsal-core-v1"],
            "build_provenance":{"cargo_lock_sha256":SHA,"rust_toolchain_sha256":SHA,"frontend_lock_sha256":SHA,"release_architecture_sha256":SHA},
            "artifact_inventory":[
                {"path":"components/control-plane.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/runtime-bundle.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/secret-resolver.tar","sha256":SHA,"size_bytes":1,"kind":"component"}
            ]
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("release fixture root must be an object"))?
            .remove("release_set_id");
        value["release_set_id"] = Value::String(format!(
            "{RELEASE_SET_ID_PREFIX}{}",
            sha256_hex(canonical_json(&identity)?.as_bytes())
        ));
        Ok(parse_json(&serde_json::to_string(&value)?)?)
    }

    fn snapshot() -> DeploymentSnapshot {
        DeploymentSnapshot {
            environment: "staging".to_owned(),
            collected_at: "2026-08-21T00:00:00Z".to_owned(),
            release_set_id: Some(format!("{RELEASE_SET_ID_PREFIX}{SHA}")),
            capability_profile_id: Some("rehearsal-core-v1".to_owned()),
            component_release_ids: Vec::new(),
            logical_resources: BTreeSet::new(),
            logical_bindings: BTreeSet::new(),
            logical_credentials: BTreeSet::new(),
            catalog_ledger_sha256: Some(SHA.to_owned()),
            catalog_schema_revision: Some("0001_initial.sql".to_owned()),
            resolver_ledger_sha256: Some(SHA.to_owned()),
            resolver_schema_revision: Some("0001_initial.sql".to_owned()),
            contracts_sha256: Some(SHA.to_owned()),
            resolver_protocol: Some("mailbox-secret-resolver-v1".to_owned()),
            camouhost_ipc_version: Some(1),
            profile_bridge_protocol_version: Some(1),
            runtime_role: Some("real_camoufox".to_owned()),
            profile_format: Some("v1".to_owned()),
            browser_identity_policy: Some("v1".to_owned()),
        }
    }

    #[test]
    fn compatible_known_good_is_compatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &snapshot(), "rehearsal-core-v1", true, false),
            CompatibilityDecision::Compatible
        );
        Ok(())
    }

    #[test]
    fn unsupported_catalog_schema_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.catalog_schema_revision = Some("9999_future.sql".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn missing_catalog_schema_is_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.catalog_schema_revision = None;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Unknown
        );
        Ok(())
    }

    #[test]
    fn unsupported_resolver_schema_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.resolver_schema_revision = Some("9999_future.sql".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", true, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn missing_resolver_schema_is_unknown_when_required() -> Result<(), Box<dyn std::error::Error>>
    {
        let known_good = release()?;
        let mut state = snapshot();
        state.resolver_schema_revision = None;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", true, false),
            CompatibilityDecision::Unknown
        );
        Ok(())
    }

    #[test]
    fn contracts_mismatch_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.contracts_sha256 =
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn camouhost_protocol_drift_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.camouhost_ipc_version = Some(2);
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn profile_bridge_protocol_drift_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.profile_bridge_protocol_version = Some(2);
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn runtime_role_drift_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.runtime_role = Some("fixture_runtime".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn profile_format_drift_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.profile_format = Some("v2".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn browser_identity_policy_drift_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.browser_identity_policy = Some("v2".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn missing_observation_is_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.contracts_sha256 = None;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", false, false),
            CompatibilityDecision::Unknown
        );
        Ok(())
    }

    #[test]
    fn wrong_profile_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &snapshot(), "unknown-profile", false, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn resolver_mismatch_is_incompatible() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let mut state = snapshot();
        state.resolver_protocol = Some("resolver-v2".to_owned());
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &state, "rehearsal-core-v1", true, false),
            CompatibilityDecision::Incompatible
        );
        Ok(())
    }

    #[test]
    fn unknown_windows_delivery_blocks() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        assert_eq!(
            evaluate_rollback_candidate(&known_good, &snapshot(), "rehearsal-core-v1", false, true),
            CompatibilityDecision::Unknown
        );
        Ok(())
    }
}
