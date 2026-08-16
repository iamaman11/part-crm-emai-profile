-- AR-8: durable OAuth refresh single-flight/fencing state.
-- This migration is additive and targets the dedicated resolver D1 only.
-- No production mutation is performed by committing this file.
PRAGMA foreign_keys = ON;

ALTER TABLE resolver_encrypted_records
    ADD COLUMN mutation_generation INTEGER NOT NULL DEFAULT 1
    CHECK (mutation_generation > 0);

ALTER TABLE resolver_encrypted_records
    ADD COLUMN credential_state TEXT NOT NULL DEFAULT 'ACTIVE'
    CHECK (credential_state IN ('ACTIVE', 'REAUTH_REQUIRED'));

ALTER TABLE resolver_encrypted_records
    ADD COLUMN refresh_owner_digest TEXT
    CHECK (refresh_owner_digest IS NULL OR length(refresh_owner_digest) = 64);

ALTER TABLE resolver_encrypted_records
    ADD COLUMN refresh_started_at_ms INTEGER;

ALTER TABLE resolver_encrypted_records
    ADD COLUMN refresh_expires_at_ms INTEGER;

CREATE INDEX resolver_encrypted_records_refresh_lease
    ON resolver_encrypted_records (refresh_expires_at_ms)
    WHERE refresh_owner_digest IS NOT NULL;

CREATE INDEX resolver_encrypted_records_credential_state
    ON resolver_encrypted_records (credential_state, tenant_id, record_kind)
    WHERE credential_state <> 'ACTIVE';
