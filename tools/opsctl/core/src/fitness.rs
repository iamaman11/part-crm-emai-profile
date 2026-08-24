//! PF-3 architecture fitness index.
//!
//! This module is deliberately small: it records stable rule identities and points to the
//! existing enforcement owner and negative proof for each rule. Enforcement logic stays in
//! those natural owners. Never repurpose an existing RuleId; introduce a successor RuleId and
//! an explicit supersession record instead.

pub const FITNESS_BASELINE_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitnessRequiredness {
    Required,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitnessRuleId {
    Af001BoundedContextDependencies,
    Af002OpsctlEffectBoundary,
    Af003PublicApiContractCompatibility,
    Af004ExactSourceWorkflowGovernance,
    Af005CredentialProfileAuthorityUniqueness,
    Af006ReleaseAdmissionAuthority,
    Af007PythonRuntimeRoleEffects,
    Af008HistoricalExecutableRetirement,
    Af009WorkflowSecretAuthority,
    Af010TypedLifecycleAuthority,
    Af011TypedHostedEvidenceAuthority,
    Af012DoctorSemanticComposition,
    Af013RetiredArchitectureSemanticInputs,
    Af014ReleaseSetHistoricalIsolation,
    #[cfg(test)]
    TestSuccessor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitnessRule {
    pub id: FitnessRuleId,
    pub requiredness: FitnessRequiredness,
    pub scope: &'static str,
    pub statement: &'static str,
    pub enforcement_owner: &'static str,
    pub negative_proof: &'static str,
}

const BASELINE_V1_RULES: [FitnessRule; 14] = [
    FitnessRule {
        id: FitnessRuleId::Af001BoundedContextDependencies,
        requiredness: FitnessRequiredness::Required,
        scope: "bounded-context dependency direction",
        statement: "Pure application/domain crates keep dependencies inward and exclude provider/runtime dependencies.",
        enforcement_owner: "scripts/check-architecture.py",
        negative_proof: "tests/architecture/fixtures/forbidden-domain",
    },
    FitnessRule {
        id: FitnessRuleId::Af002OpsctlEffectBoundary,
        requiredness: FitnessRequiredness::Required,
        scope: "opsctl pure-core and effect-shell boundary",
        statement: "opsctl-core has no filesystem/process/network/provider/serde_json authority; Product Runtime does not depend on opsctl; opsctl has no provider/network/process mutation authority.",
        enforcement_owner: "scripts/check-opsctl-readonly.py",
        negative_proof: "scripts/check-opsctl-readonly.py embedded negative self-tests",
    },
    FitnessRule {
        id: FitnessRuleId::Af003PublicApiContractCompatibility,
        requiredness: FitnessRequiredness::Required,
        scope: "accepted OpenAPI/protobuf v1 contracts",
        statement: "Accepted public API v1 baselines are immutable and breaking OpenAPI/protobuf compatibility changes fail closed.",
        enforcement_owner: "scripts/check-contract-compatibility.py + scripts/check-contract-baseline-immutable.sh",
        negative_proof: "tests/contracts/fixtures/breaking via scripts/check-contract-compatibility.py --current-root",
    },
    FitnessRule {
        id: FitnessRuleId::Af004ExactSourceWorkflowGovernance,
        requiredness: FitnessRequiredness::Required,
        scope: "GitHub Actions source-integrity boundary",
        statement: "Governed workflows use exact candidate source, immutable actions, non-persistent checkout credentials and no branch-push mutation authority.",
        enforcement_owner: "scripts/check-github-actions-runtime.py",
        negative_proof: "tests/github-actions-runtime/fixtures plus embedded post_merge_negative_self_test",
    },
    FitnessRule {
        id: FitnessRuleId::Af005CredentialProfileAuthorityUniqueness,
        requiredness: FitnessRequiredness::Required,
        scope: "credential/profile semantic authority",
        statement: "Credential/profile composition has one canonical mutable authority and retired competing authorities cannot be restored.",
        enforcement_owner: ".github/scripts/architecture-authority-check.mjs",
        negative_proof: ".github/scripts/architecture-authority-check.mjs --self-test",
    },
    FitnessRule {
        id: FitnessRuleId::Af006ReleaseAdmissionAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: "release capability and production enablement",
        statement: "Release capability and production enablement have one derived admission authority; independent enable flags and duplicate Release Set semantic authority are forbidden.",
        enforcement_owner: ".github/scripts/release-architecture-ar11.mjs",
        negative_proof: ".github/scripts/release-architecture-ar11.mjs --self-test",
    },
    FitnessRule {
        id: FitnessRuleId::Af007PythonRuntimeRoleEffects,
        requiredness: FitnessRequiredness::Required,
        scope: "Python product-runtime and opsctl child-process boundary",
        statement: "Python runtime entrypoints are classified; direct network/process/secret/deployment effects and opsctl Python/provider child authority are forbidden; real and synthetic Camoufox paths stay separated.",
        enforcement_owner: "scripts/check-ar10-runtime-cutover.py",
        negative_proof: "scripts/check-ar10-runtime-cutover.py --self-test",
    },
    FitnessRule {
        id: FitnessRuleId::Af008HistoricalExecutableRetirement,
        requiredness: FitnessRequiredness::Required,
        scope: "historical executable reachability",
        statement: "Accepted DEAD/historical executables remain unreachable from current execution and retired predecessor authority cannot be restored.",
        enforcement_owner: "scripts/check-historical-executable-debt.py",
        negative_proof: "scripts/check-historical-executable-debt.py --self-test",
    },
    FitnessRule {
        id: FitnessRuleId::Af009WorkflowSecretAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: "workflow secret authority",
        statement: "Workflow secret transport/mutation authority remains bounded to its canonical owner and competing secret authority is rejected.",
        enforcement_owner: "scripts/check-workflow-secret-authority.py",
        negative_proof: "scripts/check-workflow-secret-authority.py --self-test",
    },
    FitnessRule {
        id: FitnessRuleId::Af010TypedLifecycleAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: "architecture lifecycle semantics",
        statement: "Typed Rust lifecycle policy is the semantic owner; duplicate/unknown/non-contiguous observations fail closed and generated lifecycle projections are observers, not policy inputs.",
        enforcement_owner: "tools/opsctl/core/src/architecture.rs",
        negative_proof: "tools/opsctl/core/src/architecture.rs #[cfg(test)] negative cases",
    },
    FitnessRule {
        id: FitnessRuleId::Af011TypedHostedEvidenceAuthority,
        requiredness: FitnessRequiredness::Required,
        scope: "hosted evidence semantics",
        statement: "Typed Rust hosted-evidence policy owns trust/binding/freshness semantics and rejects foreign, mutating, unknown, failed or replayed observations.",
        enforcement_owner: "tools/opsctl/core/src/hosted_evidence.rs",
        negative_proof: "tools/opsctl/core/src/hosted_evidence.rs #[cfg(test)] negative cases",
    },
    FitnessRule {
        id: FitnessRuleId::Af012DoctorSemanticComposition,
        requiredness: FitnessRequiredness::Required,
        scope: "opsctl doctor local diagnostics",
        statement: "doctor remains local read-only typed structural diagnostics without a global authority bag, generic JSON policy aggregation, generated inventory input or runtime self-description.",
        enforcement_owner: "tools/opsctl/src/doctor.rs",
        negative_proof: "tools/opsctl/tests/pf3_architecture_fitness.rs::doctor_semantic_composition_stays_bounded",
    },
    FitnessRule {
        id: FitnessRuleId::Af013RetiredArchitectureSemanticInputs,
        requiredness: FitnessRequiredness::Required,
        scope: "retired/manual architecture semantic inputs",
        statement: "Manual architecture fitness JSON, generated architecture inventory and retired AR-qualified application ownership registry stay absent from the current repository.",
        enforcement_owner: "tools/opsctl/tests/pf3_architecture_fitness.rs",
        negative_proof: "tools/opsctl/tests/pf3_architecture_fitness.rs::retired_semantic_inputs_stay_absent",
    },
    FitnessRule {
        id: FitnessRuleId::Af014ReleaseSetHistoricalIsolation,
        requiredness: FitnessRequiredness::Required,
        scope: "Release Set current/historical contract boundary",
        statement: "Historical Release Set v2 is isolated to minimum-integrity compatibility, cannot be a current target, and obsolete v2 writer semantics cannot leak into current release code.",
        enforcement_owner: "tools/opsctl/tests/release_historical_isolation.rs",
        negative_proof: "tools/opsctl/tests/release_historical_isolation.rs negative historical/current isolation tests",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitnessRuleSupersession {
    pub predecessor: FitnessRuleId,
    pub successor: FitnessRuleId,
    pub reason: &'static str,
    pub compatibility_security_impact: &'static str,
    pub owning_slice: &'static str,
    pub accepted_source_sha: &'static str,
}

const CURRENT_SUPERSESSIONS: [FitnessRuleSupersession; 0] = [];

#[must_use]
pub const fn baseline_v1_rules() -> &'static [FitnessRule] {
    &BASELINE_V1_RULES
}

pub fn current_rules() -> Result<&'static [FitnessRule], &'static str> {
    validate_candidate_registry(&BASELINE_V1_RULES, &CURRENT_SUPERSESSIONS)?;
    Ok(&BASELINE_V1_RULES)
}

pub fn validate_candidate_registry(
    candidate: &[FitnessRule],
    supersessions: &[FitnessRuleSupersession],
) -> Result<(), &'static str> {
    validate_rules(candidate)?;
    validate_supersessions(candidate, supersessions)?;

    for baseline in &BASELINE_V1_RULES {
        match find_rule(candidate, baseline.id) {
            Some(current) if current == baseline => {}
            Some(_) => return Err("existing RuleId meaning changed"),
            None => {
                let supersession = supersessions
                    .iter()
                    .find(|entry| entry.predecessor == baseline.id)
                    .ok_or("required baseline rule removed without supersession")?;
                let successor = find_rule(candidate, supersession.successor)
                    .ok_or("supersession successor is absent")?;
                if baseline.requiredness == FitnessRequiredness::Required
                    && successor.requiredness != FitnessRequiredness::Required
                {
                    return Err("required rule superseded by non-required rule");
                }
            }
        }
    }

    Ok(())
}

fn validate_rules(rules: &[FitnessRule]) -> Result<(), &'static str> {
    for (index, rule) in rules.iter().enumerate() {
        if rule.scope.is_empty()
            || rule.statement.is_empty()
            || rule.enforcement_owner.is_empty()
            || rule.negative_proof.is_empty()
        {
            return Err("fitness rule metadata must be non-empty");
        }
        if rules[..index].iter().any(|other| other.id == rule.id) {
            return Err("duplicate fitness RuleId");
        }
    }
    Ok(())
}

fn validate_supersessions(
    candidate: &[FitnessRule],
    supersessions: &[FitnessRuleSupersession],
) -> Result<(), &'static str> {
    for (index, supersession) in supersessions.iter().enumerate() {
        if supersessions[..index]
            .iter()
            .any(|other| other.predecessor == supersession.predecessor)
        {
            return Err("duplicate supersession predecessor");
        }
        if supersession.predecessor == supersession.successor {
            return Err("supersession requires a new RuleId");
        }
        if find_rule(&BASELINE_V1_RULES, supersession.predecessor).is_none() {
            return Err("supersession predecessor is not in the accepted baseline");
        }
        if find_rule(&BASELINE_V1_RULES, supersession.successor).is_some() {
            return Err("supersession successor must use a new RuleId");
        }
        if find_rule(candidate, supersession.predecessor).is_some() {
            return Err("superseded predecessor must be removed");
        }
        if find_rule(candidate, supersession.successor).is_none() {
            return Err("supersession successor is absent");
        }
        if supersession.reason.is_empty()
            || supersession.compatibility_security_impact.is_empty()
            || supersession.owning_slice.is_empty()
        {
            return Err("supersession provenance is incomplete");
        }
        if !is_accepted_source_sha(supersession.accepted_source_sha) {
            return Err("supersession accepted source SHA is invalid");
        }
    }
    Ok(())
}

fn find_rule(rules: &[FitnessRule], id: FitnessRuleId) -> Option<&FitnessRule> {
    rules.iter().find(|rule| rule.id == id)
}

fn is_accepted_source_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::{
        BASELINE_V1_RULES, FitnessRequiredness, FitnessRule, FitnessRuleId,
        FitnessRuleSupersession, validate_candidate_registry,
    };

    fn candidate() -> Vec<FitnessRule> {
        BASELINE_V1_RULES.to_vec()
    }

    #[test]
    fn accepted_baseline_validates() {
        assert!(validate_candidate_registry(&candidate(), &[]).is_ok());
    }

    #[test]
    fn silent_rule_removal_fails() {
        let mut rules = candidate();
        rules.remove(0);
        assert!(validate_candidate_registry(&rules, &[]).is_err());
    }

    #[test]
    fn required_to_advisory_under_same_id_fails() {
        let mut rules = candidate();
        rules[0].requiredness = FitnessRequiredness::Advisory;
        assert!(validate_candidate_registry(&rules, &[]).is_err());
    }

    #[test]
    fn scope_or_meaning_change_under_same_id_fails() {
        let mut scope = candidate();
        scope[0].scope = "narrowed scope";
        assert!(validate_candidate_registry(&scope, &[]).is_err());

        let mut meaning = candidate();
        meaning[0].statement = "changed meaning";
        assert!(validate_candidate_registry(&meaning, &[]).is_err());
    }

    #[test]
    fn owner_or_negative_proof_change_under_same_id_fails() {
        let mut owner = candidate();
        owner[0].enforcement_owner = "other-owner";
        assert!(validate_candidate_registry(&owner, &[]).is_err());

        let mut proof = candidate();
        proof[0].negative_proof = "other-proof";
        assert!(validate_candidate_registry(&proof, &[]).is_err());
    }

    #[test]
    fn supersession_requires_complete_provenance_and_accepted_sha() {
        let (rules, mut supersession) = valid_supersession();
        supersession.reason = "";
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());

        let (rules, mut supersession) = valid_supersession();
        supersession.accepted_source_sha = "";
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());

        let (rules, mut supersession) = valid_supersession();
        supersession.accepted_source_sha = "deadbeef";
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());
    }

    #[test]
    fn supersession_same_predecessor_and_successor_fails() {
        let (rules, mut supersession) = valid_supersession();
        supersession.successor = supersession.predecessor;
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());
    }

    #[test]
    fn supersession_missing_successor_fails() {
        let (mut rules, supersession) = valid_supersession();
        rules.retain(|rule| rule.id != supersession.successor);
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());
    }

    #[test]
    fn required_predecessor_cannot_be_superseded_by_advisory_rule() {
        let (mut rules, supersession) = valid_supersession();
        let mut successor_found = false;
        for rule in &mut rules {
            if rule.id == supersession.successor {
                rule.requiredness = FitnessRequiredness::Advisory;
                successor_found = true;
            }
        }
        assert!(successor_found);
        assert!(validate_candidate_registry(&rules, &[supersession]).is_err());
    }

    #[test]
    fn duplicate_supersession_predecessor_fails() {
        let (rules, supersession) = valid_supersession();
        assert!(
            validate_candidate_registry(&rules, &[supersession, supersession]).is_err()
        );
    }

    #[test]
    fn explicit_required_successor_with_provenance_is_accepted() {
        let (rules, supersession) = valid_supersession();
        assert!(validate_candidate_registry(&rules, &[supersession]).is_ok());
    }

    fn valid_supersession() -> (Vec<FitnessRule>, FitnessRuleSupersession) {
        let mut rules = candidate();
        let predecessor = rules.remove(0);
        let successor = FitnessRule {
            id: FitnessRuleId::TestSuccessor,
            requiredness: FitnessRequiredness::Required,
            scope: "successor scope",
            statement: "successor meaning",
            enforcement_owner: "successor owner",
            negative_proof: "successor proof",
        };
        rules.push(successor);
        (
            rules,
            FitnessRuleSupersession {
                predecessor: predecessor.id,
                successor: successor.id,
                reason: "accepted architectural replacement",
                compatibility_security_impact: "no compatibility or security weakening",
                owning_slice: "PF-3",
                accepted_source_sha: "0123456789abcdef0123456789abcdef01234567",
            },
        )
    }
}
