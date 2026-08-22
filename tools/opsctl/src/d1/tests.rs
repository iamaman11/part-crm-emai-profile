use super::{
    ComponentAuthority, D1Action, D1Error, Decision, LedgerState, MigrationClass,
    MigrationContract, Preconditions, ReleaseSchemaContract, RolloutOrder,
};
use crate::d1::plan::evaluate;
use crate::d1::status::classify_prefix;
use std::collections::HashSet;

fn historical_authority() -> ComponentAuthority {
    ComponentAuthority {
        component_id: "catalog".to_owned(),
        historical_len: 3,
        ordered_history: vec![
            "0001_a.sql".to_owned(),
            "0002_b.sql".to_owned(),
            "0003_c.sql".to_owned(),
        ],
        post_epoch: Vec::new(),
        current_repository_revision: "0003_c.sql".to_owned(),
        history_digest: "history-digest".to_owned(),
        policy_digest: "policy-digest".to_owned(),
    }
}

fn transition_authority(contracts: Vec<MigrationContract>) -> ComponentAuthority {
    let mut ordered_history = vec!["0001_base.sql".to_owned()];
    ordered_history.extend(
        contracts
            .iter()
            .map(|contract| contract.migration_file.clone()),
    );
    let current_repository_revision = ordered_history
        .last()
        .cloned()
        .unwrap_or_else(|| "0001_base.sql".to_owned());
    ComponentAuthority {
        component_id: "resolver".to_owned(),
        historical_len: 1,
        ordered_history,
        post_epoch: contracts,
        current_repository_revision,
        history_digest: "transition-history".to_owned(),
        policy_digest: "policy-digest".to_owned(),
    }
}

fn migration(
    name: &str,
    migration_class: MigrationClass,
    rollout_order: RolloutOrder,
    fail_forward_required: bool,
    contract_preconditions: &[&str],
) -> MigrationContract {
    MigrationContract {
        migration_file: name.to_owned(),
        migration_class,
        rollout_order,
        fail_forward_required,
        destructive: migration_class == MigrationClass::Contract,
        code_rollback_allowed: !fail_forward_required
            && migration_class != MigrationClass::Contract,
        contract_preconditions: contract_preconditions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    }
}

fn contract(
    component: &str,
    target: &str,
    minimum: &str,
    maximum: &str,
    history_digest: &str,
) -> ReleaseSchemaContract {
    ReleaseSchemaContract {
        database_component: component.to_owned(),
        target_schema_revision: target.to_owned(),
        supported_schema_min: minimum.to_owned(),
        supported_schema_max: maximum.to_owned(),
        migration_history_digest: history_digest.to_owned(),
        compatibility_policy_digest: "policy-digest".to_owned(),
    }
}

#[test]
fn classifies_exact_prefix_and_noncanonical_ledgers() {
    let canonical = historical_authority().ordered_history;
    assert_eq!(classify_prefix(&canonical, &canonical), LedgerState::Exact);
    assert_eq!(
        classify_prefix(&canonical[..2], &canonical),
        LedgerState::BehindKnownPrefix
    );
    assert_eq!(
        classify_prefix(
            &["0001_a.sql".to_owned(), "0003_c.sql".to_owned()],
            &canonical
        ),
        LedgerState::Diverged
    );
    assert_eq!(
        classify_prefix(
            &["0001_a.sql".to_owned(), "9999_unknown.sql".to_owned()],
            &canonical
        ),
        LedgerState::UnknownMigration
    );
    assert_eq!(
        classify_prefix(
            &["0001_a.sql".to_owned(), "0001_a.sql".to_owned()],
            &canonical
        ),
        LedgerState::CorruptLedger
    );
}

#[test]
fn known_ahead_schema_distinguishes_rollback_window() -> Result<(), D1Error> {
    let authority = historical_authority();
    let remote = authority.ordered_history.clone();
    let compatible = evaluate(
        D1Action::Compatibility,
        &authority,
        &remote,
        Some(&contract(
            "catalog",
            "0002_b.sql",
            "0001_a.sql",
            "0003_c.sql",
            "history-digest",
        )),
        None,
        None,
        &Preconditions::default(),
    )?;
    assert_eq!(compatible.ledger_state, LedgerState::AheadKnownCompatible);
    assert_eq!(compatible.decision, Decision::CodeRollbackSafe);
    assert!(compatible.allowed);

    let incompatible = evaluate(
        D1Action::Compatibility,
        &authority,
        &remote,
        Some(&contract(
            "catalog",
            "0002_b.sql",
            "0001_a.sql",
            "0002_b.sql",
            "history-digest",
        )),
        None,
        None,
        &Preconditions::default(),
    )?;
    assert_eq!(
        incompatible.ledger_state,
        LedgerState::AheadKnownIncompatible
    );
    assert_eq!(incompatible.decision, Decision::CodeRollbackBlocked);
    assert!(!incompatible.allowed);
    Ok(())
}

#[test]
fn historical_plan_is_visible_but_fail_closed() -> Result<(), D1Error> {
    let authority = historical_authority();
    let target = contract(
        "catalog",
        "0003_c.sql",
        "0001_a.sql",
        "0003_c.sql",
        "history-digest",
    );
    let result = evaluate(
        D1Action::Plan,
        &authority,
        &["0001_a.sql".to_owned()],
        Some(&target),
        Some(&target),
        Some(&target),
        &Preconditions::default(),
    )?;
    assert_eq!(result.ledger_state, LedgerState::BehindKnownPrefix);
    assert_eq!(result.decision, Decision::MigrationRequired);
    assert!(!result.allowed);
    assert_eq!(result.planned_migrations, vec!["0002_b.sql", "0003_c.sql"]);
    assert_eq!(
        result.reason_codes,
        vec!["HISTORICAL_COMPATIBILITY_UNKNOWN"]
    );
    Ok(())
}

#[test]
fn release_contract_policy_digest_mismatch_fails_closed() {
    let authority = historical_authority();
    let mut target = contract(
        "catalog",
        "0003_c.sql",
        "0001_a.sql",
        "0003_c.sql",
        "history-digest",
    );
    target.compatibility_policy_digest = "other-policy".to_owned();
    assert!(
        evaluate(
            D1Action::Compatibility,
            &authority,
            &authority.ordered_history,
            Some(&target),
            None,
            None,
            &Preconditions::default(),
        )
        .is_err()
    );
}

#[test]
fn expand_backfill_and_rollout_order_remain_typed() -> Result<(), D1Error> {
    for (class, rollout, expected, allowed) in [
        (
            MigrationClass::Expand,
            RolloutOrder::MigrateBeforeCode,
            Decision::MigrateFirst,
            true,
        ),
        (
            MigrationClass::Backfill,
            RolloutOrder::CodeBeforeMigrate,
            Decision::DeployFirst,
            false,
        ),
        (
            MigrationClass::Expand,
            RolloutOrder::Either,
            Decision::MigrationRequired,
            true,
        ),
    ] {
        let authority = transition_authority(vec![migration(
            "0002_transition.sql",
            class,
            rollout,
            false,
            &[],
        )]);
        let target = contract(
            "resolver",
            "0002_transition.sql",
            "0001_base.sql",
            "0002_transition.sql",
            "transition-history",
        );
        let current = contract(
            "resolver",
            "0001_base.sql",
            "0001_base.sql",
            "0002_transition.sql",
            "transition-history",
        );
        let result = evaluate(
            D1Action::Plan,
            &authority,
            &["0001_base.sql".to_owned()],
            Some(&target),
            Some(&current),
            Some(&current),
            &Preconditions::default(),
        )?;
        assert_eq!(result.decision, expected);
        assert_eq!(result.allowed, allowed);
        assert_eq!(result.planned_contracts[0].migration_class, class);
    }
    Ok(())
}

#[test]
fn contract_requires_typed_preconditions() -> Result<(), D1Error> {
    let authority = transition_authority(vec![migration(
        "0002_contract.sql",
        MigrationClass::Contract,
        RolloutOrder::SeparateContractRelease,
        false,
        &["replacement_active", "old_readers_writers_retired"],
    )]);
    let target = contract(
        "resolver",
        "0002_contract.sql",
        "0001_base.sql",
        "0002_contract.sql",
        "transition-history",
    );
    let current = contract(
        "resolver",
        "0001_base.sql",
        "0001_base.sql",
        "0002_contract.sql",
        "transition-history",
    );
    let blocked = evaluate(
        D1Action::Plan,
        &authority,
        &["0001_base.sql".to_owned()],
        Some(&target),
        Some(&current),
        Some(&current),
        &Preconditions::default(),
    )?;
    assert_eq!(blocked.decision, Decision::ContractBlocked);
    assert!(!blocked.allowed);

    let complete = Preconditions {
        completed: HashSet::from([
            "replacement_active".to_owned(),
            "old_readers_writers_retired".to_owned(),
        ]),
    };
    let accepted = evaluate(
        D1Action::Plan,
        &authority,
        &["0001_base.sql".to_owned()],
        Some(&target),
        Some(&current),
        Some(&current),
        &complete,
    )?;
    assert_eq!(accepted.decision, Decision::Safe);
    assert!(accepted.allowed);
    Ok(())
}

#[test]
fn repair_fail_forward_is_explicitly_blocked() -> Result<(), D1Error> {
    let authority = transition_authority(vec![migration(
        "0002_repair.sql",
        MigrationClass::Repair,
        RolloutOrder::SeparateContractRelease,
        true,
        &[],
    )]);
    let target = contract(
        "resolver",
        "0002_repair.sql",
        "0002_repair.sql",
        "0002_repair.sql",
        "transition-history",
    );
    let current = contract(
        "resolver",
        "0001_base.sql",
        "0001_base.sql",
        "0002_repair.sql",
        "transition-history",
    );
    let result = evaluate(
        D1Action::Plan,
        &authority,
        &["0001_base.sql".to_owned()],
        Some(&target),
        Some(&current),
        Some(&current),
        &Preconditions::default(),
    )?;
    assert_eq!(result.decision, Decision::FailForwardRequired);
    assert!(!result.allowed);
    assert_eq!(
        result.reason_codes,
        vec!["EXPLICIT_FAIL_FORWARD_TRANSITION"]
    );
    Ok(())
}

#[test]
fn verify_requires_exact_target() -> Result<(), D1Error> {
    let authority = historical_authority();
    let target = contract(
        "catalog",
        "0002_b.sql",
        "0001_a.sql",
        "0003_c.sql",
        "history-digest",
    );
    let result = evaluate(
        D1Action::Verify,
        &authority,
        &authority.ordered_history,
        Some(&target),
        None,
        None,
        &Preconditions::default(),
    )?;
    assert_eq!(result.ledger_state, LedgerState::AheadKnownCompatible);
    assert_eq!(result.decision, Decision::RecoveryRequired);
    assert!(!result.allowed);
    Ok(())
}
