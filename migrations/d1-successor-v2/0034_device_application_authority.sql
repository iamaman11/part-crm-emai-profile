-- V2.1: durable browser-pairing, proof-challenge and device-bound application-session authority.
--
-- Ownership remains intentionally split across existing natural owners:
--   * memberships is the canonical user/tenant identity and enabled-state owner;
--   * device_actor_bindings is the canonical device trust-state owner;
--   * this migration stores only authorization epoch, one-shot pairing/challenge state and
--     digest-only short-lived application sessions.
-- Raw pairing secrets, raw application-session bearer tokens and private keys are never stored.
-- Pairing completion delegates the actual device binding write to the existing
-- device_public_key_binding_commands owner introduced by 0033.

CREATE TABLE device_user_authorization_state (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    auth_epoch INTEGER NOT NULL CHECK(auth_epoch >= 1),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    PRIMARY KEY (tenant_id, actor_id),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER device_user_authorization_state_insert_validate
BEFORE INSERT ON device_user_authorization_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_user_authorization_membership_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'device_user_authorization_initial_epoch_invalid')
    WHERE NEW.auth_epoch <> 1;
END;

CREATE TRIGGER device_user_authorization_state_epoch_validate
BEFORE UPDATE OF auth_epoch, updated_at_ms ON device_user_authorization_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_user_authorization_epoch_transition_invalid')
    WHERE NEW.auth_epoch <> OLD.auth_epoch + 1
       OR NEW.updated_at_ms < OLD.updated_at_ms;
END;

CREATE TRIGGER device_user_authorization_state_delete_forbidden
BEFORE DELETE ON device_user_authorization_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_user_authorization_delete_forbidden');
END;

CREATE TABLE device_pairing_transactions (
    tenant_id TEXT NOT NULL,
    pairing_digest TEXT NOT NULL
        CHECK(length(pairing_digest) = 64)
        CHECK(pairing_digest NOT GLOB '*[^0-9a-f]*'),
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    public_key_spki_der_hex TEXT NOT NULL
        CHECK(length(public_key_spki_der_hex) = 182)
        CHECK(public_key_spki_der_hex NOT GLOB '*[^0-9a-f]*'),
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL
        CHECK(expires_at_ms > issued_at_ms)
        CHECK(expires_at_ms - issued_at_ms <= 600000),
    authorized_actor_id TEXT,
    authorized_at_ms INTEGER,
    consumed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, pairing_digest),
    FOREIGN KEY (tenant_id, authorized_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    CHECK(
        (authorized_actor_id IS NULL AND authorized_at_ms IS NULL)
        OR
        (authorized_actor_id IS NOT NULL
         AND authorized_at_ms IS NOT NULL
         AND authorized_at_ms >= issued_at_ms
         AND authorized_at_ms < expires_at_ms)
    ),
    CHECK(
        consumed_at_ms IS NULL
        OR (
            authorized_actor_id IS NOT NULL
            AND consumed_at_ms >= authorized_at_ms
            AND consumed_at_ms < expires_at_ms
        )
    )
) STRICT;

-- Multiple pairing attempts may refer to the same native key. An expired, unconsumed attempt must
-- never permanently lock that non-exportable device key out of a fresh browser pairing. The
-- pairing_digest remains the transaction identity and every completion still requires a live,
-- browser-authorized transaction plus a fresh one-shot PoP challenge.
CREATE INDEX device_pairing_transactions_key_lookup
    ON device_pairing_transactions(public_key_spki_der_hex, expires_at_ms);

CREATE TRIGGER device_pairing_transactions_core_immutable
BEFORE UPDATE OF tenant_id, pairing_digest, device_id, public_key_spki_der_hex, issued_at_ms, expires_at_ms
ON device_pairing_transactions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_pairing_core_immutable');
END;

CREATE TRIGGER device_pairing_transactions_authorization_transition
BEFORE UPDATE OF authorized_actor_id, authorized_at_ms ON device_pairing_transactions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_pairing_authorization_transition_invalid')
    WHERE OLD.authorized_actor_id IS NOT NULL
       OR OLD.authorized_at_ms IS NOT NULL
       OR OLD.consumed_at_ms IS NOT NULL
       OR NEW.authorized_actor_id IS NULL
       OR NEW.authorized_at_ms IS NULL
       OR NEW.authorized_at_ms < NEW.issued_at_ms
       OR NEW.authorized_at_ms >= NEW.expires_at_ms;
END;

CREATE TRIGGER device_pairing_transactions_consumption_transition
BEFORE UPDATE OF consumed_at_ms ON device_pairing_transactions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_pairing_consumption_transition_invalid')
    WHERE OLD.consumed_at_ms IS NOT NULL
       OR NEW.consumed_at_ms IS NULL
       OR NEW.authorized_actor_id IS NULL
       OR NEW.authorized_at_ms IS NULL
       OR NEW.consumed_at_ms < NEW.authorized_at_ms
       OR NEW.consumed_at_ms >= NEW.expires_at_ms;
END;

CREATE TABLE device_pairing_authorization_commands (
    tenant_id TEXT NOT NULL,
    pairing_digest TEXT NOT NULL
        CHECK(length(pairing_digest) = 64)
        CHECK(pairing_digest NOT GLOB '*[^0-9a-f]*'),
    actor_id TEXT NOT NULL,
    authorized_at_ms INTEGER NOT NULL CHECK(authorized_at_ms >= 0),
    PRIMARY KEY (tenant_id, pairing_digest),
    FOREIGN KEY (tenant_id, pairing_digest)
        REFERENCES device_pairing_transactions(tenant_id, pairing_digest) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER device_pairing_authorization_command_validate
BEFORE INSERT ON device_pairing_authorization_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_pairing_actor_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'device_pairing_not_live')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_pairing_transactions
        WHERE tenant_id = NEW.tenant_id
          AND pairing_digest = NEW.pairing_digest
          AND authorized_actor_id IS NULL
          AND authorized_at_ms IS NULL
          AND consumed_at_ms IS NULL
          AND issued_at_ms <= NEW.authorized_at_ms
          AND expires_at_ms > NEW.authorized_at_ms
    );
END;

CREATE TRIGGER device_pairing_authorization_command_apply
AFTER INSERT ON device_pairing_authorization_commands
FOR EACH ROW
BEGIN
    INSERT INTO device_user_authorization_state (
        tenant_id, actor_id, auth_epoch, updated_at_ms
    )
    SELECT NEW.tenant_id, NEW.actor_id, 1, NEW.authorized_at_ms
    WHERE NOT EXISTS (
        SELECT 1 FROM device_user_authorization_state
        WHERE tenant_id = NEW.tenant_id AND actor_id = NEW.actor_id
    );

    UPDATE device_pairing_transactions
    SET authorized_actor_id = NEW.actor_id,
        authorized_at_ms = NEW.authorized_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND pairing_digest = NEW.pairing_digest;
END;

CREATE TABLE device_proof_challenges (
    tenant_id TEXT NOT NULL,
    challenge_digest TEXT NOT NULL
        CHECK(length(challenge_digest) = 64)
        CHECK(challenge_digest NOT GLOB '*[^0-9a-f]*'),
    purpose TEXT NOT NULL CHECK(purpose IN ('PAIRING', 'SESSION')),
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    pairing_digest TEXT,
    device_binding_version INTEGER CHECK(device_binding_version IS NULL OR device_binding_version >= 1),
    nonce_hex TEXT NOT NULL
        CHECK(length(nonce_hex) = 64)
        CHECK(nonce_hex NOT GLOB '*[^0-9a-f]*'),
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL
        CHECK(expires_at_ms > issued_at_ms)
        CHECK(expires_at_ms - issued_at_ms <= 120000),
    consumed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, challenge_digest),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, pairing_digest)
        REFERENCES device_pairing_transactions(tenant_id, pairing_digest) ON DELETE RESTRICT,
    CHECK(
        (purpose = 'PAIRING' AND pairing_digest IS NOT NULL AND device_binding_version IS NULL)
        OR
        (purpose = 'SESSION' AND pairing_digest IS NULL AND device_binding_version IS NOT NULL)
    ),
    CHECK(
        consumed_at_ms IS NULL
        OR (consumed_at_ms >= issued_at_ms AND consumed_at_ms < expires_at_ms)
    )
) STRICT;

CREATE TRIGGER device_proof_challenge_insert_validate
BEFORE INSERT ON device_proof_challenges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_proof_actor_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'device_proof_auth_state_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_user_authorization_state
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
    );
    SELECT RAISE(ABORT, 'device_proof_pairing_not_live')
    WHERE NEW.purpose = 'PAIRING'
      AND NOT EXISTS (
        SELECT 1 FROM device_pairing_transactions
        WHERE tenant_id = NEW.tenant_id
          AND pairing_digest = NEW.pairing_digest
          AND device_id = NEW.device_id
          AND authorized_actor_id = NEW.actor_id
          AND consumed_at_ms IS NULL
          AND expires_at_ms > NEW.issued_at_ms
    );
    SELECT RAISE(ABORT, 'device_proof_device_not_active')
    WHERE NEW.purpose = 'SESSION'
      AND NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND device_id = NEW.device_id
          AND version = NEW.device_binding_version
          AND status = 'ACTIVE'
          AND evidence_reference LIKE 'p256_spki_der:%'
    );
END;

CREATE TRIGGER device_proof_challenge_core_immutable
BEFORE UPDATE OF tenant_id, challenge_digest, purpose, actor_id, device_id, pairing_digest,
                 device_binding_version, nonce_hex, issued_at_ms, expires_at_ms
ON device_proof_challenges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_proof_challenge_core_immutable');
END;

CREATE TRIGGER device_proof_challenge_consumption_transition
BEFORE UPDATE OF consumed_at_ms ON device_proof_challenges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_proof_challenge_consumption_transition_invalid')
    WHERE OLD.consumed_at_ms IS NOT NULL
       OR NEW.consumed_at_ms IS NULL
       OR NEW.consumed_at_ms < NEW.issued_at_ms
       OR NEW.consumed_at_ms >= NEW.expires_at_ms;
END;

CREATE TABLE device_application_sessions (
    tenant_id TEXT NOT NULL,
    session_digest TEXT NOT NULL
        CHECK(length(session_digest) = 64)
        CHECK(session_digest NOT GLOB '*[^0-9a-f]*'),
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    user_auth_epoch INTEGER NOT NULL CHECK(user_auth_epoch >= 1),
    device_binding_version INTEGER NOT NULL CHECK(device_binding_version >= 1),
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL
        CHECK(expires_at_ms > issued_at_ms)
        CHECK(expires_at_ms - issued_at_ms <= 900000),
    revoked_at_ms INTEGER CHECK(revoked_at_ms IS NULL OR revoked_at_ms >= issued_at_ms),
    PRIMARY KEY (tenant_id, session_digest),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX device_application_sessions_actor_device
    ON device_application_sessions(tenant_id, actor_id, device_id, expires_at_ms)
    WHERE revoked_at_ms IS NULL;

CREATE TRIGGER device_application_session_insert_validate
BEFORE INSERT ON device_application_sessions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_application_session_actor_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'device_application_session_auth_epoch_stale')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_user_authorization_state
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND auth_epoch = NEW.user_auth_epoch
    );
    SELECT RAISE(ABORT, 'device_application_session_device_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND device_id = NEW.device_id
          AND version = NEW.device_binding_version
          AND status = 'ACTIVE'
          AND evidence_reference LIKE 'p256_spki_der:%'
    );
END;

CREATE TRIGGER device_application_session_core_immutable
BEFORE UPDATE OF tenant_id, session_digest, actor_id, device_id, user_auth_epoch,
                 device_binding_version, issued_at_ms, expires_at_ms
ON device_application_sessions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_application_session_core_immutable');
END;

CREATE TRIGGER device_application_session_revocation_transition
BEFORE UPDATE OF revoked_at_ms ON device_application_sessions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_application_session_revocation_transition_invalid')
    WHERE OLD.revoked_at_ms IS NOT NULL
       OR NEW.revoked_at_ms IS NULL
       OR NEW.revoked_at_ms < NEW.issued_at_ms;
END;

CREATE TABLE verified_device_pairing_completion_commands (
    tenant_id TEXT NOT NULL,
    pairing_digest TEXT NOT NULL
        CHECK(length(pairing_digest) = 64)
        CHECK(pairing_digest NOT GLOB '*[^0-9a-f]*'),
    challenge_digest TEXT NOT NULL
        CHECK(length(challenge_digest) = 64)
        CHECK(challenge_digest NOT GLOB '*[^0-9a-f]*'),
    session_digest TEXT NOT NULL
        CHECK(length(session_digest) = 64)
        CHECK(session_digest NOT GLOB '*[^0-9a-f]*'),
    expected_previous_version INTEGER CHECK(expected_previous_version IS NULL OR expected_previous_version >= 1),
    next_binding_version INTEGER NOT NULL CHECK(next_binding_version >= 1),
    user_auth_epoch INTEGER NOT NULL CHECK(user_auth_epoch >= 1),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    session_expires_at_ms INTEGER NOT NULL CHECK(session_expires_at_ms > completed_at_ms),
    PRIMARY KEY (tenant_id, pairing_digest),
    UNIQUE (tenant_id, challenge_digest),
    UNIQUE (tenant_id, session_digest),
    FOREIGN KEY (tenant_id, pairing_digest)
        REFERENCES device_pairing_transactions(tenant_id, pairing_digest) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, challenge_digest)
        REFERENCES device_proof_challenges(tenant_id, challenge_digest) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER verified_device_pairing_completion_validate
BEFORE INSERT ON verified_device_pairing_completion_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'verified_device_pairing_not_live')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_pairing_transactions AS pairing
        JOIN memberships AS membership
          ON membership.tenant_id = pairing.tenant_id
         AND membership.actor_id = pairing.authorized_actor_id
         AND membership.status = 'ACTIVE'
        JOIN device_user_authorization_state AS auth
          ON auth.tenant_id = pairing.tenant_id
         AND auth.actor_id = pairing.authorized_actor_id
         AND auth.auth_epoch = NEW.user_auth_epoch
        WHERE pairing.tenant_id = NEW.tenant_id
          AND pairing.pairing_digest = NEW.pairing_digest
          AND pairing.authorized_actor_id IS NOT NULL
          AND pairing.consumed_at_ms IS NULL
          AND pairing.expires_at_ms > NEW.completed_at_ms
    );
    SELECT RAISE(ABORT, 'verified_device_pairing_challenge_not_live')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_proof_challenges AS challenge
        JOIN device_pairing_transactions AS pairing
          ON pairing.tenant_id = challenge.tenant_id
         AND pairing.pairing_digest = challenge.pairing_digest
        WHERE challenge.tenant_id = NEW.tenant_id
          AND challenge.challenge_digest = NEW.challenge_digest
          AND challenge.purpose = 'PAIRING'
          AND challenge.actor_id = pairing.authorized_actor_id
          AND challenge.device_id = pairing.device_id
          AND challenge.consumed_at_ms IS NULL
          AND challenge.expires_at_ms > NEW.completed_at_ms
          AND pairing.pairing_digest = NEW.pairing_digest
    );
    SELECT RAISE(ABORT, 'verified_device_pairing_session_ttl_invalid')
    WHERE NEW.session_expires_at_ms - NEW.completed_at_ms > 900000;
END;

CREATE TRIGGER verified_device_pairing_completion_apply
AFTER INSERT ON verified_device_pairing_completion_commands
FOR EACH ROW
BEGIN
    INSERT INTO device_public_key_binding_commands (
        tenant_id, pairing_digest, actor_id, device_id, public_key_spki_der_hex,
        expected_previous_version, next_version, executed_at_ms
    )
    SELECT
        pairing.tenant_id,
        pairing.pairing_digest,
        pairing.authorized_actor_id,
        pairing.device_id,
        pairing.public_key_spki_der_hex,
        NEW.expected_previous_version,
        NEW.next_binding_version,
        NEW.completed_at_ms
    FROM device_pairing_transactions AS pairing
    WHERE pairing.tenant_id = NEW.tenant_id
      AND pairing.pairing_digest = NEW.pairing_digest;

    UPDATE device_pairing_transactions
    SET consumed_at_ms = NEW.completed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND pairing_digest = NEW.pairing_digest;

    UPDATE device_proof_challenges
    SET consumed_at_ms = NEW.completed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND challenge_digest = NEW.challenge_digest;

    INSERT INTO device_application_sessions (
        tenant_id, session_digest, actor_id, device_id, user_auth_epoch,
        device_binding_version, issued_at_ms, expires_at_ms, revoked_at_ms
    )
    SELECT
        pairing.tenant_id,
        NEW.session_digest,
        pairing.authorized_actor_id,
        pairing.device_id,
        NEW.user_auth_epoch,
        NEW.next_binding_version,
        NEW.completed_at_ms,
        NEW.session_expires_at_ms,
        NULL
    FROM device_pairing_transactions AS pairing
    WHERE pairing.tenant_id = NEW.tenant_id
      AND pairing.pairing_digest = NEW.pairing_digest;
END;

CREATE TABLE verified_device_session_renewal_commands (
    tenant_id TEXT NOT NULL,
    challenge_digest TEXT NOT NULL
        CHECK(length(challenge_digest) = 64)
        CHECK(challenge_digest NOT GLOB '*[^0-9a-f]*'),
    session_digest TEXT NOT NULL
        CHECK(length(session_digest) = 64)
        CHECK(session_digest NOT GLOB '*[^0-9a-f]*'),
    user_auth_epoch INTEGER NOT NULL CHECK(user_auth_epoch >= 1),
    device_binding_version INTEGER NOT NULL CHECK(device_binding_version >= 1),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    session_expires_at_ms INTEGER NOT NULL CHECK(session_expires_at_ms > completed_at_ms),
    PRIMARY KEY (tenant_id, challenge_digest),
    UNIQUE (tenant_id, session_digest),
    FOREIGN KEY (tenant_id, challenge_digest)
        REFERENCES device_proof_challenges(tenant_id, challenge_digest) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER verified_device_session_renewal_validate
BEFORE INSERT ON verified_device_session_renewal_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'verified_device_session_challenge_not_live')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_proof_challenges AS challenge
        JOIN memberships AS membership
          ON membership.tenant_id = challenge.tenant_id
         AND membership.actor_id = challenge.actor_id
         AND membership.status = 'ACTIVE'
        JOIN device_user_authorization_state AS auth
          ON auth.tenant_id = challenge.tenant_id
         AND auth.actor_id = challenge.actor_id
         AND auth.auth_epoch = NEW.user_auth_epoch
        JOIN device_actor_bindings AS binding
          ON binding.tenant_id = challenge.tenant_id
         AND binding.actor_id = challenge.actor_id
         AND binding.device_id = challenge.device_id
         AND binding.version = NEW.device_binding_version
         AND binding.version = challenge.device_binding_version
         AND binding.status = 'ACTIVE'
         AND binding.evidence_reference LIKE 'p256_spki_der:%'
        WHERE challenge.tenant_id = NEW.tenant_id
          AND challenge.challenge_digest = NEW.challenge_digest
          AND challenge.purpose = 'SESSION'
          AND challenge.consumed_at_ms IS NULL
          AND challenge.expires_at_ms > NEW.completed_at_ms
    );
    SELECT RAISE(ABORT, 'verified_device_session_ttl_invalid')
    WHERE NEW.session_expires_at_ms - NEW.completed_at_ms > 900000;
END;

CREATE TRIGGER verified_device_session_renewal_apply
AFTER INSERT ON verified_device_session_renewal_commands
FOR EACH ROW
BEGIN
    UPDATE device_proof_challenges
    SET consumed_at_ms = NEW.completed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND challenge_digest = NEW.challenge_digest;

    INSERT INTO device_application_sessions (
        tenant_id, session_digest, actor_id, device_id, user_auth_epoch,
        device_binding_version, issued_at_ms, expires_at_ms, revoked_at_ms
    )
    SELECT
        challenge.tenant_id,
        NEW.session_digest,
        challenge.actor_id,
        challenge.device_id,
        NEW.user_auth_epoch,
        NEW.device_binding_version,
        NEW.completed_at_ms,
        NEW.session_expires_at_ms,
        NULL
    FROM device_proof_challenges AS challenge
    WHERE challenge.tenant_id = NEW.tenant_id
      AND challenge.challenge_digest = NEW.challenge_digest;
END;

-- User disable/revoke invalidates every application session and every active device for that user.
-- The membership table remains the enabled-state owner; auth_epoch is only the invalidation fence.
CREATE TRIGGER membership_device_application_authority_revoke
AFTER UPDATE OF status ON memberships
FOR EACH ROW
WHEN OLD.status = 'ACTIVE' AND NEW.status <> 'ACTIVE'
BEGIN
    UPDATE device_user_authorization_state
    SET auth_epoch = auth_epoch + 1,
        updated_at_ms = CASE
            WHEN NEW.updated_at_ms < updated_at_ms THEN updated_at_ms
            ELSE NEW.updated_at_ms
        END
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id;

    UPDATE device_actor_bindings
    SET status = 'REVOKED',
        updated_at_ms = CASE
            WHEN NEW.updated_at_ms < bound_at_ms THEN bound_at_ms
            ELSE NEW.updated_at_ms
        END,
        revoked_at_ms = CASE
            WHEN NEW.updated_at_ms < bound_at_ms THEN bound_at_ms
            ELSE NEW.updated_at_ms
        END
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND status = 'ACTIVE';

    UPDATE device_application_sessions
    SET revoked_at_ms = CASE
        WHEN NEW.updated_at_ms < issued_at_ms THEN issued_at_ms
        ELSE NEW.updated_at_ms
    END
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND revoked_at_ms IS NULL;
END;

-- Per-device revoke immediately invalidates sessions bound to exactly that immutable binding version.
CREATE TRIGGER device_binding_revocation_revokes_application_sessions
AFTER UPDATE OF status ON device_actor_bindings
FOR EACH ROW
WHEN OLD.status = 'ACTIVE' AND NEW.status = 'REVOKED'
BEGIN
    UPDATE device_application_sessions
    SET revoked_at_ms = CASE
        WHEN NEW.revoked_at_ms < issued_at_ms THEN issued_at_ms
        ELSE NEW.revoked_at_ms
    END
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND device_id = NEW.device_id
      AND device_binding_version = NEW.version
      AND revoked_at_ms IS NULL;
END;
