use super::{
    ComponentAuthority, D1Action, D1Error, LedgerState, Preconditions, ReleaseSchemaContract,
};
use crate::d1::plan::evaluate;
use crate::d1::status::classify_prefix;

fn authority() -> ComponentAuthority {
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
    }
}

fn contract(maximum: &str) -> ReleaseSchemaContract {
    ReleaseSchemaContract {
        database_component: "catalog".to_owned(),
        target_schema_revision: "0002_b.sql".to_owned(),
        supported_schema_min: "0001_a.sql".to_owned(),
        supported_schema_max: maximum.to_owned(),
        migration_history_digest: "history-digest".to_owned(),
        compatibility_policy_digest: "policy-digest".to_owned(),
    }
}

#[test]
fn classifies_exact_prefix_and_divergence() {
    let canonical = authority().ordered_history;
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
    let remote = authority().ordered_history;
    let compatible = evaluate(
        D1Action::Compatibility,
        &authority(),
        &remote,
        Some(&contract("0003_c.sql")),
        None,
        None,
        &Preconditions::default(),
    )?;
    assert_eq!(compatible.ledger_state, LedgerState::AheadKnownCompatible);
    assert!(compatible.allowed);

    let incompatible = evaluate(
        D1Action::Compatibility,
        &authority(),
        &remote,
        Some(&contract("0002_b.sql")),
        None,
        None,
        &Preconditions::default(),
    )?;
    assert_eq!(
        incompatible.ledger_state,
        LedgerState::AheadKnownIncompatible
    );
    assert!(!incompatible.allowed);
    Ok(())
}

#[test]
fn historical_plan_is_visible_but_not_automatically_allowed() -> Result<(), D1Error> {
    let remote = vec!["0001_a.sql".to_owned()];
    let result = evaluate(
        D1Action::Plan,
        &authority(),
        &remote,
        Some(&contract("0003_c.sql")),
        Some(&contract("0003_c.sql")),
        Some(&contract("0003_c.sql")),
        &Preconditions::default(),
    )?;
    assert_eq!(result.ledger_state, LedgerState::BehindKnownPrefix);
    assert!(!result.allowed);
    assert_eq!(result.planned_migrations, vec!["0002_b.sql"]);
    assert_eq!(
        result.reason_codes,
        vec!["HISTORICAL_COMPATIBILITY_UNKNOWN"]
    );
    Ok(())
}
