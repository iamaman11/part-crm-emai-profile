ALTER TABLE resolver_encrypted_records
ADD COLUMN lookup_hmac_version INTEGER NOT NULL DEFAULT 1
CHECK (lookup_hmac_version > 0);

CREATE INDEX IF NOT EXISTS idx_resolver_records_lookup_hmac_version
ON resolver_encrypted_records(tenant_id, record_kind, lookup_hmac_version);

ALTER TABLE resolver_idempotency_records
ADD COLUMN hmac_version INTEGER NOT NULL DEFAULT 1
CHECK (hmac_version > 0);

CREATE INDEX IF NOT EXISTS idx_resolver_idempotency_hmac_version
ON resolver_idempotency_records(tenant_id, operation, hmac_version);
