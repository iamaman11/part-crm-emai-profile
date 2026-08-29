-- CAP-EXEC P2 / CAP12-I2: bounded, single-use Browser Profile launch authority.
--
-- This table is not a second Profile/session aggregate. It stores only short-lived authority
-- evidence after the canonical profile/device/generation admission has succeeded. The raw
-- bearer claim code is never persisted; only its SHA-256 digest is durable.

CREATE TABLE profile_launch_claims (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    claim_digest TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    audit_event_id TEXT NOT NULL,
    issued_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    redeemed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, actor_id, idempotency_key),
    UNIQUE (claim_digest),
    CHECK (length(payload_fingerprint) = 64),
    CHECK (payload_fingerprint NOT GLOB '*[^0-9a-f]*'),
    CHECK (length(claim_digest) = 64),
    CHECK (claim_digest NOT GLOB '*[^0-9a-f]*'),
    CHECK (issued_at_ms >= 0),
    CHECK (expires_at_ms > issued_at_ms),
    CHECK (redeemed_at_ms IS NULL OR (redeemed_at_ms >= issued_at_ms AND redeemed_at_ms < expires_at_ms)),
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id)
);

CREATE INDEX profile_launch_claim_digest_lookup
    ON profile_launch_claims(claim_digest);

CREATE INDEX profile_launch_claim_expiry_lookup
    ON profile_launch_claims(expires_at_ms, redeemed_at_ms);

-- Critical launch authorization is always durable-audited from the same successful INSERT.
-- Replay of the same idempotency key does not insert another row and therefore cannot duplicate
-- the audit record or launch authority.
CREATE TRIGGER profile_launch_claim_audit
AFTER INSERT ON profile_launch_claims
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
        'profile.launch.authorized',
        'browser_profile',
        NEW.profile_id,
        'authorized',
        NEW.issued_at_ms
    );
END;
