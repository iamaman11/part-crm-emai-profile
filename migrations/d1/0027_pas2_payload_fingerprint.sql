-- PAS-2 Transaction B: retire the browser-supplied request digest trust boundary.
--
-- Historical request_digest bytes are deliberately NOT reclassified or copied into
-- payload_fingerprint. Existing idempotency keys are retained as fail-closed tombstones
-- until their recorded expiry, while every new active row must carry a server-owned
-- SHA-256 fingerprint produced after typed command decoding.
ALTER TABLE idempotency_records
    RENAME TO idempotency_records_pas2_legacy;

CREATE TABLE idempotency_records (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_name TEXT NOT NULL CHECK(length(trim(command_name)) BETWEEN 1 AND 120),
    payload_fingerprint TEXT
        CHECK(
            payload_fingerprint IS NULL
            OR (
                length(payload_fingerprint) = 64
                AND payload_fingerprint NOT GLOB '*[^0-9a-f]*'
            )
        ),
    result_code TEXT NOT NULL CHECK(length(trim(result_code)) BETWEEN 1 AND 120),
    result_reference TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, actor_id, idempotency_key),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO idempotency_records (
    tenant_id,
    actor_id,
    idempotency_key,
    command_name,
    payload_fingerprint,
    result_code,
    result_reference,
    created_at_ms,
    expires_at_ms
)
SELECT
    tenant_id,
    actor_id,
    idempotency_key,
    command_name,
    NULL,
    result_code,
    result_reference,
    created_at_ms,
    expires_at_ms
FROM idempotency_records_pas2_legacy;

DROP TABLE idempotency_records_pas2_legacy;
