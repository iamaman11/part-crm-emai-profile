-- Ordinary first preparation reuses the governed generation lifecycle.
-- NULL evidence retains the owner-only administrative registration contract.
ALTER TABLE profile_generation_register_commands ADD COLUMN initial_device_id TEXT
    CHECK(initial_device_id IS NULL OR (length(initial_device_id) BETWEEN 8 AND 96
        AND initial_device_id NOT GLOB '*[^A-Za-z0-9_-]*'));
ALTER TABLE profile_generation_register_commands ADD COLUMN initial_expected_profile_version INTEGER
    CHECK(initial_expected_profile_version IS NULL OR initial_expected_profile_version >= 1);

DROP TRIGGER profile_generation_register_command_validate;
CREATE TRIGGER profile_generation_register_command_validate
BEFORE INSERT ON profile_generation_register_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_register_owner_required')
    WHERE NEW.initial_device_id IS NULL AND NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_generation_initialization_invalid_state')
    WHERE (NEW.initial_device_id IS NULL) <> (NEW.initial_expected_profile_version IS NULL);
    SELECT RAISE(ABORT, 'profile_generation_initialization_access_required')
    WHERE NEW.initial_device_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM memberships AS member
        JOIN device_actor_bindings AS binding
          ON binding.tenant_id = member.tenant_id AND binding.actor_id = member.actor_id
        JOIN browser_profiles AS profile ON profile.tenant_id = member.tenant_id
        WHERE member.tenant_id = NEW.tenant_id AND member.actor_id = NEW.command_actor_id
          AND member.status = 'ACTIVE' AND binding.status = 'ACTIVE'
          AND binding.device_id = NEW.initial_device_id
          AND profile.profile_id = NEW.profile_id AND profile.status = 'DRAFT'
          AND profile.active_generation_id IS NULL AND profile.version = NEW.initial_expected_profile_version
          AND (member.role = 'TENANT_OWNER' OR (member.role = 'MEMBER' AND EXISTS (
              SELECT 1 FROM profile_grants AS grant
              WHERE grant.tenant_id = member.tenant_id AND grant.actor_id = member.actor_id
                AND grant.profile_id = profile.profile_id AND grant.role = 'PROFILE_OPERATOR'
          )))
    );
    SELECT RAISE(ABORT, 'profile_generation_register_profile_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status <> 'DELETED'
    );
    SELECT RAISE(ABORT, 'profile_generation_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND NEW.executed_at_ms < updated_at_ms
    );
END;

DROP TRIGGER profile_generation_verify_command_validate;
CREATE TRIGGER profile_generation_verify_command_validate
BEFORE INSERT ON profile_generation_verify_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_verify_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    ) AND NOT EXISTS (
        SELECT 1 FROM profile_generation_register_commands AS registration
        WHERE registration.tenant_id = NEW.tenant_id AND registration.command_id = NEW.command_id
          AND registration.command_actor_id = NEW.command_actor_id
          AND registration.profile_id = NEW.profile_id AND registration.generation_id = NEW.generation_id
          AND registration.initial_device_id IS NOT NULL
          AND registration.executed_at_ms = NEW.executed_at_ms
          AND EXISTS (
        SELECT 1 FROM memberships AS member
        JOIN device_actor_bindings AS binding
          ON binding.tenant_id = member.tenant_id AND binding.actor_id = member.actor_id
        JOIN browser_profiles AS profile ON profile.tenant_id = member.tenant_id
        WHERE member.tenant_id = NEW.tenant_id AND member.actor_id = NEW.command_actor_id
          AND member.status = 'ACTIVE' AND binding.status = 'ACTIVE'
          AND binding.device_id = registration.initial_device_id
          AND profile.profile_id = NEW.profile_id AND profile.status = 'DRAFT'
          AND profile.active_generation_id IS NULL AND profile.version = registration.initial_expected_profile_version
          AND (member.role = 'TENANT_OWNER' OR (member.role = 'MEMBER' AND EXISTS (
              SELECT 1 FROM profile_grants AS grant
              WHERE grant.tenant_id = member.tenant_id AND grant.actor_id = member.actor_id
                AND grant.profile_id = profile.profile_id AND grant.role = 'PROFILE_OPERATOR'
          )))
    )
    );
    SELECT RAISE(ABORT, 'profile_generation_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND NEW.executed_at_ms < updated_at_ms
    );
    SELECT RAISE(ABORT, 'profile_generation_verify_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND status = 'REGISTERED'
          AND version = NEW.expected_generation_version
    );
END;

DROP TRIGGER profile_generation_activate_command_validate;
CREATE TRIGGER profile_generation_activate_command_validate
BEFORE INSERT ON profile_generation_activate_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_activate_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    ) AND NOT EXISTS (
        SELECT 1 FROM profile_generation_register_commands AS registration
        WHERE registration.tenant_id = NEW.tenant_id AND registration.command_id = NEW.command_id
          AND registration.command_actor_id = NEW.command_actor_id
          AND registration.profile_id = NEW.profile_id AND registration.generation_id = NEW.generation_id
          AND registration.initial_device_id IS NOT NULL
          AND registration.executed_at_ms = NEW.executed_at_ms
          AND EXISTS (
        SELECT 1 FROM memberships AS member
        JOIN device_actor_bindings AS binding
          ON binding.tenant_id = member.tenant_id AND binding.actor_id = member.actor_id
        JOIN browser_profiles AS profile ON profile.tenant_id = member.tenant_id
        WHERE member.tenant_id = NEW.tenant_id AND member.actor_id = NEW.command_actor_id
          AND member.status = 'ACTIVE' AND binding.status = 'ACTIVE'
          AND binding.device_id = registration.initial_device_id
          AND profile.profile_id = NEW.profile_id AND profile.status = 'DRAFT'
          AND profile.active_generation_id IS NULL AND profile.version = registration.initial_expected_profile_version
          AND (member.role = 'TENANT_OWNER' OR (member.role = 'MEMBER' AND EXISTS (
              SELECT 1 FROM profile_grants AS grant
              WHERE grant.tenant_id = member.tenant_id AND grant.actor_id = member.actor_id
                AND grant.profile_id = profile.profile_id AND grant.role = 'PROFILE_OPERATOR'
          )))
    )
    );
    SELECT RAISE(ABORT, 'profile_generation_initialization_state_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM profile_generation_register_commands AS registration
        WHERE registration.tenant_id = NEW.tenant_id AND registration.command_id = NEW.command_id
          AND registration.initial_device_id IS NOT NULL
          AND (registration.command_actor_id <> NEW.command_actor_id
            OR registration.profile_id <> NEW.profile_id OR registration.generation_id <> NEW.generation_id
            OR registration.initial_expected_profile_version <> NEW.expected_profile_version)
    );
    SELECT RAISE(ABORT, 'profile_generation_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND NEW.executed_at_ms < updated_at_ms
    ) OR EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND NEW.executed_at_ms < updated_at_ms
    );
    SELECT RAISE(ABORT, 'profile_generation_not_verified')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND status = 'VERIFIED'
    );
    SELECT RAISE(ABORT, 'profile_generation_activate_profile_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND version = NEW.expected_profile_version
          AND status IN ('DRAFT', 'QUARANTINED', 'READY', 'SUSPENDED')
          AND NOT (
              status = 'READY'
              AND active_generation_id = NEW.generation_id
          )
    );
END;

-- A user may register several computers. Actor-wide lineage versions remain CAS evidence,
-- while key replacement/revocation affects only the named computer.
DROP INDEX device_actor_bindings_one_active_actor;
CREATE UNIQUE INDEX device_actor_bindings_one_active_actor_device
    ON device_actor_bindings(tenant_id, actor_id, device_id) WHERE status = 'ACTIVE';
CREATE UNIQUE INDEX device_actor_bindings_one_active_device
    ON device_actor_bindings(tenant_id, device_id) WHERE status = 'ACTIVE';

DROP TRIGGER device_public_key_binding_command_apply;
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
      AND device_id = NEW.device_id
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

DROP TRIGGER device_binding_bind_command_apply;
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
      AND device_id = NEW.device_id
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

DROP TRIGGER device_binding_revoke_command_validate;
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
    WHERE NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id AND actor_id = NEW.target_actor_id
          AND version = NEW.expected_version
    );

    SELECT RAISE(ABORT, 'device_binding_invalid_transition')
    WHERE NOT EXISTS (
        SELECT 1 FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND version = NEW.expected_version
          AND status = 'ACTIVE'
    );
END;
