use super::model::{D1Error, LedgerState};
use super::util::ensure_unique;
use std::collections::HashSet;

pub(super) fn classify_prefix(remote: &[String], canonical: &[String]) -> LedgerState {
    if ensure_unique(remote, "remote migration ledger").is_err() {
        return LedgerState::CorruptLedger;
    }
    let known: HashSet<&str> = canonical.iter().map(String::as_str).collect();
    if remote.iter().any(|name| !known.contains(name.as_str())) {
        return LedgerState::UnknownMigration;
    }
    if remote.len() > canonical.len() {
        return LedgerState::UnknownMigration;
    }
    if remote
        .iter()
        .zip(canonical.iter())
        .any(|(observed, expected)| observed != expected)
    {
        return LedgerState::Diverged;
    }
    if remote.len() == canonical.len() {
        LedgerState::Exact
    } else {
        LedgerState::BehindKnownPrefix
    }
}

pub(super) fn classify_relative_state(
    remote_count: usize,
    target_count: usize,
    minimum: usize,
    maximum: usize,
) -> LedgerState {
    if remote_count == target_count {
        return LedgerState::Exact;
    }
    if remote_count < target_count {
        return LedgerState::BehindKnownPrefix;
    }
    let remote_index = remote_count - 1;
    if remote_index >= minimum && remote_index <= maximum {
        LedgerState::AheadKnownCompatible
    } else {
        LedgerState::AheadKnownIncompatible
    }
}

pub(super) fn revision_index(history: &[String], revision: &str) -> Result<usize, D1Error> {
    history
        .iter()
        .position(|candidate| candidate == revision)
        .ok_or_else(|| {
            D1Error::new(format!(
                "unknown schema revision in release contract: {revision}"
            ))
        })
}
