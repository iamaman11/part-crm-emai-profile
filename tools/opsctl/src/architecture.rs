use crate::canonical::{canonical_pretty_json, parse_strict_json};
use opsctl_core::architecture::{
    AcceptanceObservationV1, DerivedLifecycleStateV1, LifecycleEvaluationError, LifecycleEvaluator,
    ProductionCoreGate, ProgramSlice, RawArchitectureAcceptanceEvidenceV1, ValidatedProgramSequence,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

const PROGRAM_SEQUENCE_KIND: &str = "ARCHITECTURE_PROGRAM_SEQUENCE";
const PROGRAM_SEQUENCE_STATE_MODEL: &str = "STATIC_ORDER_ONLY";
const PROGRAM_ISSUE: u64 = 266;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchitectureAdapterError {
    message: String,
}

impl ArchitectureAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ArchitectureAdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ArchitectureAdapterError {}

impl From<LifecycleEvaluationError> for ArchitectureAdapterError {
    fn from(error: LifecycleEvaluationError) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramSequenceDto {
    schema_version: u64,
    kind: String,
    program: String,
    program_issue: u64,
    state_model: String,
    mutable_lifecycle_state_forbidden: bool,
    slices: Vec<ProgramSliceDto>,
    non_linear_preserved_decisions: Vec<NonLinearDecisionDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramSliceDto {
    id: String,
    name: String,
    predecessor: Option<String>,
    successor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NonLinearDecisionDto {
    id: String,
    name: String,
    state: String,
    reopen_only_by_later_accepted_evidence: bool,
    not_in_linear_acceptance_chain: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAcceptanceEvidenceDto {
    schema_version: u64,
    source_branch: String,
    acceptance_observations: Vec<AcceptanceObservationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceObservationDto {
    record_schema_version: u64,
    slice: String,
    pr: u64,
    base_sha: String,
    candidate_sha: String,
    candidate_tree: String,
    observed_candidate_tree: String,
    merge_sha: String,
    merge_tree: String,
    observed_merge_tree: String,
    observed_merge_first_parent: String,
    tag_target_sha: String,
    required_status_contexts_total: u64,
    required_status_contexts_success: u64,
    applicable_permanent_workflows_total: u64,
    applicable_permanent_workflows_success: u64,
    behind_by: u64,
    blocking_reviews: u64,
    unresolved_review_threads: u64,
    accepted_main_reread: String,
    architecture_complete: bool,
    production_core_gate: ProductionCoreGateDto,
    production_ready: bool,
    production_mutation: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProductionCoreGateDto {
    Blocked,
    Authorized,
}

impl From<ProductionCoreGateDto> for ProductionCoreGate {
    fn from(value: ProductionCoreGateDto) -> Self {
        match value {
            ProductionCoreGateDto::Blocked => Self::Blocked,
            ProductionCoreGateDto::Authorized => Self::Authorized,
        }
    }
}

impl From<ProductionCoreGate> for ProductionCoreGateDto {
    fn from(value: ProductionCoreGate) -> Self {
        match value {
            ProductionCoreGate::Blocked => Self::Blocked,
            ProductionCoreGate::Authorized => Self::Authorized,
        }
    }
}

#[derive(Debug, Serialize)]
struct DerivedLifecycleStateDto {
    schema_version: u64,
    kind: &'static str,
    accepted_checkpoint: String,
    current_slice: Option<String>,
    architecture_complete: bool,
    production_core_gate: ProductionCoreGateDto,
    production_ready: bool,
    production_mutation: bool,
}

pub(crate) fn evaluate_lifecycle_json(
    sequence_json: &str,
    evidence_json: &str,
) -> Result<String, ArchitectureAdapterError> {
    let sequence = decode_program_sequence(sequence_json)?;
    let evidence = decode_acceptance_evidence(evidence_json)?;
    let state = LifecycleEvaluator::evaluate(&sequence, &evidence)?;
    render_lifecycle_state(state)
}

fn decode_program_sequence(
    input: &str,
) -> Result<ValidatedProgramSequence, ArchitectureAdapterError> {
    let value = parse_strict_json(input)
        .map_err(|error| ArchitectureAdapterError::new(format!("PROGRAM_SEQUENCE_JSON: {error}")))?;
    let dto: ProgramSequenceDto = serde_json::from_value(value).map_err(|error| {
        ArchitectureAdapterError::new(format!("PROGRAM_SEQUENCE_SCHEMA: {error}"))
    })?;
    if dto.schema_version != 1
        || dto.kind != PROGRAM_SEQUENCE_KIND
        || dto.program.trim().is_empty()
        || dto.program_issue != PROGRAM_ISSUE
        || dto.state_model != PROGRAM_SEQUENCE_STATE_MODEL
        || !dto.mutable_lifecycle_state_forbidden
    {
        return Err(ArchitectureAdapterError::new(
            "PROGRAM_SEQUENCE_CONTRACT: static program sequence identity drifted",
        ));
    }
    validate_non_linear_decisions(&dto.non_linear_preserved_decisions)?;
    let slices = dto
        .slices
        .into_iter()
        .map(|slice| {
            if slice.name.trim().is_empty() {
                return Err(ArchitectureAdapterError::new(format!(
                    "PROGRAM_SEQUENCE_CONTRACT: {} has an empty name",
                    slice.id
                )));
            }
            Ok(ProgramSlice {
                id: slice.id,
                predecessor: slice.predecessor,
                successor: slice.successor,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    ValidatedProgramSequence::new(slices).map_err(Into::into)
}

fn validate_non_linear_decisions(
    decisions: &[NonLinearDecisionDto],
) -> Result<(), ArchitectureAdapterError> {
    if decisions.len() != 1 {
        return Err(ArchitectureAdapterError::new(
            "PROGRAM_SEQUENCE_CONTRACT: exactly one preserved non-linear AR-4D decision is required",
        ));
    }
    let decision = &decisions[0];
    if decision.id != "AR-4D"
        || decision.name.trim().is_empty()
        || decision.state != "NOT_REQUIRED"
        || !decision.reopen_only_by_later_accepted_evidence
        || !decision.not_in_linear_acceptance_chain
    {
        return Err(ArchitectureAdapterError::new(
            "PROGRAM_SEQUENCE_CONTRACT: AR-4D preserved decision drifted",
        ));
    }
    Ok(())
}

fn decode_acceptance_evidence(
    input: &str,
) -> Result<RawArchitectureAcceptanceEvidenceV1, ArchitectureAdapterError> {
    let value = parse_strict_json(input).map_err(|error| {
        ArchitectureAdapterError::new(format!("ACCEPTANCE_EVIDENCE_JSON: {error}"))
    })?;
    let dto: RawAcceptanceEvidenceDto = serde_json::from_value(value).map_err(|error| {
        ArchitectureAdapterError::new(format!("ACCEPTANCE_EVIDENCE_SCHEMA: {error}"))
    })?;
    Ok(RawArchitectureAcceptanceEvidenceV1 {
        schema_version: dto.schema_version,
        source_branch: dto.source_branch,
        acceptance_observations: dto
            .acceptance_observations
            .into_iter()
            .map(|observation| AcceptanceObservationV1 {
                record_schema_version: observation.record_schema_version,
                slice: observation.slice,
                pr: observation.pr,
                base_sha: observation.base_sha,
                candidate_sha: observation.candidate_sha,
                candidate_tree: observation.candidate_tree,
                observed_candidate_tree: observation.observed_candidate_tree,
                merge_sha: observation.merge_sha,
                merge_tree: observation.merge_tree,
                observed_merge_tree: observation.observed_merge_tree,
                observed_merge_first_parent: observation.observed_merge_first_parent,
                tag_target_sha: observation.tag_target_sha,
                required_status_contexts_total: observation.required_status_contexts_total,
                required_status_contexts_success: observation.required_status_contexts_success,
                applicable_permanent_workflows_total: observation
                    .applicable_permanent_workflows_total,
                applicable_permanent_workflows_success: observation
                    .applicable_permanent_workflows_success,
                behind_by: observation.behind_by,
                blocking_reviews: observation.blocking_reviews,
                unresolved_review_threads: observation.unresolved_review_threads,
                accepted_main_reread: observation.accepted_main_reread,
                architecture_complete: observation.architecture_complete,
                production_core_gate: observation.production_core_gate.into(),
                production_ready: observation.production_ready,
                production_mutation: observation.production_mutation,
            })
            .collect(),
    })
}

fn render_lifecycle_state(
    state: DerivedLifecycleStateV1,
) -> Result<String, ArchitectureAdapterError> {
    let dto = DerivedLifecycleStateDto {
        schema_version: state.schema_version,
        kind: "DERIVED_LIFECYCLE_STATE",
        accepted_checkpoint: state.accepted_checkpoint,
        current_slice: state.current_slice,
        architecture_complete: state.architecture_complete,
        production_core_gate: state.production_core_gate.into(),
        production_ready: state.production_ready,
        production_mutation: state.production_mutation,
    };
    let value = serde_json::to_value(dto).map_err(|error| {
        ArchitectureAdapterError::new(format!("LIFECYCLE_RENDER_SCHEMA: {error}"))
    })?;
    canonical_pretty_json(&value)
        .map_err(|error| ArchitectureAdapterError::new(format!("LIFECYCLE_RENDER: {error}")))
}

#[cfg(test)]
mod tests {
    use super::evaluate_lifecycle_json;
    use serde_json::{Value, json};

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const D: &str = "dddddddddddddddddddddddddddddddddddddddd";

    fn sequence_json() -> String {
        json!({
            "schema_version": 1,
            "kind": "ARCHITECTURE_PROGRAM_SEQUENCE",
            "program": "Architecture Re-baseline v3",
            "program_issue": 266,
            "state_model": "STATIC_ORDER_ONLY",
            "mutable_lifecycle_state_forbidden": true,
            "slices": [
                {"id":"AR-10","name":"Runtime","predecessor":null,"successor":"AR-11"},
                {"id":"AR-11","name":"Release","predecessor":"AR-10","successor":"AR-12"},
                {"id":"AR-12","name":"Rehearsal","predecessor":"AR-11","successor":"AR-13"},
                {"id":"AR-13","name":"Rotation","predecessor":"AR-12","successor":"AR-14"},
                {"id":"AR-14","name":"Recovery","predecessor":"AR-13","successor":"AR-15"},
                {"id":"AR-15","name":"Windows","predecessor":"AR-14","successor":"AR-16"},
                {"id":"AR-16","name":"Audit","predecessor":"AR-15","successor":"AR-17"},
                {"id":"AR-17","name":"Closeout","predecessor":"AR-16","successor":null}
            ],
            "non_linear_preserved_decisions": [{
                "id":"AR-4D",
                "name":"Profile extraction",
                "state":"NOT_REQUIRED",
                "reopen_only_by_later_accepted_evidence":true,
                "not_in_linear_acceptance_chain":true
            }]
        })
        .to_string()
    }

    fn evidence_json(observations: Value) -> String {
        json!({
            "schema_version": 1,
            "source_branch": "main",
            "acceptance_observations": observations
        })
        .to_string()
    }

    fn observation(slice: &str) -> Value {
        json!({
            "record_schema_version": 1,
            "slice": slice,
            "pr": 500,
            "base_sha": A,
            "candidate_sha": B,
            "candidate_tree": C,
            "observed_candidate_tree": C,
            "merge_sha": D,
            "merge_tree": C,
            "observed_merge_tree": C,
            "observed_merge_first_parent": A,
            "tag_target_sha": D,
            "required_status_contexts_total": 23,
            "required_status_contexts_success": 23,
            "applicable_permanent_workflows_total": 17,
            "applicable_permanent_workflows_success": 17,
            "behind_by": 0,
            "blocking_reviews": 0,
            "unresolved_review_threads": 0,
            "accepted_main_reread": D,
            "architecture_complete": false,
            "production_core_gate": "BLOCKED",
            "production_ready": false,
            "production_mutation": false
        })
    }

    #[test]
    fn strict_adapter_derives_current_state() -> Result<(), Box<dyn std::error::Error>> {
        let output = evaluate_lifecycle_json(&sequence_json(), &evidence_json(json!([])))?;
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["kind"], "DERIVED_LIFECYCLE_STATE");
        assert_eq!(value["accepted_checkpoint"], "AR-11");
        assert_eq!(value["current_slice"], "AR-12");
        assert_eq!(value["production_core_gate"], "BLOCKED");
        Ok(())
    }

    #[test]
    fn strict_adapter_rejects_unknown_fields() {
        let evidence = r#"{"schema_version":1,"source_branch":"main","acceptance_observations":[],"authority_bag":{}}"#;
        assert!(evaluate_lifecycle_json(&sequence_json(), evidence).is_err());
    }

    #[test]
    fn adapter_keeps_policy_in_pure_core() {
        let mut ar12 = observation("AR-12");
        ar12["production_core_gate"] = Value::String("AUTHORIZED".to_owned());
        assert!(evaluate_lifecycle_json(&sequence_json(), &evidence_json(json!([ar12]))).is_err());
    }
}
