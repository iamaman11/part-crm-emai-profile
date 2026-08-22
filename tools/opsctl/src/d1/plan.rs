use super::compatibility::{
    runtime_supports_index, runtime_supports_remote, validate_release_contract,
};
use super::model::{
    ComponentAuthority, D1Action, D1Error, Decision, Evaluation, LedgerState, MigrationClass,
    MigrationContract, PlannedMigrationContract, Preconditions, ReleaseSchemaContract,
    RolloutOrder,
};
use super::status::{classify_prefix, classify_relative_state, revision_index};
use super::verify::evaluate_verify;

pub(super) fn evaluate(
    action: D1Action,
    authority: &ComponentAuthority,
    remote_names: &[String],
    target: Option<&ReleaseSchemaContract>,
    current: Option<&ReleaseSchemaContract>,
    known_good: Option<&ReleaseSchemaContract>,
    preconditions: &Preconditions,
) -> Result<Evaluation, D1Error> {
    let base_state = classify_prefix(remote_names, &authority.ordered_history);
    if matches!(
        base_state,
        LedgerState::Diverged | LedgerState::UnknownMigration | LedgerState::CorruptLedger
    ) {
        return Ok(blocked_evaluation(
            base_state,
            Decision::RecoveryRequired,
            remote_names,
            target
                .map(|value| value.target_schema_revision.clone())
                .unwrap_or_else(|| authority.current_repository_revision.clone()),
            "LEDGER_NOT_CANONICAL",
        ));
    }

    let Some(target) = target else {
        let state = if remote_names.len() == authority.ordered_history.len() {
            LedgerState::Exact
        } else {
            LedgerState::BehindKnownPrefix
        };
        return Ok(Evaluation {
            ledger_state: state,
            decision: if state == LedgerState::Exact {
                Decision::Safe
            } else {
                Decision::MigrationRequired
            },
            remote_revision: remote_names.last().cloned(),
            target_revision: authority.current_repository_revision.clone(),
            planned_migrations: if state == LedgerState::BehindKnownPrefix {
                authority.ordered_history[remote_names.len()..].to_vec()
            } else {
                Vec::new()
            },
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    };

    validate_release_contract(authority, target)?;
    if let Some(value) = current {
        validate_release_contract(authority, value)?;
    }
    if let Some(value) = known_good {
        validate_release_contract(authority, value)?;
    }

    let target_index = revision_index(&authority.ordered_history, &target.target_schema_revision)?;
    let minimum = revision_index(&authority.ordered_history, &target.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &target.supported_schema_max)?;
    if minimum > target_index || target_index > maximum {
        return Err(D1Error::new(
            "release schema window must satisfy supported_min <= target <= supported_max",
        ));
    }

    let remote_count = remote_names.len();
    let target_count = target_index + 1;
    let state = classify_relative_state(remote_count, target_count, minimum, maximum);

    if action == D1Action::Verify {
        return Ok(evaluate_verify(state, remote_names, target));
    }

    if state == LedgerState::AheadKnownCompatible {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackSafe,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations: Vec::new(),
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    }
    if state == LedgerState::AheadKnownIncompatible {
        return Ok(blocked_evaluation(
            state,
            Decision::CodeRollbackBlocked,
            remote_names,
            target.target_schema_revision.clone(),
            "RUNTIME_SCHEMA_WINDOW_EXCEEDED",
        ));
    }
    if state == LedgerState::Exact {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::Safe,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations: Vec::new(),
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: current.is_some() && known_good.is_some(),
            allowed: true,
        });
    }

    let planned_migrations = authority.ordered_history[remote_count..target_count].to_vec();
    let planned_contracts = planned_contract_values(authority, remote_count, target_count);
    if action == D1Action::Compatibility {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::MigrationRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    }

    if remote_count < authority.historical_len {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::MigrationRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["HISTORICAL_COMPATIBILITY_UNKNOWN".to_owned()],
            rollback_context_complete: current.is_some() && known_good.is_some(),
            allowed: false,
        });
    }

    let Some(current) = current else {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["CURRENT_RUNTIME_CONTEXT_MISSING".to_owned()],
            rollback_context_complete: false,
            allowed: false,
        });
    };
    let Some(known_good) = known_good else {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["KNOWN_GOOD_RUNTIME_CONTEXT_MISSING".to_owned()],
            rollback_context_complete: false,
            allowed: false,
        });
    };

    if !runtime_supports_remote(authority, current, remote_count)? {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::RecoveryRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["CURRENT_RUNTIME_ALREADY_SCHEMA_INCOMPATIBLE".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let contracts = post_epoch_slice(authority, remote_count, target_count)?;
    if let Some(missing) = missing_contract_precondition(&contracts, preconditions) {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::ContractBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec![format!("CONTRACT_PRECONDITION_MISSING:{missing}")],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let rollback_supported = runtime_supports_index(authority, known_good, target_index)?;
    let fail_forward = contracts
        .iter()
        .any(|contract| contract.fail_forward_required);
    if !rollback_supported {
        return Ok(Evaluation {
            ledger_state: state,
            decision: if fail_forward {
                Decision::FailForwardRequired
            } else {
                Decision::CodeRollbackBlocked
            },
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["KNOWN_GOOD_INCOMPATIBLE_AFTER_MIGRATION".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }
    if fail_forward {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::FailForwardRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["EXPLICIT_FAIL_FORWARD_TRANSITION".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let decision = aggregate_rollout_decision(&contracts)?;
    let allowed = !matches!(decision, Decision::DeployFirst);
    Ok(Evaluation {
        ledger_state: state,
        decision,
        remote_revision: remote_names.last().cloned(),
        target_revision: target.target_schema_revision.clone(),
        planned_migrations,
        planned_contracts,
        reason_codes: Vec::new(),
        rollback_context_complete: true,
        allowed,
    })
}

fn post_epoch_slice(
    authority: &ComponentAuthority,
    remote_count: usize,
    target_count: usize,
) -> Result<Vec<&MigrationContract>, D1Error> {
    if remote_count < authority.historical_len {
        return Err(D1Error::new(
            "post-epoch contract slice cannot include frozen historical migrations",
        ));
    }
    let start = remote_count - authority.historical_len;
    let end = target_count - authority.historical_len;
    authority
        .post_epoch
        .get(start..end)
        .map(|slice| slice.iter().collect())
        .ok_or_else(|| D1Error::new("post-epoch migration contract slice is invalid"))
}

fn missing_contract_precondition(
    contracts: &[&MigrationContract],
    evidence: &Preconditions,
) -> Option<String> {
    contracts
        .iter()
        .filter(|contract| contract.migration_class == MigrationClass::Contract)
        .flat_map(|contract| contract.contract_preconditions.iter())
        .find(|required| !evidence.completed.contains(required.as_str()))
        .cloned()
}

fn aggregate_rollout_decision(contracts: &[&MigrationContract]) -> Result<Decision, D1Error> {
    let mut migrate_first = false;
    let mut deploy_first = false;
    let mut separate_contract = false;
    for contract in contracts {
        match contract.rollout_order {
            RolloutOrder::MigrateBeforeCode => migrate_first = true,
            RolloutOrder::CodeBeforeMigrate => deploy_first = true,
            RolloutOrder::Either => {}
            RolloutOrder::SeparateContractRelease => separate_contract = true,
        }
    }
    if migrate_first && deploy_first {
        return Err(D1Error::new(
            "planned migration set contains contradictory rollout orders",
        ));
    }
    if separate_contract {
        return Ok(Decision::Safe);
    }
    if migrate_first {
        return Ok(Decision::MigrateFirst);
    }
    if deploy_first {
        return Ok(Decision::DeployFirst);
    }
    Ok(Decision::MigrationRequired)
}

fn planned_contract_values(
    authority: &ComponentAuthority,
    remote_count: usize,
    target_count: usize,
) -> Vec<PlannedMigrationContract> {
    if remote_count < authority.historical_len {
        return Vec::new();
    }
    let start = remote_count - authority.historical_len;
    let end = target_count.saturating_sub(authority.historical_len);
    authority
        .post_epoch
        .get(start..end)
        .map(|contracts| {
            contracts
                .iter()
                .map(|contract| PlannedMigrationContract {
                    migration_file: contract.migration_file.clone(),
                    migration_class: contract.migration_class,
                    rollout_order: contract.rollout_order,
                    fail_forward_required: contract.fail_forward_required,
                    destructive: contract.destructive,
                    code_rollback_allowed: contract.code_rollback_allowed,
                    contract_preconditions: contract.contract_preconditions.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn blocked_evaluation(
    state: LedgerState,
    decision: Decision,
    remote_names: &[String],
    target_revision: String,
    reason: &str,
) -> Evaluation {
    Evaluation {
        ledger_state: state,
        decision,
        remote_revision: remote_names.last().cloned(),
        target_revision,
        planned_migrations: Vec::new(),
        planned_contracts: Vec::new(),
        reason_codes: vec![reason.to_owned()],
        rollback_context_complete: false,
        allowed: false,
    }
}
