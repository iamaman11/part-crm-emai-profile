-- Profile generation registry and governed lifecycle commands.
-- The catalog stores metadata-only immutable object identity and digests. It does
-- not store encrypted object bytes, keys, raw profile contents or verification data.

CREATE TABLE profile_generations (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL
        CHECK(length(generation_id) BETWEEN 8 AND 96)
        CHECK(generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    object_key TEXT NOT NULL
        CHECK(length(object_key) BETWEEN 16 AND 512)
        CHECK(substr(object_key, 1, 1) <> '/')
        CHECK(instr(object_key, '\\') = 0)
        CHECK(instr(object_key, '..') = 0)
        CHECK(object_key NOT GLOB '*[^A-Za-z0-9_./:-]*'),
    metadata_digest TEXT NOT NULL
        CHECK(length(metadata_digest) = 64)
        CHECK(metadata_digest NOT GLOB '*[^0-9a-f]*'),
    container_digest TEXT NOT NULL
        CHECK(length(container_digest) = 64)
        CHECK(container_digest NOT GLOB '*[^0-9a-f]*'),
    status TEXT NOT NULL CHECK(status IN ('REGISTERED', 'VERIFIED', 'QUARANTINED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    verification_reference TEXT
        CHECK(verification_reference IS NULL OR (
            length(verification_reference) BETWEEN 8 AND 256
            AND verification_reference NOT GLOB '*[^A-Za-z0-9_:-]*'
        )),
    registered_by_actor_id TEXT NOT NULL,
    verified_by_actor_id TEXT,
    quarantined_by_actor_id TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    verified_at_ms INTEGER CHECK(verified_at_ms IS NULL OR verified_at_ms >= created_at_ms),
    quarantined_at_ms INTEGER CHECK(quarantined_at_ms IS NULL OR quarantined_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, profile_id, generation_id),
    UNIQUE (tenant_id, object_key),
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, registered_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, verified_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, quarantined_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX profile_generations_status_lookup
    ON profile_generations(tenant_id, profile_id, status, generation_id);

CREATE TABLE profile_generation_register_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    object_key TEXT NOT NULL,
    metadata_digest TEXT NOT NULL,
    container_digest TEXT NOT NULL,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_generation_register_command_validate
BEFORE INSERT ON profile_generation_register_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_register_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
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

CREATE TRIGGER profile_generation_register_command_apply
AFTER INSERT ON profile_generation_register_commands
FOR EACH ROW
BEGIN
    INSERT INTO profile_generations (
        tenant_id, profile_id, generation_id, object_key,
        metadata_digest, container_digest, status, version,
        registered_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.profile_id, NEW.generation_id, NEW.object_key,
        NEW.metadata_digest, NEW.container_digest, 'REGISTERED', 1,
        NEW.command_actor_id, NEW.executed_at_ms, NEW.executed_at_ms
    );
END;

CREATE TABLE profile_generation_verify_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    expected_generation_version INTEGER NOT NULL CHECK(expected_generation_version >= 1),
    verification_reference TEXT NOT NULL,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT
) STRICT;

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

CREATE TRIGGER profile_generation_verify_command_apply
AFTER INSERT ON profile_generation_verify_commands
FOR EACH ROW
BEGIN
    UPDATE profile_generations
    SET status = 'VERIFIED',
        version = version + 1,
        verification_reference = NEW.verification_reference,
        verified_by_actor_id = NEW.command_actor_id,
        verified_at_ms = NEW.executed_at_ms,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND generation_id = NEW.generation_id;
END;

CREATE TABLE profile_generation_activate_commands (
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

CREATE TRIGGER profile_generation_activate_command_apply
AFTER INSERT ON profile_generation_activate_commands
FOR EACH ROW
BEGIN
    UPDATE browser_profiles
    SET active_generation_id = NEW.generation_id,
        status = 'READY',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

CREATE TABLE profile_generation_quarantine_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    expected_generation_version INTEGER NOT NULL CHECK(expected_generation_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_generation_quarantine_command_validate
BEFORE INSERT ON profile_generation_quarantine_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_quarantine_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_generation_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND NEW.executed_at_ms < updated_at_ms
    );
    SELECT RAISE(ABORT, 'profile_generation_quarantine_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND status IN ('REGISTERED', 'VERIFIED')
          AND version = NEW.expected_generation_version
    );
    SELECT RAISE(ABORT, 'active_profile_generation_cannot_be_quarantined')
    WHERE EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND active_generation_id = NEW.generation_id
    );
END;

CREATE TRIGGER profile_generation_quarantine_command_apply
AFTER INSERT ON profile_generation_quarantine_commands
FOR EACH ROW
BEGIN
    UPDATE profile_generations
    SET status = 'QUARANTINED',
        version = version + 1,
        quarantined_by_actor_id = NEW.command_actor_id,
        quarantined_at_ms = NEW.executed_at_ms,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND generation_id = NEW.generation_id;
END;
