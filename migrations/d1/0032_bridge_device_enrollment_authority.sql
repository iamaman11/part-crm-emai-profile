-- V2.3 / B7: bounded one-shot Bridge device enrollment authority.
--
-- This table is not a device registry, certificate store or PKI. It stores only short-lived
-- enrollment authorization state. Raw bearer claim values, CSR bytes, certificate bytes and
-- private/signing key material are never persisted here.

CREATE TABLE bridge_device_enrollment_claims (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    claim_digest TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    correlation_id TEXT NOT NULL,
    audit_event_id TEXT NOT NULL,
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > issued_at_ms),
    reserved_csr_sha256 TEXT,
    reserved_at_ms INTEGER,
    certificate_sha256 TEXT,
    consumed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, actor_id, idempotency_key),
    UNIQUE (claim_digest),
    CHECK (length(payload_fingerprint) = 64),
    CHECK (payload_fingerprint NOT GLOB '*[^0-9a-f]*'),
    CHECK (length(claim_digest) = 64),
    CHECK (claim_digest NOT GLOB '*[^0-9a-f]*'),
    CHECK (reserved_csr_sha256 IS NULL OR (
        length(reserved_csr_sha256) = 64
        AND reserved_csr_sha256 NOT GLOB '*[^0-9a-f]*'
    )),
    CHECK (certificate_sha256 IS NULL OR (
        length(certificate_sha256) = 64
        AND certificate_sha256 NOT GLOB '*[^0-9a-f]*'
    )),
    CHECK (
        (reserved_csr_sha256 IS NULL AND reserved_at_ms IS NULL)
        OR
        (reserved_csr_sha256 IS NOT NULL
            AND reserved_at_ms IS NOT NULL
            AND reserved_at_ms >= issued_at_ms
            AND reserved_at_ms < expires_at_ms)
    ),
    CHECK (
        (certificate_sha256 IS NULL AND consumed_at_ms IS NULL)
        OR
        (certificate_sha256 IS NOT NULL
            AND consumed_at_ms IS NOT NULL
            AND reserved_csr_sha256 IS NOT NULL
            AND reserved_at_ms IS NOT NULL
            AND consumed_at_ms >= reserved_at_ms)
    ),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX bridge_device_enrollment_claim_digest_lookup
    ON bridge_device_enrollment_claims(claim_digest);

CREATE INDEX bridge_device_enrollment_claim_expiry_lookup
    ON bridge_device_enrollment_claims(expires_at_ms, consumed_at_ms);

-- Exactly one CSR may reserve a claim. Replaying the same CSR is allowed by the adapter as a read
-- of the existing reservation; SQL may only perform the first NULL -> exact digest transition.
CREATE TRIGGER bridge_device_enrollment_reservation_immutable
BEFORE UPDATE OF reserved_csr_sha256, reserved_at_ms ON bridge_device_enrollment_claims
FOR EACH ROW
WHEN OLD.reserved_csr_sha256 IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'bridge enrollment CSR reservation is immutable');
END;

-- Certificate identity is final. Exact replay is served by reading the completed row; a second
-- UPDATE is forbidden regardless of whether the caller proposes the same or a different digest.
CREATE TRIGGER bridge_device_enrollment_completion_immutable
BEFORE UPDATE OF certificate_sha256, consumed_at_ms ON bridge_device_enrollment_claims
FOR EACH ROW
WHEN OLD.certificate_sha256 IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'bridge enrollment completion is immutable');
END;

CREATE TRIGGER bridge_device_enrollment_issue_audit
AFTER INSERT ON bridge_device_enrollment_claims
FOR EACH ROW
BEGIN
    INSERT INTO audit_events (
        tenant_id,
        audit_event_id,
        correlation_id,
        actor_id,
        action,
        resource_type,
        resource_id,
        result_code,
        occurred_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.audit_event_id,
        NEW.correlation_id,
        NEW.actor_id,
        'bridge.device.enrollment.authorized',
        'device',
        NEW.device_id,
        'authorized',
        NEW.issued_at_ms
    );
END;
