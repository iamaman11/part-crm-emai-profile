-- CAP-EXEC P2 / CAP12-I2: bounded one-time Profile Bridge launch authority.
--
-- This table stores only the digest of the opaque claim carried in the custom URI.
-- The raw claim, Access assertions, device private keys, browser profile bytes and
-- runtime secrets must never be persisted here.

CREATE TABLE profile_launch_claims (
    claim_digest TEXT PRIMARY KEY
        CHECK(length(claim_digest) = 64)
        CHECK(claim_digest NOT GLOB '*[^0-9a-f]*'),
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    payload_fingerprint TEXT NOT NULL
        CHECK(length(payload_fingerprint) = 64)
        CHECK(payload_fingerprint NOT GLOB '*[^0-9a-f]*'),
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL,
    redeemed_at_ms INTEGER,
    CHECK(expires_at_ms > issued_at_ms),
    CHECK(expires_at_ms - issued_at_ms <= 120000),
    CHECK(
        redeemed_at_ms IS NULL
        OR (redeemed_at_ms >= issued_at_ms AND redeemed_at_ms < expires_at_ms)
    ),
    UNIQUE (tenant_id, actor_id, idempotency_key),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, device_id, profile_id, generation_id)
        REFERENCES device_authorizations(
            tenant_id, device_id, profile_id, generation_id
        ) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(
            tenant_id, profile_id, generation_id
        ) ON DELETE RESTRICT
) STRICT;

CREATE INDEX profile_launch_claim_active_lookup
    ON profile_launch_claims(claim_digest, expires_at_ms, redeemed_at_ms);

CREATE INDEX profile_launch_claim_actor_history
    ON profile_launch_claims(tenant_id, actor_id, issued_at_ms, claim_digest);

-- The application use case remains the semantic authorization owner. These guards
-- repeat the security-critical facts at the write boundary so a grant/device/profile
-- race between admission and claim persistence fails closed instead of minting stale
-- authority.
CREATE TRIGGER profile_launch_claim_validate_insert
BEFORE INSERT ON profile_launch_claims
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_launch_claim_authority_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships AS membership
        WHERE membership.tenant_id = NEW.tenant_id
          AND membership.actor_id = NEW.actor_id
          AND membership.status = 'ACTIVE'
          AND (
              membership.role = 'TENANT_OWNER'
              OR EXISTS (
                  SELECT 1
                  FROM profile_grants AS grant
                  WHERE grant.tenant_id = NEW.tenant_id
                    AND grant.actor_id = NEW.actor_id
                    AND grant.profile_id = NEW.profile_id
                    AND grant.role = 'PROFILE_OPERATOR'
              )
          )
    );

    SELECT RAISE(ABORT, 'profile_launch_claim_authority_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_actor_bindings AS binding
        WHERE binding.tenant_id = NEW.tenant_id
          AND binding.actor_id = NEW.actor_id
          AND binding.device_id = NEW.device_id
          AND binding.status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_launch_claim_authority_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_authorizations AS authorization
        WHERE authorization.tenant_id = NEW.tenant_id
          AND authorization.device_id = NEW.device_id
          AND authorization.profile_id = NEW.profile_id
          AND authorization.generation_id = NEW.generation_id
          AND authorization.status = 'ACTIVE'
          AND authorization.version >= 1
    );

    SELECT RAISE(ABORT, 'profile_launch_claim_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_profiles AS profile
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'READY'
          AND profile.active_generation_id = NEW.generation_id
    );

    SELECT RAISE(ABORT, 'profile_launch_claim_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM profile_generations AS generation
        WHERE generation.tenant_id = NEW.tenant_id
          AND generation.profile_id = NEW.profile_id
          AND generation.generation_id = NEW.generation_id
          AND generation.status = 'VERIFIED'
          AND generation.verification_reference IS NOT NULL
    );
END;

-- Every binding field is immutable after issuance. The only permitted mutation is
-- the single compare-and-set transition of redeemed_at_ms from NULL to a valid time.
CREATE TRIGGER profile_launch_claim_binding_immutable
BEFORE UPDATE OF
    claim_digest,
    tenant_id,
    actor_id,
    idempotency_key,
    payload_fingerprint,
    device_id,
    profile_id,
    generation_id,
    issued_at_ms,
    expires_at_ms
ON profile_launch_claims
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_launch_claim_binding_immutable');
END;

CREATE TRIGGER profile_launch_claim_redeem_once
BEFORE UPDATE OF redeemed_at_ms ON profile_launch_claims
FOR EACH ROW
WHEN OLD.redeemed_at_ms IS NOT NULL
   OR NEW.redeemed_at_ms IS NULL
   OR NEW.redeemed_at_ms < OLD.issued_at_ms
   OR NEW.redeemed_at_ms >= OLD.expires_at_ms
BEGIN
    SELECT RAISE(ABORT, 'profile_launch_claim_replay_rejected');
END;

CREATE TRIGGER profile_launch_claim_delete_forbidden
BEFORE DELETE ON profile_launch_claims
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_launch_claim_history_immutable');
END;
