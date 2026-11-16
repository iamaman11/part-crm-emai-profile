-- Business display names stay in D1; browser identity, fingerprint and generations are untouched.
ALTER TABLE browser_profiles ADD COLUMN display_name TEXT NOT NULL DEFAULT 'Unnamed profile'
    CHECK(length(trim(display_name)) BETWEEN 1 AND 80);
ALTER TABLE profile_create_commands ADD COLUMN display_name TEXT NOT NULL DEFAULT 'Unnamed profile'
    CHECK(length(trim(display_name)) BETWEEN 1 AND 80);

DROP TRIGGER profile_create_command_apply;
CREATE TRIGGER profile_create_command_apply
AFTER INSERT ON profile_create_commands
FOR EACH ROW
BEGIN
    INSERT INTO browser_profiles (
        tenant_id, profile_id, display_name, status, active_generation_id, version,
        created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.profile_id, NEW.display_name, 'DRAFT', NULL, 1,
        NEW.command_actor_id, NEW.command_actor_id, NEW.executed_at_ms, NEW.executed_at_ms
    );
END;

CREATE TABLE profile_rename_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) BETWEEN 1 AND 80),
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version >= 1),
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
          AND version = NEW.expected_profile_version
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
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;
END;
