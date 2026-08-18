ALTER TABLE resolver_encrypted_records
ADD COLUMN refresh_owner_hmac_version INTEGER
CHECK (refresh_owner_hmac_version IS NULL OR refresh_owner_hmac_version > 0);

-- Any lease that existed before lookup-HMAC versioning was necessarily signed by
-- the legacy v1 handle-HMAC key. Preserve that non-secret dependency metadata so
-- retirement checks can fail closed until the lease expires or is cleared.
UPDATE resolver_encrypted_records
SET refresh_owner_hmac_version = 1
WHERE refresh_owner_digest IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_resolver_records_refresh_owner_hmac_version
ON resolver_encrypted_records(refresh_owner_hmac_version, refresh_expires_at_ms)
WHERE refresh_owner_hmac_version IS NOT NULL;
