-- Dedicated mailbox resolver state. This migration must never target the business/catalog D1.
PRAGMA foreign_keys = ON;

CREATE TABLE resolver_request_nonces (
    tenant_id TEXT NOT NULL,
    nonce TEXT NOT NULL,
    request_path TEXT NOT NULL,
    body_sha256 TEXT NOT NULL,
    authenticated_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, nonce),
    CHECK (length(tenant_id) BETWEEN 1 AND 128),
    CHECK (length(nonce) = 32),
    CHECK (length(body_sha256) = 64),
    CHECK (expires_at_ms > authenticated_at_ms)
) STRICT;

CREATE INDEX resolver_request_nonces_expiry
    ON resolver_request_nonces (expires_at_ms);

CREATE TABLE resolver_encrypted_records (
    tenant_id TEXT NOT NULL,
    lookup_digest TEXT NOT NULL,
    provider TEXT NOT NULL,
    record_kind TEXT NOT NULL,
    logical_id TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    nonce_hex TEXT NOT NULL,
    ciphertext_hex TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    consumed_at_ms INTEGER,
    discarded_at_ms INTEGER,
    PRIMARY KEY (tenant_id, lookup_digest, record_kind),
    CHECK (length(lookup_digest) = 64),
    CHECK (key_version > 0),
    CHECK (length(nonce_hex) = 24),
    CHECK (length(ciphertext_hex) BETWEEN 32 AND 65568),
    CHECK (updated_at_ms >= created_at_ms),
    CHECK (expires_at_ms IS NULL OR expires_at_ms > created_at_ms),
    CHECK (consumed_at_ms IS NULL OR consumed_at_ms >= created_at_ms),
    CHECK (discarded_at_ms IS NULL OR discarded_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX resolver_encrypted_records_expiry
    ON resolver_encrypted_records (record_kind, expires_at_ms)
    WHERE expires_at_ms IS NOT NULL;

CREATE TABLE resolver_idempotency_records (
    tenant_id TEXT NOT NULL,
    idempotency_digest TEXT NOT NULL,
    operation TEXT NOT NULL,
    request_sha256 TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_digest, operation),
    CHECK (length(idempotency_digest) = 64),
    CHECK (length(request_sha256) = 64)
) STRICT;

CREATE TABLE resolver_key_rotation_runs (
    rotation_id TEXT PRIMARY KEY,
    from_key_version INTEGER NOT NULL,
    to_key_version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('RUNNING', 'VERIFIED', 'FAILED')),
    scanned_records INTEGER NOT NULL DEFAULT 0,
    reencrypted_records INTEGER NOT NULL DEFAULT 0,
    started_at_ms INTEGER NOT NULL,
    verified_at_ms INTEGER,
    CHECK (from_key_version > 0),
    CHECK (to_key_version > 0),
    CHECK (from_key_version <> to_key_version),
    CHECK (scanned_records >= 0),
    CHECK (reencrypted_records >= 0),
    CHECK (reencrypted_records <= scanned_records),
    CHECK ((status = 'VERIFIED') = (verified_at_ms IS NOT NULL))
) STRICT;
