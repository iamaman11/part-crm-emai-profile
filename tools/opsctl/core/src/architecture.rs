use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const RAW_ACCEPTANCE_EVIDENCE_SCHEMA_VERSION: u64 = 1;
pub const ACCEPTANCE_RECORD_SCHEMA_VERSION: u64 = 1;
pub const ACCEPTED_BASELINE: &str = "AR-11";
pub const ACCEPTANCE_SOURCE_BRANCH: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionCoreGate {
    Blocked,
    Authorized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSlice {
    pub id: String,
    pub predecessor: Option<String>,
    pub successor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceObservationV1 {
    pub record_schema_version: u64,
    pub slice: String,
    pub pr: u64,
    pub base_sha: String,
    pub candidate_sha: String,
    pub candidate_tree: String,
    pub observed_candidate_tree: String,
    pub merge_sha: String,
    pub merge_tree: String,
    pub observed_merge_tree: String,
    pub observed_merge_first_parent: String,
    pub tag_target_sha: String,
    pub required_status_contexts_total: u64,
    pub required_status_contexts_success: u64,
    pub applicable_permanent_workflows_total: u64,
    pub applicable_permanent_workflows_success: u64,
    pub behind_by: u64,
    pub blocking_reviews: u64,
    pub unresolved_review_threads: u64,
    pub accepted_main_reread: String,
    pub architecture_complete: bool,
    pub production_core_gate: ProductionCoreGate,
    pub production_ready: bool,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawArchitectureAcceptanceEvidenceV1 {
    pub schema_version: u64,
    pub source_branch: String,
    pub sequence: Vec<ProgramSlice>,
    pub acceptance_observations: Vec<AcceptanceObservationV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedLifecycleStateV1 {
    pub schema_version: u64,
    pub accepted_checkpoint: String,
    pub current_slice: Option<String>,
    pub architecture_complete: bool,
    pub production_core_gate: ProductionCoreGate,
    pub production_ready: bool,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleEvaluationError {
    message: String,
}

impl LifecycleEvaluationError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for LifecycleEvaluationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LifecycleEvaluationError {}

pub struct LifecycleEvaluator;

impl LifecycleEvaluator {
    pub fn evaluate(
        evidence: &RawArchitectureAcceptanceEvidenceV1,
    ) -> Result<DerivedLifecycleStateV1, LifecycleEvaluationError> {
        if evidence.schema_version != RAW_ACCEPTANCE_EVIDENCE_SCHEMA_VERSION {
            return Err(LifecycleEvaluationError::new(format!(
                "UNSUPPORTED_ACCEPTANCE_EVIDENCE_VERSION: expected {}, got {}",
                RAW_ACCEPTANCE_EVIDENCE_SCHEMA_VERSION, evidence.schema_version
            )));
        }
        if evidence.source_branch != ACCEPTANCE_SOURCE_BRANCH {
            return Err(LifecycleEvaluationError::new(format!(
                "INVALID_ACCEPTANCE_SOURCE_BRANCH: expected {ACCEPTANCE_SOURCE_BRANCH}, got {}",
                evidence.source_branch
            )));
        }

        let baseline_index = validate_sequence(&evidence.sequence)?;
        let mut observations = BTreeMap::new();
        for observation in &evidence.acceptance_observations {
            if observations
                .insert(observation.slice.as_str(), observation)
                .is_some()
            {
                return Err(LifecycleEvaluationError::new(format!(
                    "DUPLICATE_ACCEPTANCE_OBSERVATION: {}",
                    observation.slice
                )));
            }
        }

        for observation in &evidence.acceptance_observations {
            let index = evidence
                .sequence
                .iter()
                .position(|slice| slice.id == observation.slice)
                .ok_or_else(|| {
                    LifecycleEvaluationError::new(format!(
                        "UNKNOWN_ACCEPTANCE_SLICE: {}",
                        observation.slice
                    ))
                })?;
            if index <= baseline_index {
                return Err(LifecycleEvaluationError::new(format!(
                    "ACCEPTANCE_OBSERVATION_PRECEDES_TYPED_BASELINE: {}",
                    observation.slice
                )));
            }
        }

        let mut accepted_index = baseline_index;
        let mut gap_seen = false;
        for (index, slice) in evidence.sequence.iter().enumerate().skip(baseline_index + 1) {
            match observations.get(slice.id.as_str()) {
                Some(observation) => {
                    if gap_seen {
                        return Err(LifecycleEvaluationError::new(format!(
                            "NON_CONTIGUOUS_ACCEPTANCE: {} appears after a gap",
                            slice.id
                        )));
                    }
                    validate_observation(observation, &slice.id)?;
                    accepted_index = index;
                }
                None => gap_seen = true,
            }
        }

        let accepted = &evidence.sequence[accepted_index];
        let expected = expected_state(&accepted.id);
        Ok(DerivedLifecycleStateV1 {
            schema_version: 1,
            accepted_checkpoint: accepted.id.clone(),
            current_slice: accepted.successor.clone(),
            architecture_complete: expected.architecture_complete,
            production_core_gate: expected.production_core_gate,
            production_ready: expected.production_ready,
            production_mutation: expected.production_mutation,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpectedState {
    architecture_complete: bool,
    production_core_gate: ProductionCoreGate,
    production_ready: bool,
    production_mutation: bool,
}

fn expected_state(slice: &str) -> ExpectedState {
    if slice == "AR-17" {
        ExpectedState {
            architecture_complete: true,
            production_core_gate: ProductionCoreGate::Authorized,
            production_ready: false,
            production_mutation: false,
        }
    } else {
        ExpectedState {
            architecture_complete: false,
            production_core_gate: ProductionCoreGate::Blocked,
            production_ready: false,
            production_mutation: false,
        }
    }
}

fn validate_sequence(sequence: &[ProgramSlice]) -> Result<usize, LifecycleEvaluationError> {
    if sequence.len() < 2 {
        return Err(LifecycleEvaluationError::new(
            "INVALID_PROGRAM_SEQUENCE: at least two linear slices are required",
        ));
    }

    let mut ids = BTreeSet::new();
    let mut baseline_index = None;
    for (index, slice) in sequence.iter().enumerate() {
        if slice.id.is_empty() || !ids.insert(slice.id.as_str()) {
            return Err(LifecycleEvaluationError::new(format!(
                "INVALID_PROGRAM_SEQUENCE_ID: {}",
                slice.id
            )));
        }
        if slice.id == ACCEPTED_BASELINE {
            baseline_index = Some(index);
        }

        let expected_predecessor = index
            .checked_sub(1)
            .map(|previous| sequence[previous].id.as_str());
        let expected_successor = sequence.get(index + 1).map(|next| next.id.as_str());
        if slice.predecessor.as_deref() != expected_predecessor
            || slice.successor.as_deref() != expected_successor
        {
            return Err(LifecycleEvaluationError::new(format!(
                "NON_LINEAR_PROGRAM_SEQUENCE: {}",
                slice.id
            )));
        }
    }

    let baseline_index = baseline_index.ok_or_else(|| {
        LifecycleEvaluationError::new(format!(
            "MISSING_TYPED_ACCEPTANCE_BASELINE: {ACCEPTED_BASELINE}"
        ))
    })?;
    if sequence.get(baseline_index + 1).is_none() {
        return Err(LifecycleEvaluationError::new(
            "INVALID_PROGRAM_SEQUENCE: accepted baseline must have a successor",
        ));
    }
    if !ids.contains("AR-17") {
        return Err(LifecycleEvaluationError::new(
            "INVALID_PROGRAM_SEQUENCE: AR-17 closeout slice is required",
        ));
    }
    Ok(baseline_index)
}

fn validate_observation(
    observation: &AcceptanceObservationV1,
    expected_slice: &str,
) -> Result<(), LifecycleEvaluationError> {
    if observation.record_schema_version != ACCEPTANCE_RECORD_SCHEMA_VERSION {
        return Err(LifecycleEvaluationError::new(format!(
            "UNSUPPORTED_ACCEPTANCE_RECORD_VERSION: {}",
            observation.record_schema_version
        )));
    }
    if observation.slice != expected_slice {
        return Err(LifecycleEvaluationError::new(format!(
            "ACCEPTANCE_SLICE_MISMATCH: expected {expected_slice}, got {}",
            observation.slice
        )));
    }
    if observation.pr == 0 {
        return Err(LifecycleEvaluationError::new(
            "INVALID_ACCEPTANCE_RECORD: PR identity must be non-zero",
        ));
    }

    for (label, value) in [
        ("base_sha", observation.base_sha.as_str()),
        ("candidate_sha", observation.candidate_sha.as_str()),
        ("candidate_tree", observation.candidate_tree.as_str()),
        (
            "observed_candidate_tree",
            observation.observed_candidate_tree.as_str(),
        ),
        ("merge_sha", observation.merge_sha.as_str()),
        ("merge_tree", observation.merge_tree.as_str()),
        (
            "observed_merge_tree",
            observation.observed_merge_tree.as_str(),
        ),
        (
            "observed_merge_first_parent",
            observation.observed_merge_first_parent.as_str(),
        ),
        ("tag_target_sha", observation.tag_target_sha.as_str()),
        (
            "accepted_main_reread",
            observation.accepted_main_reread.as_str(),
        ),
    ] {
        validate_git_identity(value, label)?;
    }

    if observation.candidate_tree != observation.observed_candidate_tree {
        return Err(LifecycleEvaluationError::new(
            "CANDIDATE_TREE_IDENTITY_MISMATCH",
        ));
    }
    if observation.merge_tree != observation.observed_merge_tree {
        return Err(LifecycleEvaluationError::new(
            "MERGE_TREE_IDENTITY_MISMATCH",
        ));
    }
    if observation.candidate_tree != observation.merge_tree {
        return Err(LifecycleEvaluationError::new(
            "CANDIDATE_TREE_DIFFERS_FROM_ACCEPTED_TREE",
        ));
    }
    if observation.observed_merge_first_parent != observation.base_sha {
        return Err(LifecycleEvaluationError::new(
            "MERGE_FIRST_PARENT_DIFFERS_FROM_PREMERGE_BASE",
        ));
    }
    if observation.tag_target_sha != observation.merge_sha {
        return Err(LifecycleEvaluationError::new(
            "ACCEPTANCE_TAG_TARGET_MISMATCH",
        ));
    }
    if observation.accepted_main_reread != observation.merge_sha {
        return Err(LifecycleEvaluationError::new(
            "ACCEPTED_MAIN_REREAD_MISMATCH",
        ));
    }

    if observation.required_status_contexts_total == 0
        || observation.required_status_contexts_success
            != observation.required_status_contexts_total
    {
        return Err(LifecycleEvaluationError::new(
            "INCOMPLETE_REQUIRED_STATUS_CONTEXTS",
        ));
    }
    if observation.applicable_permanent_workflows_total == 0
        || observation.applicable_permanent_workflows_success
            != observation.applicable_permanent_workflows_total
    {
        return Err(LifecycleEvaluationError::new(
            "INCOMPLETE_PERMANENT_WORKFLOW_SET",
        ));
    }
    if observation.behind_by != 0 {
        return Err(LifecycleEvaluationError::new(
            "ACCEPTANCE_HEAD_BEHIND_MAIN",
        ));
    }
    if observation.blocking_reviews != 0 {
        return Err(LifecycleEvaluationError::new(
            "ACCEPTANCE_HAS_BLOCKING_REVIEWS",
        ));
    }
    if observation.unresolved_review_threads != 0 {
        return Err(LifecycleEvaluationError::new(
            "ACCEPTANCE_HAS_UNRESOLVED_THREADS",
        ));
    }

    let expected = expected_state(expected_slice);
    if observation.architecture_complete != expected.architecture_complete
        || observation.production_core_gate != expected.production_core_gate
        || observation.production_ready != expected.production_ready
        || observation.production_mutation != expected.production_mutation
    {
        return Err(LifecycleEvaluationError::new(format!(
            "INVALID_ACCEPTANCE_STATE: {expected_slice} violates fail-closed lifecycle policy"
        )));
    }
    Ok(())
}

fn validate_git_identity(value: &str, label: &str) -> Result<(), LifecycleEvaluationError> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(LifecycleEvaluationError::new(format!(
            "INVALID_GIT_IDENTITY: {label} must be exact lowercase 40-hex"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ACCEPTANCE_RECORD_SCHEMA_VERSION, AcceptanceObservationV1, DerivedLifecycleStateV1,
        LifecycleEvaluator, ProductionCoreGate, ProgramSlice, RawArchitectureAcceptanceEvidenceV1,
    };

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const D: &str = "dddddddddddddddddddddddddddddddddddddddd";

    fn sequence() -> Vec<ProgramSlice> {
        let ids = [
            "AR-10", "AR-11", "AR-12", "AR-13", "AR-14", "AR-15", "AR-16", "AR-17",
        ];
        ids.iter()
            .enumerate()
            .map(|(index, id)| ProgramSlice {
                id: (*id).to_owned(),
                predecessor: index.checked_sub(1).map(|previous| ids[previous].to_owned()),
                successor: ids.get(index + 1).map(|next| (*next).to_owned()),
            })
            .collect()
    }

    fn observation(slice: &str) -> AcceptanceObservationV1 {
        let gate = if slice == "AR-17" {
            ProductionCoreGate::Authorized
        } else {
            ProductionCoreGate::Blocked
        };
        AcceptanceObservationV1 {
            record_schema_version: ACCEPTANCE_RECORD_SCHEMA_VERSION,
            slice: slice.to_owned(),
            pr: 500,
            base_sha: A.to_owned(),
            candidate_sha: B.to_owned(),
            candidate_tree: C.to_owned(),
            observed_candidate_tree: C.to_owned(),
            merge_sha: D.to_owned(),
            merge_tree: C.to_owned(),
            observed_merge_tree: C.to_owned(),
            observed_merge_first_parent: A.to_owned(),
            tag_target_sha: D.to_owned(),
            required_status_contexts_total: 23,
            required_status_contexts_success: 23,
            applicable_permanent_workflows_total: 17,
            applicable_permanent_workflows_success: 17,
            behind_by: 0,
            blocking_reviews: 0,
            unresolved_review_threads: 0,
            accepted_main_reread: D.to_owned(),
            architecture_complete: slice == "AR-17",
            production_core_gate: gate,
            production_ready: false,
            production_mutation: false,
        }
    }

    fn evidence(observations: Vec<AcceptanceObservationV1>) -> RawArchitectureAcceptanceEvidenceV1 {
        RawArchitectureAcceptanceEvidenceV1 {
            schema_version: 1,
            source_branch: "main".to_owned(),
            sequence: sequence(),
            acceptance_observations: observations,
        }
    }

    #[test]
    fn baseline_without_new_acceptance_derives_ar12_current() {
        let state = LifecycleEvaluator::evaluate(&evidence(Vec::new())).expect("valid lifecycle");
        assert_eq!(
            state,
            DerivedLifecycleStateV1 {
                schema_version: 1,
                accepted_checkpoint: "AR-11".to_owned(),
                current_slice: Some("AR-12".to_owned()),
                architecture_complete: false,
                production_core_gate: ProductionCoreGate::Blocked,
                production_ready: false,
                production_mutation: false,
            }
        );
    }

    #[test]
    fn contiguous_acceptance_advances_one_slice() {
        let state = LifecycleEvaluator::evaluate(&evidence(vec![observation("AR-12")]))
            .expect("valid lifecycle");
        assert_eq!(state.accepted_checkpoint, "AR-12");
        assert_eq!(state.current_slice.as_deref(), Some("AR-13"));
    }

    #[test]
    fn gap_before_later_acceptance_fails_closed() {
        let error = LifecycleEvaluator::evaluate(&evidence(vec![observation("AR-13")]))
            .expect_err("gap must fail");
        assert!(error.to_string().contains("NON_CONTIGUOUS_ACCEPTANCE"));
    }

    #[test]
    fn duplicate_acceptance_fails_closed() {
        let ar12 = observation("AR-12");
        let error = LifecycleEvaluator::evaluate(&evidence(vec![ar12.clone(), ar12]))
            .expect_err("duplicate must fail");
        assert!(error.to_string().contains("DUPLICATE_ACCEPTANCE_OBSERVATION"));
    }

    #[test]
    fn incomplete_hosted_verification_fails_closed() {
        let mut ar12 = observation("AR-12");
        ar12.required_status_contexts_success -= 1;
        let error = LifecycleEvaluator::evaluate(&evidence(vec![ar12]))
            .expect_err("missing check must fail");
        assert!(error.to_string().contains("INCOMPLETE_REQUIRED_STATUS_CONTEXTS"));
    }

    #[test]
    fn wrong_candidate_tree_fails_closed() {
        let mut ar12 = observation("AR-12");
        ar12.observed_candidate_tree = B.to_owned();
        let error = LifecycleEvaluator::evaluate(&evidence(vec![ar12]))
            .expect_err("tree mismatch must fail");
        assert!(error.to_string().contains("CANDIDATE_TREE_IDENTITY_MISMATCH"));
    }

    #[test]
    fn premature_authorization_fails_closed() {
        let mut ar12 = observation("AR-12");
        ar12.production_core_gate = ProductionCoreGate::Authorized;
        let error = LifecycleEvaluator::evaluate(&evidence(vec![ar12]))
            .expect_err("premature authorization must fail");
        assert!(error.to_string().contains("INVALID_ACCEPTANCE_STATE"));
    }

    #[test]
    fn ar17_is_the_only_architecture_authorization_boundary() {
        let observations = ["AR-12", "AR-13", "AR-14", "AR-15", "AR-16", "AR-17"]
            .into_iter()
            .map(observation)
            .collect();
        let state = LifecycleEvaluator::evaluate(&evidence(observations)).expect("valid closeout");
        assert_eq!(state.accepted_checkpoint, "AR-17");
        assert_eq!(state.current_slice, None);
        assert!(state.architecture_complete);
        assert_eq!(state.production_core_gate, ProductionCoreGate::Authorized);
        assert!(!state.production_ready);
        assert!(!state.production_mutation);
    }
}
