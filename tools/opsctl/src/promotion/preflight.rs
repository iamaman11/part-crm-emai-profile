use crate::promotion::plan::{PlanRequest, PromotionPlan, build};
use crate::promotion::snapshot::DeploymentSnapshot;
use crate::release::compatibility::CompatibilityEvidence;
use crate::release::document::LoadedReleaseSet;
use crate::release::model::ReleaseModelError;
#[cfg(test)]
use crate::release::model::CompatibilityDecision;
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
    pub target: &'a LoadedReleaseSet,
    pub target_profile_id: &'a str,
    pub environment: &'a str,
    pub snapshot: &'a DeploymentSnapshot,
    pub compatibility_evidence: &'a CompatibilityEvidence,
    pub current_release: Option<&'a LoadedReleaseSet>,
    pub known_good_release: Option<&'a LoadedReleaseSet>,
    pub expected_current_release_set_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackDecision {
    Compatible,
    Incompatible,
    Unknown,
    NotApplicable,
}

impl RollbackDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "COMPATIBLE",
            Self::Incompatible => "INCOMPATIBLE",
            Self::Unknown => "UNKNOWN",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackDiagnostic {
    pub decision: RollbackDecision,
    pub reason_code: String,
    pub summary: String,
    pub remediation: String,
}

impl RollbackDiagnostic {
    fn new(
        decision: RollbackDecision,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            decision,
            reason_code: reason_code.into(),
            summary: summary.into(),
            remediation: remediation.into(),
        }
    }

    fn compatible() -> Self {
        Self::new(
            RollbackDecision::Compatible,
            "ROLLBACK_COMPATIBLE",
            "rollback Release Set is compatible with the observed deployment state",
            "NONE",
        )
    }

    fn incompatible(
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self::new(
            RollbackDecision::Incompatible,
            reason_code,
            summary,
            remediation,
        )
    }

    fn unknown(
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self::new(
            RollbackDecision::Unknown,
            reason_code,
            summary,
            remediation,
        )
    }

    fn not_applicable() -> Self {
        Self::new(
            RollbackDecision::NotApplicable,
            "NO_PREVIOUS_RELEASE_SET",
            "fresh environment has no previous Release Set; rollback compatibility is not applicable",
            "NONE",
        )
    }

    #[must_use]
    pub fn machine_json(&self) -> Value {
        json!({
            "decision": self.decision.as_str(),
            "reason_code": self.reason_code,
            "summary": self.summary,
            "remediation": self.remediation
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreflightResult {
    pub ready: bool,
    pub promotion_id: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub required_steps: Vec<String>,
    pub rollback_compatibility: String,
    pub rollback_diagnostic: RollbackDiagnostic,
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
            "rollback_diagnostic": self.rollback_diagnostic.machine_json(),
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

    let rollback_diagnostic = if request.snapshot.release_set_id.is_some() {
        match (request.current_release, request.known_good_release) {
            (Some(_), Some(known_good)) => {
                let diagnostic = evaluate_rollback_candidate_diagnostic(
                    known_good,
                    request.snapshot,
                    request.target_profile_id,
                    plan.closure.required_resources.contains("resolver_d1"),
                    request.environment == "production",
                );
                match diagnostic.decision {
                    RollbackDecision::Compatible => {}
                    RollbackDecision::Incompatible => {
                        blockers.push("ROLLBACK_INCOMPATIBLE".to_owned());
                    }
                    RollbackDecision::Unknown => {
                        blockers.push("ROLLBACK_COMPATIBILITY_UNKNOWN".to_owned());
                    }
                    RollbackDecision::NotApplicable => unreachable!(
                        "rollback candidate evaluation is only called for an existing deployment"
                    ),
                }
                diagnostic
            }
            (None, _) => {
                blockers.push("PROVIDER_STATE_UNKNOWN".to_owned());
                RollbackDiagnostic::unknown(
                    "CURRENT_RELEASE_OBSERVATION_MISSING",
                    "the current deployed Release Set could not be resolved from provider observation",
                    "collect a fresh provider deployment observation that resolves the current Release Set before promotion",
                )
            }
            (_, None) => {
                blockers.push("ROLLBACK_CANDIDATE_UNAVAILABLE".to_owned());
                RollbackDiagnostic::unknown(
                    "ROLLBACK_CANDIDATE_UNAVAILABLE",
                    "no immutable verified known-good Release Set is available for rollback evaluation",
                    "resolve an immutable verified known-good Release Set before promotion",
                )
            }
        }
    } else {
        warnings.push(
            "fresh environment has no previous Release Set; rollback artifact is not applicable"
                .to_owned(),
        );
        RollbackDiagnostic::not_applicable()
    };
    let rollback_compatibility = rollback_diagnostic.decision.as_str().to_owned();

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
        rollback_diagnostic,
    })
}

/// Evaluate whether an immutable previously verified Release Set can run against the
/// actually observed current deployment state. Missing required observation is UNKNOWN
/// and therefore blocks mutation. Exact equality is intentionally strict for protocol and
/// runtime dimensions until an explicit compatibility window is owned by a later authority.
fn evaluate_rollback_candidate_diagnostic(
    known_good: &LoadedReleaseSet,
    snapshot: &DeploymentSnapshot,
    profile_id: &str,
    resolver_required: bool,
    windows_delivery_required: bool,
) -> RollbackDiagnostic {
    let known_good = known_good.semantic();
    if !known_good
        .capability_profile_compatibility
        .iter()
        .any(|value| value == profile_id)
    {
        return RollbackDiagnostic::incompatible(
            "TARGET_PROFILE_UNSUPPORTED",
            format!("rollback Release Set does not support target capability profile {profile_id}"),
            "select a verified rollback Release Set that supports the target capability profile",
        );
    }

    let Some(catalog_revision) = snapshot.catalog_schema_revision.as_deref() else {
        return RollbackDiagnostic::unknown(
            "CATALOG_SCHEMA_OBSERVATION_MISSING",
            "Catalog D1 schema revision is missing from the provider observation",
            "collect the current Catalog D1 schema revision before promotion",
        );
    };
    if !known_good.schemas.catalog.supports(catalog_revision) {
        return RollbackDiagnostic::incompatible(
            "CATALOG_SCHEMA_UNSUPPORTED",
            format!(
                "rollback Release Set does not support observed Catalog D1 schema revision {catalog_revision}"
            ),
            "select a verified rollback Release Set that supports the observed Catalog D1 schema revision, or use the authorized schema recovery procedure before promotion",
        );
    }

    if resolver_required {
        let Some(resolver_revision) = snapshot.resolver_schema_revision.as_deref() else {
            return RollbackDiagnostic::unknown(
                "RESOLVER_SCHEMA_OBSERVATION_MISSING",
                "Resolver D1 schema revision is missing from the provider observation",
                "collect the current Resolver D1 schema revision before promotion",
            );
        };
        if !known_good.schemas.resolver.supports(resolver_revision) {
            return RollbackDiagnostic::incompatible(
                "RESOLVER_SCHEMA_UNSUPPORTED",
                format!(
                    "rollback Release Set does not support observed Resolver D1 schema revision {resolver_revision}"
                ),
                "select a verified rollback Release Set that supports the observed Resolver D1 schema revision, or use the authorized schema recovery procedure before promotion",
            );
        }
        let Some(resolver_protocol) = snapshot.resolver_protocol.as_deref() else {
            return RollbackDiagnostic::unknown(
                "RESOLVER_PROTOCOL_OBSERVATION_MISSING",
                "Resolver protocol identity is missing from the provider observation",
                "collect the current Resolver protocol identity before promotion",
            );
        };
        if known_good.protocols.resolver_protocol != resolver_protocol {
            return RollbackDiagnostic::incompatible(
                "RESOLVER_PROTOCOL_MISMATCH",
                format!(
                    "rollback Release Set requires Resolver protocol {} but provider observation reports {resolver_protocol}",
                    known_good.protocols.resolver_protocol
                ),
                "select a rollback Release Set matching the observed Resolver protocol or restore the compatible protocol through its authorized owner before promotion",
            );
        }
    }

    let Some(contracts_sha256) = snapshot.contracts_sha256.as_deref() else {
        return RollbackDiagnostic::unknown(
            "CONTRACTS_OBSERVATION_MISSING",
            "deployed contract digest is missing from the provider observation",
            "collect the deployed contract digest before promotion",
        );
    };
    if known_good.contracts.sha256 != contracts_sha256 {
        return RollbackDiagnostic::incompatible(
            "CONTRACTS_MISMATCH",
            format!(
                "rollback Release Set contract digest {} does not match observed deployed contract digest {contracts_sha256}",
                known_good.contracts.sha256
            ),
            "select a rollback Release Set matching the observed contracts or restore compatible contracts through their authorized owner before promotion",
        );
    }

    let Some(camouhost_ipc_version) = snapshot.camouhost_ipc_version else {
        return RollbackDiagnostic::unknown(
            "CAMOUHOST_IPC_OBSERVATION_MISSING",
            "Camouhost IPC version is missing from the provider observation",
            "collect the deployed Camouhost IPC version before promotion",
        );
    };
    if known_good.protocols.camouhost_ipc_version != camouhost_ipc_version {
        return RollbackDiagnostic::incompatible(
            "CAMOUHOST_IPC_MISMATCH",
            format!(
                "rollback Release Set requires Camouhost IPC version {} but provider observation reports {camouhost_ipc_version}",
                known_good.protocols.camouhost_ipc_version
            ),
            "select a rollback Release Set matching the observed Camouhost IPC version or restore the compatible runtime through its authorized owner before promotion",
        );
    }

    let Some(profile_bridge_protocol_version) = snapshot.profile_bridge_protocol_version else {
        return RollbackDiagnostic::unknown(
            "PROFILE_BRIDGE_PROTOCOL_OBSERVATION_MISSING",
            "Profile Bridge protocol version is missing from the provider observation",
            "collect the deployed Profile Bridge protocol version before promotion",
        );
    };
    if known_good.protocols.profile_bridge_protocol_version != profile_bridge_protocol_version {
        return RollbackDiagnostic::incompatible(
            "PROFILE_BRIDGE_PROTOCOL_MISMATCH",
            format!(
                "rollback Release Set requires Profile Bridge protocol version {} but provider observation reports {profile_bridge_protocol_version}",
                known_good.protocols.profile_bridge_protocol_version
            ),
            "select a rollback Release Set matching the observed Profile Bridge protocol version or restore the compatible runtime through its authorized owner before promotion",
        );
    }

    let Some(runtime_role) = snapshot.runtime_role.as_deref() else {
        return RollbackDiagnostic::unknown(
            "RUNTIME_ROLE_OBSERVATION_MISSING",
            "runtime role is missing from the provider observation",
            "collect the deployed runtime role before promotion",
        );
    };
    if known_good.runtime_compatibility.runtime_role != runtime_role {
        return RollbackDiagnostic::incompatible(
            "RUNTIME_ROLE_MISMATCH",
            format!(
                "rollback Release Set requires runtime role {} but provider observation reports {runtime_role}",
                known_good.runtime_compatibility.runtime_role
            ),
            "select a rollback Release Set matching the observed runtime role or restore the compatible runtime through its authorized owner before promotion",
        );
    }

    let Some(profile_format) = snapshot.profile_format.as_deref() else {
        return RollbackDiagnostic::unknown(
            "PROFILE_FORMAT_OBSERVATION_MISSING",
            "profile format is missing from the provider observation",
            "collect the deployed profile format before promotion",
        );
    };
    if known_good.runtime_compatibility.profile_format != profile_format {
        return RollbackDiagnostic::incompatible(
            "PROFILE_FORMAT_MISMATCH",
            format!(
                "rollback Release Set requires profile format {} but provider observation reports {profile_format}",
                known_good.runtime_compatibility.profile_format
            ),
            "select a rollback Release Set matching the observed profile format or restore a compatible format through its authorized owner before promotion",
        );
    }

    let Some(browser_identity_policy) = snapshot.browser_identity_policy.as_deref() else {
        return RollbackDiagnostic::unknown(
            "BROWSER_IDENTITY_POLICY_OBSERVATION_MISSING",
            "browser identity policy is missing from the provider observation",
            "collect the deployed browser identity policy before promotion",
        );
    };
    if known_good.runtime_compatibility.browser_identity_policy != browser_identity_policy {
        return RollbackDiagnostic::incompatible(
            "BROWSER_IDENTITY_POLICY_MISMATCH",
            format!(
                "rollback Release Set requires browser identity policy {} but provider observation reports {browser_identity_policy}",
                known_good.runtime_compatibility.browser_identity_policy
            ),
            "select a rollback Release Set matching the observed browser identity policy or restore the compatible policy through its authorized owner before promotion",
        );
    }

    if windows_delivery_required {
        return RollbackDiagnostic::unknown(
            "WINDOWS_DELIVERY_COMPATIBILITY_UNKNOWN",
            "rollback Windows delivery compatibility is not proven for this production closure",
            "provide authoritative Windows delivery compatibility evidence before relying on this rollback candidate",
        );
    }

    RollbackDiagnostic::compatible()
}

#[cfg(test)]
fn evaluate_rollback_candidate(
    known_good: &LoadedReleaseSet,
    snapshot: &DeploymentSnapshot,
    profile_id: &str,
    resolver_required: bool,
    windows_delivery_required: bool,
) -> CompatibilityDecision {
    match evaluate_rollback_candidate_diagnostic(
        known_good,
        snapshot,
        profile_id,
        resolver_required,
        windows_delivery_required,
    )
    .decision
    {
        RollbackDecision::Compatible => CompatibilityDecision::Compatible,
        RollbackDecision::Incompatible => CompatibilityDecision::Incompatible,
        RollbackDecision::Unknown => CompatibilityDecision::Unknown,
        RollbackDecision::NotApplicable => unreachable!(
            "rollback candidate evaluation cannot return NOT_APPLICABLE for an explicit candidate"
        ),
    }
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::{
        PreflightResult, RollbackDiagnostic, evaluate_rollback_candidate,
        evaluate_rollback_candidate_diagnostic,
    };
    use crate::promotion::snapshot::DeploymentSnapshot;
    use crate::release::digest::{canonical_json, sha256_hex};
    use crate::release::document::LoadedReleaseSet;
    use crate::release::model::{CompatibilityDecision, ReleaseModelError};
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REPO: &str = "iamaman11/part-crm-emai-profile";
    const HISTORICAL_PREFIX: &str = "release-set-v2-sha256-";

    fn release() -> Result<LoadedReleaseSet, Box<dyn std::error::Error>> {
        let accepted = sha256_hex(
            canonical_json(
                &json!({"authority":"accepted-main","commit_sha":GIT,"repository":REPO}),
            )?
            .as_bytes(),
        );
        let schema = |component: &str| json!({"database_component":component,"target_schema_revision":"0001_initial.sql","supported_schema_min":"0001_initial.sql","supported_schema_max":"0001_initial.sql","migration_history_digest":SHA,"compatibility_policy_digest":SHA});
        let component = |id: &str, path: &str| json!({"release_id":id,"source_commit_sha":GIT,"artifact_path":path,"artifact_sha256":SHA,"artifact_size_bytes":1,"component_manifest_sha256":SHA});
        let mut value = json!({
            "schema_version":2,
            "release_set_id":format!("{HISTORICAL_PREFIX}{SHA}"),
            "source":{"repository":REPO,"commit_sha":GIT,"accepted_main":true,"accepted_main_evidence_sha256":accepted},
            "components":{
                "control_plane":component("cp","components/control-plane.tar"),
                "secret_resolver":component("rs","components/secret-resolver.tar"),
                "runtime_bundle":component("rt","components/runtime-bundle.tar")
            },
            "contracts":{"files":[{"path":"openapi/v1/openapi.json","sha256":SHA,"size_bytes":1}],"sha256":SHA},
            "protocols":{"public_api_contract_sha256":SHA,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
            "schemas":{"d1_evolution_authority_sha256":SHA,"catalog":schema("catalog"),"resolver":schema("resolver")},
            "runtime_compatibility":{"runtime_lock_sha256":SHA,"runtime_role":"real_camoufox","profile_format":"v1","browser_identity_policy":"v1"},
            "capability_profile_compatibility":["rehearsal-core-v1"],
            "build_provenance":{"cargo_lock_sha256":SHA,"rust_toolchain_sha256":SHA,"frontend_lock_sha256":SHA,"release_architecture_sha256":SHA},
            "artifact_inventory":[
                {"path":"components/control-plane.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/secret-resolver.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/runtime-bundle.tar","sha256":SHA,"size_bytes":1,"kind":"component"}
            ]
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("release fixture root must be an object"))?
            .remove("release_set_id");
        value["release_set_id"] = Value::String(format!(
            "{HISTORICAL_PREFIX}{}",
            sha256_hex(canonical_json(&identity)?.as_bytes())
        ));
        let bytes = serde_json::to_vec(&value)?;
        Ok(LoadedReleaseSet::parse(&bytes)?)
    }

    fn snapshot() -> DeploymentSnapshot {
        DeploymentSnapshot {
            environment: "staging".to_owned(),
            collected_at: "2026-08-21T00:00:00Z".to_owned(),
            release_set_id: Some(format!("{HISTORICAL_PREFIX}{SHA}")),
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
    fn compatible_diagnostic_is_exact() -> Result<(), Box<dyn std::error::Error>> {
        let known_good = release()?;
        let diagnostic = evaluate_rollback_candidate_diagnostic(
            &known_good,
            &snapshot(),
            "rehearsal-core-v1",
            true,
            false,
        );
        assert_eq!(diagnostic.decision.as_str(), "COMPATIBLE");
        assert_eq!(diagnostic.reason_code, "ROLLBACK_COMPATIBLE");
        assert_eq!(diagnostic.remediation, "NONE");
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
        let diagnostic = evaluate_rollback_candidate_diagnostic(
            &known_good,
            &state,
            "rehearsal-core-v1",
            false,
            false,
        );
        assert_eq!(diagnostic.reason_code, "CATALOG_SCHEMA_UNSUPPORTED");
        assert!(diagnostic.summary.contains("9999_future.sql"));
        assert!(diagnostic.remediation.contains("Catalog D1 schema revision"));
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
        let diagnostic = evaluate_rollback_candidate_diagnostic(
            &known_good,
            &state,
            "rehearsal-core-v1",
            false,
            false,
        );
        assert_eq!(diagnostic.reason_code, "CATALOG_SCHEMA_OBSERVATION_MISSING");
        assert!(diagnostic.remediation.contains("collect"));
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
        let diagnostic = evaluate_rollback_candidate_diagnostic(
            &known_good,
            &snapshot(),
            "unknown-profile",
            false,
            false,
        );
        assert_eq!(diagnostic.reason_code, "TARGET_PROFILE_UNSUPPORTED");
        assert!(diagnostic.summary.contains("unknown-profile"));
        assert!(diagnostic.remediation.contains("target capability profile"));
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
        let diagnostic = evaluate_rollback_candidate_diagnostic(
            &known_good,
            &snapshot(),
            "rehearsal-core-v1",
            false,
            true,
        );
        assert_eq!(
            diagnostic.reason_code,
            "WINDOWS_DELIVERY_COMPATIBILITY_UNKNOWN"
        );
        assert!(diagnostic.remediation.contains("Windows delivery"));
        Ok(())
    }

    #[test]
    fn preflight_machine_json_emits_owner_rollback_diagnostic() {
        let result = PreflightResult {
            ready: false,
            promotion_id: "promotion-test".to_owned(),
            blockers: vec!["ROLLBACK_INCOMPATIBLE".to_owned()],
            warnings: Vec::new(),
            required_steps: Vec::new(),
            rollback_compatibility: "INCOMPATIBLE".to_owned(),
            rollback_diagnostic: RollbackDiagnostic::incompatible(
                "CATALOG_SCHEMA_UNSUPPORTED",
                "rollback cannot run against the observed Catalog schema",
                "select a compatible rollback Release Set",
            ),
        };
        let machine = result.machine_json("release-set-test", "profile-test", "staging");
        assert_eq!(machine["rollback_compatibility"], "INCOMPATIBLE");
        assert_eq!(machine["rollback_diagnostic"]["decision"], "INCOMPATIBLE");
        assert_eq!(
            machine["rollback_diagnostic"]["reason_code"],
            "CATALOG_SCHEMA_UNSUPPORTED"
        );
        assert_eq!(
            machine["rollback_diagnostic"]["remediation"],
            "select a compatible rollback Release Set"
        );
    }
}
