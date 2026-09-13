-- V2.1: canonical public-key device binding persistence for browser-authorized pairing.
--
-- device_actor_bindings remains the single authoritative device trust-state owner. This migration
-- adds a self-binding command for the first-release P-256 application key; it does not create a
-- second device registry. The exact canonical SPKI is public credential material, never a private
-- key. Historical mtls_cert_sha256 rows/commands remain readable only for migration/recovery and
-- are not used by the successor pairing path.

CREATE UNIQUE INDEX device_actor_bindings_one_active_p256_key
    ON device_actor_bindings(evidence_reference)
    WHERE status = 'ACTIVE' AND evidence_reference LIKE 'p256_spki_der:%';

CREATE TABLE device_public_key_binding_commands (
    tenant_id TEXT NOT NULL,
    pairing_digest TEXT NOT NULL
        CHECK(length(pairing_digest) = 64)
        CHECK(pairing_digest NOT GLOB '*[^0-9a-f]*'),
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    public_key_spki_der_hex TEXT NOT NULL
        CHECK(length(public_key_spki_der_hex) = 182)
        CHECK(public_key_spki_der_hex NOT GLOB '*[^0-9a-f]*'),
    expected_previous_version INTEGER CHECK(expected_previous_version IS NULL OR expected_previous_version >= 1),
    next_version INTEGER NOT NULL CHECK(next_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, pairing_digest),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER device_public_key_binding_command_validate
BEFORE INSERT ON device_public_key_binding_commands
FOR EACH ROW
BEGIN
    -- Browser pairing may bind only the authenticated actor's own device. The actor identity is
    -- recovered from the one-shot pairing authority by the application layer; arbitrary target
    -- actor selection is intentionally absent from this persistence contract.
    SELECT RAISE(ABORT, 'device_public_key_binding_actor_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );

    -- Initial pairing is valid only when the actor has no binding history. Re-pair/recovery must
    -- name the exact latest immutable lineage version and advance exactly once.
    SELECT RAISE(ABORT, 'device_public_key_binding_version_mismatch')
    WHERE NEW.expected_previous_version IS NULL
      AND (
        NEW.next_version <> 1
        OR EXISTS (
            SELECT 1 FROM device_actor_bindings
            WHERE tenant_id = NEW.tenant_id
              AND actor_id = NEW.actor_id
        )
      );

    SELECT RAISE(ABORT, 'device_public_key_binding_version_mismatch')
    WHERE NEW.expected_previous_version IS NOT NULL
      AND (
        NEW.next_version <> NEW.expected_previous_version + 1
        OR COALESCE((
            SELECT MAX(version) FROM device_actor_bindings
            WHERE tenant_id = NEW.tenant_id
              AND actor_id = NEW.actor_id
        ), 0) <> NEW.expected_previous_version
      );
END;

CREATE TRIGGER device_public_key_binding_command_apply
AFTER INSERT ON device_public_key_binding_commands
FOR EACH ROW
BEGIN
    -- Re-pairing is atomic: the previous active credential is revoked in the same D1 transaction
    -- that publishes the new exact public key. There is no revoke -> bind trust gap.
    UPDATE device_actor_bindings
    SET status = 'REVOKED',
        updated_at_ms = NEW.executed_at_ms,
        revoked_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND status = 'ACTIVE';

    INSERT INTO device_actor_bindings (
        tenant_id, actor_id, device_id, version, status, evidence_reference,
        bound_at_ms, updated_at_ms, revoked_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.actor_id,
        NEW.device_id,
        NEW.next_version,
        'ACTIVE',
        'p256_spki_der:' || NEW.public_key_spki_der_hex,
        NEW.executed_at_ms,
        NEW.executed_at_ms,
        NULL
    );
END;
