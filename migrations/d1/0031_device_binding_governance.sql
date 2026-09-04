-- CAP-EXEC V2 / TX-2: canonical governed lifecycle for Profile Bridge machine identity.
--
-- device_actor_bindings remains the only authoritative trust-state table. These command journals
-- add the same transaction-fatal optimistic/idempotent mutation boundary used by the existing
-- governed identity operations. Certificate/private-key material is never stored here: only the
-- canonical SHA-256 certificate fingerprint is admitted and projected into evidence_reference.

-- A verified client certificate may authenticate at most one ACTIVE device principal. The Bridge
-- reader already treats duplicate ACTIVE fingerprints as integrity failure; make that invariant
-- mechanical at the persistence owner.
CREATE UNIQUE INDEX device_actor_bindings_one_active_certificate
    ON device_actor_bindings(evidence_reference)
    WHERE status = 'ACTIVE' AND evidence_reference LIKE 'mtls_cert_sha256:%';

CREATE TABLE device_binding_bind_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    certificate_sha256 TEXT NOT NULL
        CHECK(length(certificate_sha256) = 64)
        CHECK(certificate_sha256 NOT GLOB '*[^0-9a-f]*'),
    expected_previous_version INTEGER CHECK(expected_previous_version IS NULL OR expected_previous_version >= 1),
    next_version INTEGER NOT NULL CHECK(next_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER device_binding_bind_command_validate
BEFORE INSERT ON device_binding_bind_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_binding_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'device_binding_target_not_active_member')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND status = 'ACTIVE'
    );

    -- Initial enrollment is valid only when this actor has no device-binding history at all.
    SELECT RAISE(ABORT, 'device_binding_version_mismatch')
    WHERE NEW.expected_previous_version IS NULL
      AND (
        NEW.next_version <> 1
        OR EXISTS (
            SELECT 1 FROM device_actor_bindings
            WHERE tenant_id = NEW.tenant_id
              AND actor_id = NEW.target_actor_id
        )
      );

    -- Rebind/reenroll must name the exact latest immutable lineage version and advance by one.
    SELECT RAISE(ABORT, 'device_binding_version_mismatch')
    WHERE NEW.expected_previous_version IS NOT NULL
      AND (
        NEW.next_version <> NEW.expected_previous_version + 1
        OR COALESCE((
            SELECT MAX(version) FROM device_actor_bindings
            WHERE tenant_id = NEW.tenant_id
              AND actor_id = NEW.target_actor_id
        ), 0) <> NEW.expected_previous_version
      );
END;

CREATE TRIGGER device_binding_bind_command_apply
AFTER INSERT ON device_binding_bind_commands
FOR EACH ROW
BEGIN
    -- Atomic rebind: the prior ACTIVE machine stops authenticating in the same D1 batch/transaction
    -- that publishes the new fingerprint. There is no workflow-owned revoke -> bind race window.
    UPDATE device_actor_bindings
    SET status = 'REVOKED',
        updated_at_ms = NEW.executed_at_ms,
        revoked_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id
      AND status = 'ACTIVE';

    INSERT INTO device_actor_bindings (
        tenant_id, actor_id, device_id, version, status, evidence_reference,
        bound_at_ms, updated_at_ms, revoked_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.target_actor_id,
        NEW.device_id,
        NEW.next_version,
        'ACTIVE',
        'mtls_cert_sha256:' || NEW.certificate_sha256,
        NEW.executed_at_ms,
        NEW.executed_at_ms,
        NULL
    );
END;

CREATE TABLE device_binding_revoke_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    expected_version INTEGER NOT NULL CHECK(expected_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER device_binding_revoke_command_validate
BEFORE INSERT ON device_binding_revoke_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_binding_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    -- Revocation remains possible after the target membership itself is suspended/revoked so that
    -- compromise cleanup cannot be blocked by ordering. The membership must still exist.
    SELECT RAISE(ABORT, 'device_binding_target_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
    );

    SELECT RAISE(ABORT, 'device_binding_target_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
    );

    SELECT RAISE(ABORT, 'device_binding_version_mismatch')
    WHERE COALESCE((
        SELECT MAX(version) FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
    ), 0) <> NEW.expected_version;

    SELECT RAISE(ABORT, 'device_binding_invalid_transition')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND version = NEW.expected_version
          AND status = 'ACTIVE'
    );
END;

CREATE TRIGGER device_binding_revoke_command_apply
AFTER INSERT ON device_binding_revoke_commands
FOR EACH ROW
BEGIN
    UPDATE device_actor_bindings
    SET status = 'REVOKED',
        updated_at_ms = NEW.executed_at_ms,
        revoked_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id
      AND version = NEW.expected_version
      AND status = 'ACTIVE';
END;
