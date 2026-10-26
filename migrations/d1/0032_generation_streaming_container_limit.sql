-- #802/K2: enlarge ONLY the two existing append-only generation command journals.
-- The former 80 MiB SQL CHECK contradicted the bounded 608 MiB BPGC file stream.
-- This is a metadata-only SQLite table rebuild; never copy or touch R2/native objects,
-- rewrite actor/coordinator evidence, or invent an additional lifecycle owner.
-- Preserve historical command rows, exact primary keys, CAS/fencing validation,
-- transition triggers, idempotent lookups and append-only protections.
-- D1 release applies this migration atomically; a failed application blocks release.
-- Endpoints and file adapters remain independently capped at MAX_STREAM_CONTAINER_BYTES.
-- No new paths that can delete cloud generations or root keys are introduced.

CREATE TABLE profile_generation_successor_commands_k2 (
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
    container_bytes INTEGER NOT NULL CHECK(container_bytes BETWEEN 1 AND 637534208),
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
INSERT INTO profile_generation_successor_commands_k2
SELECT * FROM profile_generation_successor_commands;
DROP TABLE profile_generation_successor_commands;
ALTER TABLE profile_generation_successor_commands_k2
RENAME TO profile_generation_successor_commands;
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

CREATE TABLE device_generation_commit_commands_k2 (
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL
        CHECK(length(job_id) BETWEEN 8 AND 96)
        CHECK(job_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    profile_id TEXT NOT NULL,
    base_generation_id TEXT NOT NULL
        CHECK(length(base_generation_id) BETWEEN 8 AND 96)
        CHECK(base_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
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
    container_bytes INTEGER NOT NULL CHECK(container_bytes BETWEEN 1 AND 637534208),
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    claim_id TEXT NOT NULL
        CHECK(length(claim_id) BETWEEN 8 AND 96)
        CHECK(claim_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    claim_fence INTEGER NOT NULL CHECK(claim_fence > 0),
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
    PRIMARY KEY (tenant_id, job_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, base_generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, job_id)
        REFERENCES device_jobs(tenant_id, job_id) ON DELETE RESTRICT
) STRICT;
INSERT INTO device_generation_commit_commands_k2
SELECT * FROM device_generation_commit_commands;
DROP TABLE device_generation_commit_commands;
ALTER TABLE device_generation_commit_commands_k2
RENAME TO device_generation_commit_commands;
CREATE INDEX device_generation_commit_profile_lookup
    ON device_generation_commit_commands(
        tenant_id, profile_id, generation_id, executed_at_ms, job_id
    );
CREATE TRIGGER device_generation_commit_command_validate
BEFORE INSERT ON device_generation_commit_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_generation_commit_actor_inactive')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'device_generation_commit_device_binding_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_actor_bindings
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND device_id = NEW.device_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'device_generation_commit_authorization_stale')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_authorizations
        WHERE tenant_id = NEW.tenant_id
          AND device_id = NEW.device_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.base_generation_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'device_generation_commit_claim_stale')
    WHERE NOT EXISTS (
        SELECT 1
        FROM device_jobs
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
          AND NEW.executed_at_ms < claim_lease_expires_at_ms
    );

    SELECT RAISE(ABORT, 'device_generation_commit_base_generation_stale')
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

    SELECT RAISE(ABORT, 'device_generation_commit_candidate_exists')
    WHERE EXISTS (
        SELECT 1
        FROM profile_generations
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND generation_id = NEW.generation_id
    );

    SELECT RAISE(ABORT, 'device_generation_commit_coordinator_stale')
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
          AND idle_expires_at_ms > NEW.executed_at_ms
          AND hard_expires_at_ms > NEW.executed_at_ms
          AND (
              coordinator_status <> 'draining'
              OR drain_deadline_ms > NEW.executed_at_ms
          )
    );
END;

CREATE TRIGGER device_generation_commit_commands_are_append_only_update
BEFORE UPDATE ON device_generation_commit_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_generation_commit_command_append_only');
END;

CREATE TRIGGER device_generation_commit_commands_are_append_only_delete
BEFORE DELETE ON device_generation_commit_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'device_generation_commit_command_append_only');
END;

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

-- A returned foreign-key violation is a hard failure in the accompanying
-- migration test. No deletion/rewrite is permitted if historical rows differ.
PRAGMA foreign_key_check;
