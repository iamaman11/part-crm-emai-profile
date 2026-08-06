-- Governed deactivation is the only path that clears an active generation pointer.
-- It leaves the profile suspended so the former generation may be quarantined or a
-- different verified generation may be activated later.

CREATE TABLE profile_generation_deactivate_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_generation_deactivate_command_validate
BEFORE INSERT ON profile_generation_deactivate_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_deactivate_owner_required')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_generation_time_regression')
    WHERE EXISTS (
        SELECT 1
        FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND NEW.executed_at_ms < updated_at_ms
    );

    SELECT RAISE(ABORT, 'profile_generation_deactivate_profile_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status = 'READY'
          AND active_generation_id = NEW.generation_id
          AND version = NEW.expected_profile_version
    );
END;

CREATE TRIGGER profile_generation_deactivate_command_apply
AFTER INSERT ON profile_generation_deactivate_commands
FOR EACH ROW
BEGIN
    UPDATE browser_profiles
    SET active_generation_id = NULL,
        status = 'SUSPENDED',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

CREATE TRIGGER profile_generation_deactivation_requires_command
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW
WHEN OLD.active_generation_id IS NOT NULL
 AND NEW.active_generation_id IS NULL
 AND NOT EXISTS (
     SELECT 1
     FROM profile_generation_deactivate_commands AS command
     WHERE command.tenant_id = OLD.tenant_id
       AND command.profile_id = OLD.profile_id
       AND command.generation_id = OLD.active_generation_id
       AND command.expected_profile_version = OLD.version
       AND command.command_actor_id = NEW.updated_by_actor_id
       AND command.executed_at_ms = NEW.updated_at_ms
 )
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_deactivation_not_governed');
END;

CREATE TRIGGER profile_generation_object_key_rejects_backslash_insert
BEFORE INSERT ON profile_generations
FOR EACH ROW
WHEN instr(NEW.object_key, char(92)) <> 0
BEGIN
    SELECT RAISE(ABORT, 'invalid_profile_generation_object_key');
END;

CREATE TRIGGER profile_generation_object_key_rejects_backslash_update
BEFORE UPDATE OF object_key ON profile_generations
FOR EACH ROW
WHEN instr(NEW.object_key, char(92)) <> 0
BEGIN
    SELECT RAISE(ABORT, 'invalid_profile_generation_object_key');
END;

-- Validate any rows created before this defense-in-depth migration.
UPDATE profile_generations
SET object_key = object_key;
