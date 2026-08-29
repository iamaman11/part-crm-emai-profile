-- CAP-EXEC P3 / CAP12-I3: one atomic Profile generation successor lifecycle owner.
--
-- The immutable encrypted object is uploaded and exactly verified before this command is
-- attempted. This journal owns only the metadata transition
--
--     exact active verified N -> register N+1 -> verify N+1 -> activate N+1
--
-- and is shared by interactive Profile save and the existing device-job flow. Authority remains
-- in its natural owner: interactive launch/session authority is proven by the Profile coordinator
-- Durable Object before this D1 command, while device-job authority is proven by the existing
-- device_generation_commit command. D1 rechecks durable witnesses and performs the catalog
-- transition atomically. Raw fencing tokens, encrypted Profile bytes and key material are never
-- stored here.

CREATE TABLE profile_generation_successor_commands (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    base_generation_id TEXT NOT NULL
        CHECK(length(base_generation_id) BETWEEN 8 AND 96)
        CHECK(base_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    generation_id TEXT NOT NULL
        CHECK(length(generation_id) BETWEEN 8 AND 96)
        CHECK(generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    authority_kind TEXT NOT NULL CHECK(authority_kind IN ('INTERACTIVE_LAUNCH', 'DEVICE_JOB')),
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
    container_bytes INTEGER NOT NULL CHECK(container_bytes BETWEEN 1 AND 83886080),
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version >= 1),
    coordinator_session_id TEXT NOT NULL
        CHECK(length(coordinator_session_id) BETWEEN 8 AND 96)
        CHECK(coordinator_session_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    coordinator_fencing_token_digest TEXT NOT NULL
        CHECK(length(coordinator_fencing_token_digest) = 64)
        CHECK(coordinator_fencing_token_digest NOT GLOB '*[^0-9a-f]*'),
    coordinator_epoch INTEGER NOT NULL CHECK(coordinator_epoch > 0),
    coordinator_version INTEGER NOT NULL CHECK(coordinator_version >= 1),
    coordinator_sequence INTEGER NOT NULL CHECK(coordinator_sequence >= 0),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    CHECK(generation_id <> base_generation_id),
    CHECK(coordinator_version = coordinator_sequence + 1),
    CHECK(
        object_key = 'tenants/' || tenant_id
            || '/profiles/' || profile_id
            || '/generations/' || generation_id || '.bpgc'
    ),
    PRIMARY KEY (tenant_id, profile_id, base_generation_id),
    UNIQUE (tenant_id, profile_id, generation_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, base_generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX profile_generation_successor_lookup
    ON profile_generation_successor_commands(
        tenant_id, profile_id, generation_id, executed_at_ms
    );

CREATE TRIGGER profile_generation_successor_command_validate
BEFORE INSERT ON profile_generation_successor_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_successor_actor_inactive')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND status = 'ACTIVE'
    );

    -- Interactive save must still have operator authority at the exact commit point. A device-job
    -- caller has its own already-validated job capability and is linked below to that command row.
    SELECT RAISE(ABORT, 'profile_generation_successor_profile_access_denied')
    WHERE NEW.authority_kind = 'INTERACTIVE_LAUNCH'
      AND NOT EXISTS (
        SELECT 1
        FROM memberships AS requester
        WHERE requester.tenant_id = NEW.tenant_id
          AND requester.actor_id = NEW.command_actor_id
          AND requester.status = 'ACTIVE'
          AND (
              requester.role = 'TENANT_OWNER'
              OR (
                  requester.role = 'MEMBER'
                  AND EXISTS (
                      SELECT 1
                      FROM profile_grants AS grant_row
                      WHERE grant_row.tenant_id = NEW.tenant_id
                        AND grant_row.actor_id = NEW.command_actor_id
                        AND grant_row.profile_id = NEW.profile_id
                        AND grant_row.role = 'PROFILE_OPERATOR'
                  )
              )
          )
    );

    SELECT RAISE(ABORT, 'profile_generation_successor_device_binding_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND device_id = NEW.device_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_generation_successor_device_authorization_stale')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_authorizations
        WHERE tenant_id = NEW.tenant_id
          AND device_id = NEW.device_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.base_generation_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'profile_generation_successor_device_job_authority_missing')
    WHERE NEW.authority_kind = 'DEVICE_JOB'
      AND NOT EXISTS (
        SELECT 1
        FROM device_generation_commit_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.command_actor_id = NEW.command_actor_id
          AND command.device_id = NEW.device_id
          AND command.profile_id = NEW.profile_id
          AND command.base_generation_id = NEW.base_generation_id
          AND command.generation_id = NEW.generation_id
          AND command.object_key = NEW.object_key
          AND command.metadata_digest = NEW.metadata_digest
          AND command.container_digest = NEW.container_digest
          AND command.container_bytes = NEW.container_bytes
          AND command.expected_profile_version = NEW.expected_profile_version
          AND command.coordinator_session_id = NEW.coordinator_session_id
          AND command.coordinator_fencing_token_digest = NEW.coordinator_fencing_token_digest
          AND command.coordinator_epoch = NEW.coordinator_epoch
          AND command.coordinator_version = NEW.coordinator_version
          AND command.coordinator_sequence = NEW.coordinator_sequence
          AND command.executed_at_ms = NEW.executed_at_ms
    );

    SELECT RAISE(ABORT, 'profile_generation_successor_base_generation_stale')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_profiles AS profile
        JOIN profile_generations AS generation
          ON generation.tenant_id = profile.tenant_id
         AND generation.profile_id = profile.profile_id
         AND generation.generation_id = profile.active_generation_id
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'READY'
          AND profile.version = NEW.expected_profile_version
          AND profile.active_generation_id = NEW.base_generation_id
          AND profile.updated_at_ms <= NEW.executed_at_ms
          AND generation.status = 'VERIFIED'
    );

    SELECT RAISE(ABORT, 'profile_generation_successor_candidate_exists')
    WHERE EXISTS (
        SELECT 1
        FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
    );

    -- The Durable Object is authoritative and validates the raw fencing token plus Claim journal
    -- provenance. This D1 projection is the independent fail-closed witness used by the atomic
    -- catalog transaction; raw fencing tokens are intentionally absent from D1.
    SELECT RAISE(ABORT, 'profile_generation_successor_coordinator_stale')
    WHERE NOT EXISTS (
        SELECT 1
        FROM profile_coordinator_projections
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND coordinator_status IN ('active', 'draining')
          AND coordinator_version = NEW.coordinator_version
          AND coordinator_sequence = NEW.coordinator_sequence
          AND active_session_id = NEW.coordinator_session_id
          AND active_device_id = NEW.device_id
          AND active_epoch = NEW.coordinator_epoch
          AND projected_at_ms <= NEW.executed_at_ms
          AND idle_expires_at_ms > NEW.executed_at_ms
          AND hard_expires_at_ms > NEW.executed_at_ms
          AND (
              coordinator_status <> 'draining'
              OR drain_deadline_ms > NEW.executed_at_ms
          )
    );
END;

-- Automated generation transitions now have exactly one SQL owner: this successor command. The
-- original explicit OWNER/admin generation commands remain valid for their separate public API.
DROP TRIGGER profile_generation_insert_requires_register_command;
CREATE TRIGGER profile_generation_insert_requires_register_command
BEFORE INSERT ON profile_generations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_insert_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generation_register_commands
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND object_key = NEW.object_key
          AND metadata_digest = NEW.metadata_digest
          AND container_digest = NEW.container_digest
          AND command_actor_id = NEW.registered_by_actor_id
          AND executed_at_ms = NEW.created_at_ms
    )
    AND NOT EXISTS (
        SELECT 1 FROM profile_generation_successor_commands
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
          AND object_key = NEW.object_key
          AND metadata_digest = NEW.metadata_digest
          AND container_digest = NEW.container_digest
          AND command_actor_id = NEW.registered_by_actor_id
          AND executed_at_ms = NEW.created_at_ms
    );
END;

DROP TRIGGER profile_generation_transition_requires_command;
CREATE TRIGGER profile_generation_transition_requires_command
BEFORE UPDATE ON profile_generations
FOR EACH ROW
WHEN NEW.status <> OLD.status
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_transition_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generation_verify_commands
        WHERE tenant_id = OLD.tenant_id
          AND profile_id = OLD.profile_id
          AND generation_id = OLD.generation_id
          AND expected_generation_version = OLD.version
          AND verification_reference = NEW.verification_reference
          AND command_actor_id = NEW.verified_by_actor_id
          AND executed_at_ms = NEW.updated_at_ms
          AND OLD.status = 'REGISTERED'
          AND NEW.status = 'VERIFIED'
          AND NEW.version = OLD.version + 1
    )
    AND NOT EXISTS (
        SELECT 1 FROM profile_generation_quarantine_commands
        WHERE tenant_id = OLD.tenant_id
          AND profile_id = OLD.profile_id
          AND generation_id = OLD.generation_id
          AND expected_generation_version = OLD.version
          AND command_actor_id = NEW.quarantined_by_actor_id
          AND executed_at_ms = NEW.updated_at_ms
          AND OLD.status IN ('REGISTERED', 'VERIFIED')
          AND NEW.status = 'QUARANTINED'
          AND NEW.version = OLD.version + 1
    )
    AND NOT EXISTS (
        SELECT 1 FROM profile_generation_successor_commands
        WHERE tenant_id = OLD.tenant_id
          AND profile_id = OLD.profile_id
          AND generation_id = OLD.generation_id
          AND command_actor_id = NEW.verified_by_actor_id
          AND NEW.verification_reference = 'r2sha256:' || container_digest
          AND executed_at_ms = NEW.updated_at_ms
          AND OLD.status = 'REGISTERED'
          AND OLD.version = 1
          AND NEW.status = 'VERIFIED'
          AND NEW.version = 2
    );
END;

DROP TRIGGER profile_generation_activation_requires_command;
CREATE TRIGGER profile_generation_activation_requires_command
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW
WHEN NEW.active_generation_id IS NOT OLD.active_generation_id
 AND NEW.active_generation_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_activation_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_generation_activate_commands
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.active_generation_id
          AND expected_profile_version = OLD.version
          AND command_actor_id = NEW.updated_by_actor_id
          AND executed_at_ms = NEW.updated_at_ms
          AND NEW.version = OLD.version + 1
    )
    AND NOT EXISTS (
        SELECT 1 FROM profile_generation_successor_commands
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND base_generation_id = OLD.active_generation_id
          AND generation_id = NEW.active_generation_id
          AND expected_profile_version = OLD.version
          AND command_actor_id = NEW.updated_by_actor_id
          AND executed_at_ms = NEW.updated_at_ms
          AND NEW.version = OLD.version + 1
    );
END;

CREATE TRIGGER profile_generation_successor_command_apply
AFTER INSERT ON profile_generation_successor_commands
FOR EACH ROW
BEGIN
    INSERT INTO profile_generations (
        tenant_id,
        profile_id,
        generation_id,
        object_key,
        metadata_digest,
        container_digest,
        status,
        version,
        registered_by_actor_id,
        created_at_ms,
        updated_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.profile_id,
        NEW.generation_id,
        NEW.object_key,
        NEW.metadata_digest,
        NEW.container_digest,
        'REGISTERED',
        1,
        NEW.command_actor_id,
        NEW.executed_at_ms,
        NEW.executed_at_ms
    );

    UPDATE profile_generations
    SET status = 'VERIFIED',
        version = 2,
        verification_reference = 'r2sha256:' || NEW.container_digest,
        verified_by_actor_id = NEW.command_actor_id,
        verified_at_ms = NEW.executed_at_ms,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND generation_id = NEW.generation_id
      AND status = 'REGISTERED'
      AND version = 1;

    SELECT RAISE(ABORT, 'profile_generation_successor_verify_incomplete')
    WHERE changes() <> 1;

    UPDATE browser_profiles
    SET active_generation_id = NEW.generation_id,
        status = 'READY',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND version = NEW.expected_profile_version
      AND status = 'READY'
      AND active_generation_id = NEW.base_generation_id
      AND EXISTS (
          SELECT 1
          FROM profile_generations
          WHERE tenant_id = NEW.tenant_id
            AND profile_id = NEW.profile_id
            AND generation_id = NEW.generation_id
            AND object_key = NEW.object_key
            AND metadata_digest = NEW.metadata_digest
            AND container_digest = NEW.container_digest
            AND status = 'VERIFIED'
            AND version = 2
      );

    SELECT RAISE(ABORT, 'profile_generation_successor_activate_incomplete')
    WHERE changes() <> 1;
END;

CREATE TRIGGER profile_generation_successor_commands_are_append_only_update
BEFORE UPDATE ON profile_generation_successor_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_successor_command_append_only');
END;

CREATE TRIGGER profile_generation_successor_commands_are_append_only_delete
BEFORE DELETE ON profile_generation_successor_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_successor_command_append_only');
END;

-- The device-job flow remains one outer transaction, but it no longer owns or duplicates the
-- generation lifecycle. Its already-validated command delegates the exact successor metadata to
-- the generalized owner, then terminalizes the job. Any later failure rolls back both effects.
DROP TRIGGER device_generation_commit_command_apply;
CREATE TRIGGER device_generation_commit_command_apply
AFTER INSERT ON device_generation_commit_commands
FOR EACH ROW
BEGIN
    INSERT INTO profile_generation_successor_commands (
        tenant_id,
        profile_id,
        base_generation_id,
        generation_id,
        command_actor_id,
        device_id,
        authority_kind,
        object_key,
        metadata_digest,
        container_digest,
        container_bytes,
        expected_profile_version,
        coordinator_session_id,
        coordinator_fencing_token_digest,
        coordinator_epoch,
        coordinator_version,
        coordinator_sequence,
        executed_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.profile_id,
        NEW.base_generation_id,
        NEW.generation_id,
        NEW.command_actor_id,
        NEW.device_id,
        'DEVICE_JOB',
        NEW.object_key,
        NEW.metadata_digest,
        NEW.container_digest,
        NEW.container_bytes,
        NEW.expected_profile_version,
        NEW.coordinator_session_id,
        NEW.coordinator_fencing_token_digest,
        NEW.coordinator_epoch,
        NEW.coordinator_version,
        NEW.coordinator_sequence,
        NEW.executed_at_ms
    );

    UPDATE device_jobs
    SET aggregate_version = aggregate_version + 1,
        status = 'SUCCEEDED',
        current_claim_id = NULL,
        claim_fence = NULL,
        claimed_at_ms = NULL,
        claim_heartbeat_at_ms = NULL,
        claim_lease_expires_at_ms = NULL,
        retry_at_ms = NULL,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND job_id = NEW.job_id
      AND device_id = NEW.device_id
      AND profile_id = NEW.profile_id
      AND generation_id = NEW.base_generation_id
      AND aggregate_version = NEW.expected_job_version
      AND status = 'RUNNING'
      AND current_claim_id = NEW.claim_id
      AND claim_fence = NEW.claim_fence
      AND last_fence = NEW.claim_fence
      AND updated_at_ms <= NEW.executed_at_ms
      AND claim_heartbeat_at_ms <= NEW.executed_at_ms
      AND NEW.executed_at_ms < claim_lease_expires_at_ms;

    SELECT RAISE(ABORT, 'device_generation_commit_job_terminalize_incomplete')
    WHERE changes() <> 1;
END;
