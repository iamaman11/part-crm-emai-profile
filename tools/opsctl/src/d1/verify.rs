use super::model::{Decision, Evaluation, LedgerState, ReleaseSchemaContract};

pub(super) fn evaluate_verify(
    state: LedgerState,
    remote_names: &[String],
    target: &ReleaseSchemaContract,
) -> Evaluation {
    if state == LedgerState::Exact {
        return Evaluation {
            ledger_state: state,
            decision: Decision::Safe,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations: Vec::new(),
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        };
    }
    Evaluation {
        ledger_state: state,
        decision: Decision::RecoveryRequired,
        remote_revision: remote_names.last().cloned(),
        target_revision: target.target_schema_revision.clone(),
        planned_migrations: Vec::new(),
        planned_contracts: Vec::new(),
        reason_codes: vec!["POST_APPLY_TARGET_MISMATCH".to_owned()],
        rollback_context_complete: false,
        allowed: false,
    }
}
