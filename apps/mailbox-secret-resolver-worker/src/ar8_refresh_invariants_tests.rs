const STORAGE: &str = include_str!("storage.rs");
const OPERATIONS: &str = include_str!("operations.rs");
const MIGRATION: &str =
    include_str!("../../../migrations/resolver-d1/0002_oauth_refresh_fencing.sql");

fn normalized(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn function_body<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("start marker must exist");
    let tail = &source[start..];
    let end = tail.find(end).expect("end marker must exist");
    &tail[..end]
}

#[test]
fn migration_persists_generation_lifecycle_and_bounded_lease_state() {
    let migration = normalized(MIGRATION);
    assert!(migration.contains(
        "ADD COLUMN mutation_generation INTEGER NOT NULL DEFAULT 1 CHECK (mutation_generation > 0)"
    ));
    assert!(migration.contains(
        "ADD COLUMN credential_state TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (credential_state IN ('ACTIVE', 'REAUTH_REQUIRED'))"
    ));
    assert!(migration.contains(
        "ADD COLUMN refresh_owner_digest TEXT CHECK (refresh_owner_digest IS NULL OR length(refresh_owner_digest) = 64)"
    ));
    assert!(migration.contains("ADD COLUMN refresh_started_at_ms INTEGER"));
    assert!(migration.contains("ADD COLUMN refresh_expires_at_ms INTEGER"));
}

#[test]
fn lease_acquire_is_generation_scoped_single_flight_and_crash_recoverable() {
    let storage = normalized(STORAGE);
    assert!(storage.contains(
        "mutation_generation = ? AND credential_state = 'ACTIVE' AND discarded_at_ms IS NULL AND (refresh_owner_digest IS NULL OR refresh_expires_at_ms <= ?) RETURNING mutation_generation, refresh_owner_digest, refresh_expires_at_ms"
    ));
    assert!(storage.contains(
        "if row.refresh_owner_digest.is_some() && row.refresh_expires_at_ms.is_some_and(|value| value > now) { return Err(RefreshAcquireError::Busy); }"
    ));
}

#[test]
fn refresh_commit_requires_generation_owner_live_lease_and_no_key_downgrade() {
    let storage = normalized(STORAGE);
    assert!(storage.contains(
        "mutation_generation = ? AND credential_state = 'ACTIVE' AND refresh_owner_digest = ? AND refresh_expires_at_ms > ? AND discarded_at_ms IS NULL AND key_version <= ?"
    ));
    assert!(storage.contains(
        "mutation_generation = ?, refresh_owner_digest = NULL, refresh_started_at_ms = NULL, refresh_expires_at_ms = NULL"
    ));
}

#[test]
fn reauth_transition_is_fenced_by_the_same_live_lease() {
    let storage = normalized(STORAGE);
    assert!(storage.contains(
        "SET credential_state = 'REAUTH_REQUIRED', mutation_generation = ?, refresh_owner_digest = NULL, refresh_started_at_ms = NULL, refresh_expires_at_ms = NULL, updated_at_ms = ?"
    ));
    assert!(storage.contains(
        "mutation_generation = ? AND credential_state = 'ACTIVE' AND refresh_owner_digest = ? AND refresh_expires_at_ms > ? AND discarded_at_ms IS NULL"
    ));
}

#[test]
fn refresh_execution_has_one_provider_call_site_and_no_unconditional_store() {
    assert_eq!(OPERATIONS.matches("refresh_access_token(").count(), 1);
    let coordinator = function_body(
        OPERATIONS,
        "async fn refresh_credential(",
        "async fn release_then_error(",
    );
    assert!(!coordinator.contains("store_credential("));
    assert!(!coordinator.contains("store_credential_kind("));
    assert!(!coordinator.contains(".store("));
    assert!(coordinator.contains("acquire_refresh_lease"));
    assert!(coordinator.contains("commit_refresh"));
    assert!(coordinator.contains("mark_reauth_required"));
    assert!(coordinator.contains("release_refresh_lease"));
}

#[test]
fn public_refresh_response_does_not_expose_refresh_or_fencing_material() {
    let explicit_refresh = function_body(
        OPERATIONS,
        "async fn refresh_graph(",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\nenum RefreshMode",
    );
    assert!(explicit_refresh.contains("\"access_token\""));
    assert!(!explicit_refresh.contains("\"refresh_token\""));
    assert!(!explicit_refresh.contains("owner_digest"));
    assert!(!explicit_refresh.contains("mutation_generation"));
}
