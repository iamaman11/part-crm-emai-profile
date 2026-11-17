-- Business display names stay in D1; browser identity, fingerprint and generations are untouched.
ALTER TABLE browser_profiles ADD COLUMN display_name TEXT NOT NULL DEFAULT 'Unnamed profile'
    CHECK(length(trim(display_name)) BETWEEN 1 AND 80);
-- Separate the independent CRM business metadata OCC from native Firefox CURRENT.
-- The native version remains owned solely by generation/Sync/Restore transitions.
ALTER TABLE browser_profiles ADD COLUMN metadata_version INTEGER NOT NULL DEFAULT 1
    CHECK(metadata_version >= 1);
ALTER TABLE profile_create_commands ADD COLUMN display_name TEXT NOT NULL DEFAULT 'Unnamed profile'
    CHECK(length(trim(display_name)) BETWEEN 1 AND 80);

DROP TRIGGER profile_create_command_apply;
CREATE TRIGGER profile_create_command_apply
AFTER INSERT ON profile_create_commands
FOR EACH ROW
BEGIN
    INSERT INTO browser_profiles (
        tenant_id, profile_id, display_name, status, active_generation_id, version, metadata_version,
        created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.profile_id, NEW.display_name, 'DRAFT', NULL, 1, 1,
        NEW.command_actor_id, NEW.command_actor_id, NEW.executed_at_ms, NEW.executed_at_ms
    );
END;

CREATE TABLE profile_rename_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) BETWEEN 1 AND 80),
    expected_metadata_version INTEGER NOT NULL CHECK(expected_metadata_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_rename_command_validate
BEFORE INSERT ON profile_rename_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_rename_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER' AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_rename_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id
          AND metadata_version = NEW.expected_metadata_version
          AND status NOT IN ('DELETING', 'DELETED')
          AND display_name <> NEW.display_name
    );
END;

CREATE TRIGGER profile_rename_command_apply
AFTER INSERT ON profile_rename_commands
FOR EACH ROW
BEGIN
    UPDATE browser_profiles
    SET display_name = NEW.display_name,
        metadata_version = metadata_version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;
END;

-- Test entities are not backwards-compatible. Change the governed command column names
-- to express the correct optimistic-concurrency owner on the current successor schema.
ALTER TABLE profile_assignment_commands
    RENAME COLUMN expected_profile_version TO expected_metadata_version;
ALTER TABLE profile_grant_commands
    RENAME COLUMN expected_profile_version TO expected_metadata_version;

DROP TRIGGER profile_assignment_command_validate;
CREATE TRIGGER profile_assignment_command_validate
BEFORE INSERT ON profile_assignment_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_assignment_profile_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status NOT IN ('DELETING','DELETED')
    );

    SELECT RAISE(ABORT, 'profile_assignment_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status NOT IN ('DELETING','DELETED')
          AND metadata_version <> NEW.expected_metadata_version
    );

    SELECT RAISE(ABORT, 'profile_assignment_client_not_active')
    WHERE NEW.operation = 'ASSIGN'
      AND NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_assignment_active_assignment_missing')
    WHERE NEW.operation = 'DETACH'
      AND NOT EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND assignment_id = NEW.assignment_id
          AND profile_id = NEW.profile_id
          AND client_id = NEW.client_id
          AND closed_at_ms IS NULL
    );
END;

DROP TRIGGER profile_assignment_history_update_guard;
CREATE TRIGGER profile_assignment_history_update_guard
BEFORE UPDATE ON profile_client_assignments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.assignment_id <> OLD.assignment_id
       OR NEW.profile_id <> OLD.profile_id
       OR NEW.client_id <> OLD.client_id
       OR NEW.assigned_by_actor_id <> OLD.assigned_by_actor_id
       OR NEW.assigned_at_ms <> OLD.assigned_at_ms
       OR NEW.reason <> OLD.reason;

    SELECT RAISE(ABORT, 'profile_assignment_closed_history_immutable')
    WHERE OLD.closed_at_ms IS NOT NULL
      AND NEW.closed_at_ms IS NOT OLD.closed_at_ms;

    SELECT RAISE(ABORT, 'profile_assignment_invalid_close_time')
    WHERE NEW.closed_at_ms IS NOT NULL
      AND NEW.closed_at_ms < OLD.assigned_at_ms;

    SELECT RAISE(ABORT, 'profile_assignment_close_not_governed')
    WHERE OLD.closed_at_ms IS NULL
      AND NEW.closed_at_ms IS NOT NULL
      AND NOT EXISTS (
        SELECT 1
        FROM profile_assignment_commands AS command
        JOIN browser_profiles AS profile
          ON profile.tenant_id = command.tenant_id
         AND profile.profile_id = command.profile_id
         AND profile.metadata_version = command.expected_metadata_version
        WHERE command.tenant_id = OLD.tenant_id
          AND command.profile_id = OLD.profile_id
          AND command.executed_at_ms = NEW.closed_at_ms
          AND (
              command.operation = 'ASSIGN'
              OR (
                  command.operation = 'DETACH'
                  AND command.assignment_id = OLD.assignment_id
                  AND command.client_id = OLD.client_id
              )
          )
    )
      -- Delete is another governed lifecycle command, not a free-standing
      -- UPDATE permission. The close must occur atomically before the
      -- original version is consumed by the DELETING transition.
      AND NOT EXISTS (
          SELECT 1 FROM profile_delete_commands AS deletion
          JOIN browser_profiles AS profile
            ON profile.tenant_id = deletion.tenant_id
           AND profile.profile_id = deletion.profile_id
          WHERE deletion.tenant_id = OLD.tenant_id
            AND deletion.profile_id = OLD.profile_id
            AND deletion.requested_at_ms = NEW.closed_at_ms
            AND profile.version = deletion.expected_profile_version
            AND profile.status IN ('DRAFT','READY','SUSPENDED','QUARANTINED')
      )
      -- Backfill only rows already finalized by the original owner-command.
      -- It never enables history changes to an active or deleting profile.
      AND NOT EXISTS (
          SELECT 1 FROM profile_delete_completion_commands AS completion
          JOIN browser_profiles AS profile
            ON profile.tenant_id = completion.tenant_id
           AND profile.profile_id = completion.profile_id
          WHERE completion.tenant_id = OLD.tenant_id
            AND completion.profile_id = OLD.profile_id
            AND completion.completed_at_ms = NEW.closed_at_ms
            AND profile.status = 'DELETED'
      );
END;

DROP TRIGGER profile_assignment_command_apply_assign;
CREATE TRIGGER profile_assignment_command_apply_assign
AFTER INSERT ON profile_assignment_commands
FOR EACH ROW
WHEN NEW.operation = 'ASSIGN'
BEGIN
    UPDATE profile_client_assignments
    SET closed_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND closed_at_ms IS NULL;

    INSERT INTO profile_client_assignments (
        tenant_id, assignment_id, profile_id, client_id,
        assigned_by_actor_id, assigned_at_ms, reason
    ) VALUES (
        NEW.tenant_id, NEW.assignment_id, NEW.profile_id, NEW.client_id,
        NEW.command_actor_id, NEW.executed_at_ms, NEW.reason
    );

    UPDATE browser_profiles
    SET metadata_version = metadata_version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

DROP TRIGGER profile_assignment_command_apply_detach;
CREATE TRIGGER profile_assignment_command_apply_detach
AFTER INSERT ON profile_assignment_commands
FOR EACH ROW
WHEN NEW.operation = 'DETACH'
BEGIN
    UPDATE profile_client_assignments
    SET closed_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND assignment_id = NEW.assignment_id
      AND profile_id = NEW.profile_id
      AND client_id = NEW.client_id
      AND closed_at_ms IS NULL;

    UPDATE browser_profiles
    SET metadata_version = metadata_version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

DROP TRIGGER profile_grant_command_validate;
CREATE TRIGGER profile_grant_command_validate
BEFORE INSERT ON profile_grant_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_grant_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_grant_target_not_active_member')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND role = 'MEMBER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_grant_profile_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status <> 'DELETED'
    );
    SELECT RAISE(ABORT, 'profile_grant_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status <> 'DELETED'
          AND metadata_version <> NEW.expected_metadata_version
    );
    SELECT RAISE(ABORT, 'profile_grant_missing')
    WHERE NEW.operation = 'REVOKE'
      AND NOT EXISTS (
        SELECT 1 FROM profile_grants
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND profile_id = NEW.profile_id
    );
END;

DROP TRIGGER profile_grant_command_apply_grant;
CREATE TRIGGER profile_grant_command_apply_grant
AFTER INSERT ON profile_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'GRANT'
BEGIN
    INSERT INTO profile_grants (
        tenant_id, actor_id, profile_id, role,
        granted_by_actor_id, reason, created_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.target_actor_id, NEW.profile_id, NEW.role,
        NEW.command_actor_id, NEW.reason, NEW.executed_at_ms
    )
    ON CONFLICT (tenant_id, actor_id, profile_id) DO UPDATE SET
        role = excluded.role,
        granted_by_actor_id = excluded.granted_by_actor_id,
        reason = excluded.reason,
        created_at_ms = excluded.created_at_ms;
    UPDATE browser_profiles
    SET metadata_version = metadata_version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

DROP TRIGGER profile_grant_command_apply_revoke;
CREATE TRIGGER profile_grant_command_apply_revoke
AFTER INSERT ON profile_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'REVOKE'
BEGIN
    DELETE FROM profile_grants
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id
      AND profile_id = NEW.profile_id;
    UPDATE browser_profiles
    SET metadata_version = metadata_version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;
