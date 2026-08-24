use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const FITNESS_BASELINE_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitnessRequiredness {
    Required,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitnessScope {
    BoundedContextDependencies,
    OpsctlPureCore,
    DurableContracts,
    WorkflowGovernance,
    CredentialProfileAuthority,
    ReleaseAdmission,
    PythonProductRuntime,
    HistoricalExecutables,
    WorkflowSecrets,
    ArchitectureLifecycle,
    HostedEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitnessRuleId {
    Af001BoundedContextDependencies,
    Af002OpsctlPureCoreEffects,
    Af003DurableContractVersioning,
    Af004ExactSourceWorkflowGovernance,
    Af005CredentialProfileAuthorityUniqueness,
    Af006ReleaseAdmissionAuthority,
    Af007PythonRuntimeRoleEffects,
    Af008HistoricalExecutableRetirement,
    Af009WorkflowSecretAuthority,
    Af010TypedLifecycleObservationPolicyBoundary,
    Af011TypedHostedEvidenceObservationPolicyBoundary,
}

impl FitnessRuleId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Af001BoundedContextDependencies => "AF-001",
            Self::Af002OpsctlPureCoreEffects => "AF-002",
            Self::Af003DurableContractVersioning => "AF-003",
            Self::Af004ExactSourceWorkflowGovernance => "AF-004",
            Self::Af005CredentialProfileAuthorityUniqueness => "AF-005",
            Self::Af006ReleaseAdmissionAuthority => "AF-006",
            Self::Af007PythonRuntimeRoleEffects => "AF-007",
            Self::Af008HistoricalExecutableRetirement => "AF-008",
            Self::Af009WorkflowSecretAuthority => "AF-009",
            Self::Af010TypedLifecycleObservationPolicyBoundary => "AF-010",
            Self::Af011TypedHostedEvidenceObservationPolicyBoundary => "AF-011",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitnessEnforcementOwner {
    ArchitectureDependencyBoundary,
    OpsctlReadOnlyBoundary,
    ContractCompatibilityBoundary,
    GithubActionsRuntimeBoundary,
    CredentialProfileAuthorityBoundary,
    ReleaseAdmissionBoundary,
    PythonRuntimeCutoverBoundary,
    HistoricalExecutableDebtBoundary,
    WorkflowSecretAuthorityBoundary,
    TypedLifecycleCore,
    TypedHostedEvidenceCore,
}

impl FitnessEnforcementOwner {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArchitectureDependencyBoundary => "scripts/check-architecture.py",
            Self::OpsctlReadOnlyBoundary => "scripts/check-opsctl-readonly.py",
            Self::ContractCompatibilityBoundary => "scripts/check-contract-compatibility.py",
            Self::GithubActionsRuntimeBoundary => "scripts/check-github-actions-runtime.py",
            Self::CredentialProfileAuthorityBoundary => {
                ".github/scripts/architecture-authority-check.mjs"
            }
            Self::ReleaseAdmissionBoundary => ".github/scripts/release-architecture-ar11.mjs",
            Self::PythonRuntimeCutoverBoundary => "scripts/check-ar10-runtime-cutover.py",
            Self::HistoricalExecutableDebtBoundary => "scripts/check-historical-executable-debt.py",
            Self::WorkflowSecretAuthorityBoundary => "scripts/check-workflow-secret-authority.py",
            Self::TypedLifecycleCore => "opsctl-core::architecture",
            Self::TypedHostedEvidenceCore => "opsctl-core::hosted_evidence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitnessNegativeProof {
    ForbiddenDomainFixture,
    OpsctlReadOnlySelfTest,
    BreakingContractFixture,
    GithubActionsRuntimeNegativeFixtures,
    CredentialProfileAuthoritySelfTest,
    ReleaseArchitectureSelfTest,
    Ar10RuntimeCutoverSelfTest,
    HistoricalExecutableDebtSelfTest,
    WorkflowSecretAuthoritySelfTest,
    TypedLifecycleCoreTests,
    TypedHostedEvidenceCoreTests,
}

impl FitnessNegativeProof {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForbiddenDomainFixture => "tests/architecture/fixtures/forbidden-domain",
            Self::OpsctlReadOnlySelfTest => "scripts/check-opsctl-readonly.py --self-test",
            Self::BreakingContractFixture => "tests/contracts/fixtures/breaking",
            Self::GithubActionsRuntimeNegativeFixtures => "tests/github-actions-runtime/fixtures",
            Self::CredentialProfileAuthoritySelfTest => {
                ".github/scripts/architecture-authority-check.mjs --self-test"
            }
            Self::ReleaseArchitectureSelfTest => {
                ".github/scripts/release-architecture-ar11.mjs --self-test"
            }
            Self::Ar10RuntimeCutoverSelfTest => "scripts/check-ar10-runtime-cutover.py --self-test",
            Self::HistoricalExecutableDebtSelfTest => {
                "scripts/check-historical-executable-debt.py --self-test"
            }
            Self::WorkflowSecretAuthoritySelfTest => {
                "scripts/check-workflow-secret-authority.py --self-test"
            }
            Self::TypedLifecycleCoreTests => "cargo test opsctl-core::architecture",
            Self::TypedHostedEvidenceCoreTests => "cargo test opsctl-core::hosted_evidence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitnessRuleDefinition {
    pub id: FitnessRuleId,
    pub requiredness: FitnessRequiredness,
    pub scope: FitnessScope,
    pub enforcement_owner: FitnessEnforcementOwner,
    pub negative_proof: FitnessNegativeProof,
    pub introduced_in_baseline: u16,
}

const BASELINE_V1_RULES: [FitnessRuleDefinition; 11] = [
    FitnessRuleDefinition {
        id: FitnessRuleId::Af001BoundedContextDependencies,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::BoundedContextDependencies,
        enforcement_owner: FitnessEnforcementOwner::ArchitectureDependencyBoundary,
        negative_proof: FitnessNegativeProof::ForbiddenDomainFixture,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af002OpsctlPureCoreEffects,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::OpsctlPureCore,
        enforcement_owner: FitnessEnforcementOwner::OpsctlReadOnlyBoundary,
        negative_proof: FitnessNegativeProof::OpsctlReadOnlySelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af003DurableContractVersioning,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::DurableContracts,
        enforcement_owner: FitnessEnforcementOwner::ContractCompatibilityBoundary,
        negative_proof: FitnessNegativeProof::BreakingContractFixture,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af004ExactSourceWorkflowGovernance,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::WorkflowGovernance,
        enforcement_owner: FitnessEnforcementOwner::GithubActionsRuntimeBoundary,
        negative_proof: FitnessNegativeProof::GithubActionsRuntimeNegativeFixtures,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af005CredentialProfileAuthorityUniqueness,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::CredentialProfileAuthority,
        enforcement_owner: FitnessEnforcementOwner::CredentialProfileAuthorityBoundary,
        negative_proof: FitnessNegativeProof::CredentialProfileAuthoritySelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af006ReleaseAdmissionAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::ReleaseAdmission,
        enforcement_owner: FitnessEnforcementOwner::ReleaseAdmissionBoundary,
        negative_proof: FitnessNegativeProof::ReleaseArchitectureSelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af007PythonRuntimeRoleEffects,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::PythonProductRuntime,
        enforcement_owner: FitnessEnforcementOwner::PythonRuntimeCutoverBoundary,
        negative_proof: FitnessNegativeProof::Ar10RuntimeCutoverSelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af008HistoricalExecutableRetirement,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::HistoricalExecutables,
        enforcement_owner: FitnessEnforcementOwner::HistoricalExecutableDebtBoundary,
        negative_proof: FitnessNegativeProof::HistoricalExecutableDebtSelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af009WorkflowSecretAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::WorkflowSecrets,
        enforcement_owner: FitnessEnforcementOwner::WorkflowSecretAuthorityBoundary,
        negative_proof: FitnessNegativeProof::WorkflowSecretAuthoritySelfTest,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af010TypedLifecycleObservationPolicyBoundary,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::ArchitectureLifecycle,
        enforcement_owner: FitnessEnforcementOwner::TypedLifecycleCore,
        negative_proof: FitnessNegativeProof::TypedLifecycleCoreTests,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
    FitnessRuleDefinition {
        id: FitnessRuleId::Af011TypedHostedEvidenceObservationPolicyBoundary,
        requiredness: FitnessRequiredness::Required,
        scope: FitnessScope::HostedEvidence,
        enforcement_owner: FitnessEnforcementOwner::TypedHostedEvidenceCore,
        negative_proof: FitnessNegativeProof::TypedHostedEvidenceCoreTests,
        introduced_in_baseline: FITNESS_BASELINE_VERSION,
    },
];

const CURRENT_RULES: [FitnessRuleDefinition; 11] = BASELINE_V1_RULES;
const CURRENT_SUPERSESSIONS: [FitnessRuleSupersession; 0] = [];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitnessRuleSupersession {
    pub predecessor: FitnessRuleId,
    pub successor: FitnessRuleId,
    pub reason: String,
    pub compatibility_security_impact: String,
    pub owning_slice: String,
    pub accepted_source_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitnessRegistryError {
    code: &'static str,
    detail: String,
}

impl FitnessRegistryError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Display for FitnessRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for FitnessRegistryError {}

#[must_use]
pub const fn baseline_v1_rules() -> &'static [FitnessRuleDefinition] {
    &BASELINE_V1_RULES
}

pub fn current_rules() -> Result<&'static [FitnessRuleDefinition], FitnessRegistryError> {
    validate_candidate_registry(&CURRENT_RULES, &CURRENT_SUPERSESSIONS)?;
    Ok(&CURRENT_RULES)
}

pub fn validate_candidate_registry(
    candidate: &[FitnessRuleDefinition],
    supersessions: &[FitnessRuleSupersession],
) -> Result<(), FitnessRegistryError> {
    validate_rule_set(candidate)?;
    validate_supersessions(candidate, supersessions)?;

    let candidate_by_id = candidate
        .iter()
        .map(|rule| (rule.id, rule))
        .collect::<BTreeMap<_, _>>();
    let supersession_by_predecessor = supersessions
        .iter()
        .map(|supersession| (supersession.predecessor, supersession))
        .collect::<BTreeMap<_, _>>();

    for baseline in &BASELINE_V1_RULES {
        if candidate_by_id.get(&baseline.id).is_some_and(|rule| **rule == *baseline) {
            continue;
        }
        let Some(supersession) = supersession_by_predecessor.get(&baseline.id) else {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_WEAKENING_WITHOUT_SUPERSESSION",
                format!(
                    "{} changed or disappeared without an explicit supersession",
                    baseline.id.as_str()
                ),
            ));
        };
        if candidate_by_id.contains_key(&baseline.id) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_ID_MEANING_CHANGED",
                format!(
                    "{} must be removed when superseded; rule meaning may not change under one ID",
                    baseline.id.as_str()
                ),
            ));
        }
        if !candidate_by_id.contains_key(&supersession.successor) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUCCESSOR_MISSING",
                format!(
                    "{} supersedes {} but is absent from the candidate registry",
                    supersession.successor.as_str(),
                    supersession.predecessor.as_str()
                ),
            ));
        }
    }
    Ok(())
}

fn validate_rule_set(candidate: &[FitnessRuleDefinition]) -> Result<(), FitnessRegistryError> {
    if candidate.is_empty() {
        return Err(FitnessRegistryError::new(
            "FITNESS_REGISTRY_EMPTY",
            "architecture fitness registry must contain required rules",
        ));
    }
    let mut ids = BTreeSet::new();
    for rule in candidate {
        if !ids.insert(rule.id) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_DUPLICATE",
                format!("duplicate rule {}", rule.id.as_str()),
            ));
        }
        if rule.introduced_in_baseline == 0 || rule.introduced_in_baseline > FITNESS_BASELINE_VERSION {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_BASELINE_VERSION_INVALID",
                format!("{} has an invalid baseline version", rule.id.as_str()),
            ));
        }
    }
    Ok(())
}

fn validate_supersessions(
    candidate: &[FitnessRuleDefinition],
    supersessions: &[FitnessRuleSupersession],
) -> Result<(), FitnessRegistryError> {
    let candidate_ids = candidate.iter().map(|rule| rule.id).collect::<BTreeSet<_>>();
    let baseline_ids = BASELINE_V1_RULES
        .iter()
        .map(|rule| rule.id)
        .collect::<BTreeSet<_>>();
    let mut predecessors = BTreeSet::new();
    let mut successors = BTreeSet::new();

    for supersession in supersessions {
        if supersession.predecessor == supersession.successor {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUPERSESSION_SELF_REFERENCE",
                format!("{} cannot supersede itself", supersession.predecessor.as_str()),
            ));
        }
        if !baseline_ids.contains(&supersession.predecessor) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUPERSESSION_PREDECESSOR_UNKNOWN",
                format!(
                    "{} is not an immutable PF-3 baseline rule",
                    supersession.predecessor.as_str()
                ),
            ));
        }
        if !predecessors.insert(supersession.predecessor) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUPERSESSION_PREDECESSOR_DUPLICATE",
                format!(
                    "{} has more than one direct supersession",
                    supersession.predecessor.as_str()
                ),
            ));
        }
        if !successors.insert(supersession.successor) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUPERSESSION_SUCCESSOR_DUPLICATE",
                format!(
                    "{} is reused as multiple supersession targets",
                    supersession.successor.as_str()
                ),
            ));
        }
        if !candidate_ids.contains(&supersession.successor) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUCCESSOR_MISSING",
                format!("{} is not present in the candidate registry", supersession.successor.as_str()),
            ));
        }
        for (field, value) in [
            ("reason", supersession.reason.as_str()),
            (
                "compatibility_security_impact",
                supersession.compatibility_security_impact.as_str(),
            ),
            ("owning_slice", supersession.owning_slice.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(FitnessRegistryError::new(
                    "FITNESS_RULE_SUPERSESSION_METADATA_MISSING",
                    format!("{} is required for an explicit supersession", field),
                ));
            }
        }
        if !is_lower_hex_sha(&supersession.accepted_source_sha) {
            return Err(FitnessRegistryError::new(
                "FITNESS_RULE_SUPERSESSION_ACCEPTED_SOURCE_INVALID",
                "accepted_source_sha must be exactly 40 lowercase hexadecimal characters",
            ));
        }
    }
    Ok(())
}

fn is_lower_hex_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::{
        BASELINE_V1_RULES, FitnessEnforcementOwner, FitnessNegativeProof, FitnessRegistryError,
        FitnessRequiredness, FitnessRuleId, FitnessRuleSupersession, current_rules,
        validate_candidate_registry,
    };

    fn error_code(result: Result<(), FitnessRegistryError>) -> Option<&'static str> {
        result.err().map(|error| error.code())
    }

    #[test]
    fn current_registry_is_valid_and_all_v1_rules_are_required() -> Result<(), FitnessRegistryError> {
        let current = current_rules()?;
        assert_eq!(current.len(), 11);
        assert!(
            current
                .iter()
                .all(|rule| rule.requiredness == FitnessRequiredness::Required)
        );
        assert!(current.iter().all(|rule| !rule.enforcement_owner.as_str().is_empty()));
        assert!(current.iter().all(|rule| !rule.negative_proof.as_str().is_empty()));
        Ok(())
    }

    #[test]
    fn silent_rule_removal_is_rejected() {
        let candidate = BASELINE_V1_RULES
            .iter()
            .copied()
            .filter(|rule| rule.id != FitnessRuleId::Af002OpsctlPureCoreEffects)
            .collect::<Vec<_>>();
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[])),
            Some("FITNESS_RULE_WEAKENING_WITHOUT_SUPERSESSION")
        );
    }

    #[test]
    fn silent_requiredness_downgrade_is_rejected() {
        let mut candidate = BASELINE_V1_RULES;
        candidate[0].requiredness = FitnessRequiredness::Advisory;
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[])),
            Some("FITNESS_RULE_WEAKENING_WITHOUT_SUPERSESSION")
        );
    }

    #[test]
    fn silent_enforcement_owner_replacement_is_rejected() {
        let mut candidate = BASELINE_V1_RULES;
        candidate[2].enforcement_owner = FitnessEnforcementOwner::OpsctlReadOnlyBoundary;
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[])),
            Some("FITNESS_RULE_WEAKENING_WITHOUT_SUPERSESSION")
        );
    }

    #[test]
    fn silent_negative_proof_replacement_is_rejected() {
        let mut candidate = BASELINE_V1_RULES;
        candidate[3].negative_proof = FitnessNegativeProof::OpsctlReadOnlySelfTest;
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[])),
            Some("FITNESS_RULE_WEAKENING_WITHOUT_SUPERSESSION")
        );
    }

    #[test]
    fn incomplete_supersession_metadata_is_rejected() {
        let candidate = BASELINE_V1_RULES
            .iter()
            .copied()
            .filter(|rule| rule.id != FitnessRuleId::Af001BoundedContextDependencies)
            .collect::<Vec<_>>();
        let supersession = FitnessRuleSupersession {
            predecessor: FitnessRuleId::Af001BoundedContextDependencies,
            successor: FitnessRuleId::Af002OpsctlPureCoreEffects,
            reason: String::new(),
            compatibility_security_impact: "no security weakening".to_owned(),
            owning_slice: "future-slice".to_owned(),
            accepted_source_sha: "a".repeat(40),
        };
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[supersession])),
            Some("FITNESS_RULE_SUPERSESSION_METADATA_MISSING")
        );
    }

    #[test]
    fn supersession_requires_accepted_source_sha() {
        let candidate = BASELINE_V1_RULES
            .iter()
            .copied()
            .filter(|rule| rule.id != FitnessRuleId::Af001BoundedContextDependencies)
            .collect::<Vec<_>>();
        let supersession = FitnessRuleSupersession {
            predecessor: FitnessRuleId::Af001BoundedContextDependencies,
            successor: FitnessRuleId::Af002OpsctlPureCoreEffects,
            reason: "ownership moved".to_owned(),
            compatibility_security_impact: "review required".to_owned(),
            owning_slice: "future-slice".to_owned(),
            accepted_source_sha: "not-a-commit".to_owned(),
        };
        assert_eq!(
            error_code(validate_candidate_registry(&candidate, &[supersession])),
            Some("FITNESS_RULE_SUPERSESSION_ACCEPTED_SOURCE_INVALID")
        );
    }
}
