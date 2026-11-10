-- #849: durable, exact-object profile deletion. D1 is the only lifecycle
-- owner. R2 deletion happens OUTSIDE the transaction only after a command
-- has frozen the profile; one object receipt follows an exact-key R2 absence.
-- This migration does not delete or migrate any existing test profile.

-- One D1 owner also fences pre-signed immutable R2 PUTs against DELETE.
-- A capability is NOT returned before its bound reservation is committed.
-- Signatures expire in 300s; 60s conservatively covers in-flight delivery.
CREATE TABLE profile_generation_upload_reservations (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    source_mode TEXT NOT NULL CHECK(source_mode IN ('DRAFT','READY')),
    source_generation_id TEXT,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version > 0),
    issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms = issued_at_ms + 360000),
    PRIMARY KEY(tenant_id, profile_id),
    FOREIGN KEY(tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    CHECK (
        (source_mode = 'DRAFT' AND source_generation_id IS NULL)
        OR
        (source_mode = 'READY' AND source_generation_id IS NOT NULL)
    )
) STRICT;

CREATE TRIGGER profile_generation_upload_reservation_admit
BEFORE INSERT ON profile_generation_upload_reservations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'upload_reservation_profile_not_writable')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = NEW.source_mode
          AND profile.version = NEW.expected_profile_version
          AND (
              (NEW.source_mode = 'DRAFT' AND profile.active_generation_id IS NULL)
              OR
              (NEW.source_mode = 'READY'
                AND profile.active_generation_id = NEW.source_generation_id)
          )
    );
END;

CREATE TRIGGER profile_generation_upload_reservation_update
BEFORE UPDATE ON profile_generation_upload_reservations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'upload_reservation_profile_not_writable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.profile_id <> OLD.profile_id
       OR NEW.issued_at_ms < OLD.issued_at_ms
       OR NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = NEW.source_mode
          AND profile.version = NEW.expected_profile_version
          AND (
              (NEW.source_mode = 'DRAFT' AND profile.active_generation_id IS NULL)
              OR
              (NEW.source_mode = 'READY'
                AND profile.active_generation_id = NEW.source_generation_id)
          )
    );
END;

CREATE TABLE profile_delete_commands (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version > 0),
    requested_at_ms INTEGER NOT NULL CHECK(requested_at_ms >= 0),
    PRIMARY KEY(tenant_id, profile_id),
    UNIQUE(tenant_id, command_id),
    FOREIGN KEY(tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE profile_delete_objects (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    object_key TEXT NOT NULL,
    metadata_digest TEXT NOT NULL,
    container_digest TEXT NOT NULL,
    PRIMARY KEY(tenant_id, profile_id, generation_id),
    UNIQUE(tenant_id, object_key),
    FOREIGN KEY(tenant_id, profile_id)
        REFERENCES profile_delete_commands(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    CHECK(length(object_key) > 0 AND instr(object_key, char(92)) = 0),
    CHECK(length(metadata_digest) = 64
      AND metadata_digest NOT GLOB '*[^0-9a-f]*'),
    CHECK(length(container_digest) = 64
      AND container_digest NOT GLOB '*[^0-9a-f]*')
) STRICT;

CREATE TABLE profile_delete_object_receipts (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    object_key TEXT NOT NULL,
    metadata_digest TEXT NOT NULL,
    container_digest TEXT NOT NULL,
    deleted_at_ms INTEGER NOT NULL CHECK(deleted_at_ms >= 0),
    PRIMARY KEY(tenant_id, profile_id, generation_id),
    FOREIGN KEY(tenant_id, profile_id, generation_id)
        REFERENCES profile_delete_objects(tenant_id, profile_id, generation_id)
        ON DELETE RESTRICT
) STRICT;

CREATE TABLE profile_delete_completion_commands (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version > 0),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    PRIMARY KEY(tenant_id, profile_id),
    FOREIGN KEY(tenant_id, profile_id)
        REFERENCES profile_delete_commands(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_delete_admit
BEFORE INSERT ON profile_delete_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships AS member
        WHERE member.tenant_id = NEW.tenant_id
          AND member.actor_id = NEW.command_actor_id
          AND member.status = 'ACTIVE'
          AND member.role = 'TENANT_OWNER'
    );
    SELECT RAISE(ABORT, 'profile_delete_version_or_state_conflict')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.version = NEW.expected_profile_version
          AND profile.status IN ('DRAFT','READY','SUSPENDED','QUARANTINED')
          AND profile.updated_at_ms <= NEW.requested_at_ms
    );
    SELECT RAISE(ABORT, 'profile_delete_coordinator_busy')
    WHERE EXISTS (
        SELECT 1 FROM profile_coordinator_projections AS coordinator
        WHERE coordinator.tenant_id = NEW.tenant_id
          AND coordinator.profile_id = NEW.profile_id
          AND (
            coordinator.coordinator_status <> 'idle'
            OR coordinator.pending_launch_intent_id IS NOT NULL
          )
    );
    SELECT RAISE(ABORT, 'profile_delete_device_job_busy')
    WHERE EXISTS (
        SELECT 1 FROM device_jobs AS job
        WHERE job.tenant_id = NEW.tenant_id
          AND job.profile_id = NEW.profile_id
          AND job.status NOT IN ('SUCCEEDED','FAILED','CANCELLED')
    );
    SELECT RAISE(ABORT, 'profile_delete_unredeemed_launch')
    WHERE EXISTS (
        SELECT 1 FROM profile_launch_claims AS claim
        WHERE claim.tenant_id = NEW.tenant_id
          AND claim.profile_id = NEW.profile_id
          AND claim.redeemed_at_ms IS NULL
          AND claim.expires_at_ms > NEW.requested_at_ms
    );
END;

CREATE TRIGGER profile_delete_command_apply
AFTER INSERT ON profile_delete_commands
FOR EACH ROW
BEGIN
    INSERT INTO profile_delete_objects (
        tenant_id, profile_id, generation_id, object_key,
        metadata_digest, container_digest
    )
    SELECT tenant_id, profile_id, generation_id, object_key,
           metadata_digest, container_digest
    FROM profile_generations
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;

    UPDATE browser_profiles
    SET status = 'DELETING',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.requested_at_ms
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;
END;

CREATE TRIGGER profile_delete_object_receipt_admit
BEFORE INSERT ON profile_delete_object_receipts
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_presigned_put_not_expired')
    WHERE EXISTS (
        SELECT 1 FROM profile_delete_commands AS command
        LEFT JOIN profile_generation_upload_reservations AS reservation
          ON reservation.tenant_id = command.tenant_id
         AND reservation.profile_id = command.profile_id
        WHERE command.tenant_id = NEW.tenant_id
          AND command.profile_id = NEW.profile_id
          AND NEW.deleted_at_ms < MAX(
              command.requested_at_ms + 360000,
              COALESCE(reservation.expires_at_ms, 0)
          )
    );
    SELECT RAISE(ABORT, 'profile_delete_exact_object_not_frozen')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_delete_objects AS object
        JOIN browser_profiles AS profile
          ON profile.tenant_id = object.tenant_id
         AND profile.profile_id = object.profile_id
        WHERE object.tenant_id = NEW.tenant_id
          AND object.profile_id = NEW.profile_id
          AND object.generation_id = NEW.generation_id
          AND object.object_key = NEW.object_key
          AND object.metadata_digest = NEW.metadata_digest
          AND object.container_digest = NEW.container_digest
          AND profile.status = 'DELETING'
          AND NEW.deleted_at_ms >= profile.updated_at_ms
    );
END;

CREATE TRIGGER profile_delete_complete_admit
BEFORE INSERT ON profile_delete_completion_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_complete_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_delete_complete_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        JOIN profile_delete_commands AS command
          ON command.tenant_id = profile.tenant_id
         AND command.profile_id = profile.profile_id
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'DELETING'
          AND profile.version = NEW.expected_profile_version
          AND NEW.completed_at_ms >= profile.updated_at_ms
    );
    SELECT RAISE(ABORT, 'profile_delete_objects_remain')
    WHERE EXISTS (
        SELECT 1 FROM profile_delete_objects AS object
        WHERE object.tenant_id = NEW.tenant_id
          AND object.profile_id = NEW.profile_id
          AND NOT EXISTS (
              SELECT 1 FROM profile_delete_object_receipts AS receipt
              WHERE receipt.tenant_id = object.tenant_id
                AND receipt.profile_id = object.profile_id
                AND receipt.generation_id = object.generation_id
                AND receipt.object_key = object.object_key
                AND receipt.metadata_digest = object.metadata_digest
                AND receipt.container_digest = object.container_digest
          )
    );
    SELECT RAISE(ABORT, 'profile_delete_complete_busy')
    WHERE EXISTS (
        SELECT 1 FROM profile_coordinator_projections AS coordinator
        WHERE coordinator.tenant_id = NEW.tenant_id
          AND coordinator.profile_id = NEW.profile_id
          AND (
            coordinator.coordinator_status <> 'idle'
            OR coordinator.pending_launch_intent_id IS NOT NULL
          )
    );
END;

CREATE TRIGGER profile_delete_complete_apply
AFTER INSERT ON profile_delete_completion_commands
FOR EACH ROW
BEGIN
    UPDATE browser_profiles
    SET status = 'DELETED',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.completed_at_ms
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;
END;

-- A tombstone is terminal. Background workers cannot revive or Sync a
-- profile already freezing its immutable R2 object inventory.
CREATE TRIGGER profile_delete_terminal_status
BEFORE UPDATE OF status ON browser_profiles
FOR EACH ROW
WHEN OLD.status IN ('DELETING','DELETED')
 AND NEW.status <> OLD.status
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_terminal') WHERE
      NOT (OLD.status = 'DELETING' AND NEW.status = 'DELETED'
        AND EXISTS (
            SELECT 1 FROM profile_delete_completion_commands AS command
            WHERE command.tenant_id = OLD.tenant_id
              AND command.profile_id = OLD.profile_id
              AND command.expected_profile_version = OLD.version
        ));
END;

-- A late offline Save must not change the frozen CURRENT pointer or
-- append new generation metadata while exact-key deletion is in flight.
-- The same terminal D1 owner protects both states, including replay.
CREATE TRIGGER profile_delete_current_pointer_immutable
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW WHEN OLD.status IN ('DELETING','DELETED')
  AND NEW.active_generation_id IS NOT OLD.active_generation_id
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_current_pointer_frozen');
END;

CREATE TRIGGER profile_delete_new_generation_forbidden
BEFORE INSERT ON profile_generation_register_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM browser_profiles
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND status IN ('DELETING','DELETED')
)
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_new_generation_forbidden');
END;

CREATE TRIGGER profile_delete_successor_forbidden
BEFORE INSERT ON profile_generation_successor_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM browser_profiles
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND status IN ('DELETING','DELETED')
)
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_successor_forbidden');
END;

CREATE TRIGGER profile_delete_objects_immutable_update
BEFORE UPDATE ON profile_delete_objects
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_inventory_immutable');
END;
CREATE TRIGGER profile_delete_objects_immutable_delete
BEFORE DELETE ON profile_delete_objects
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_inventory_immutable');
END;
CREATE TRIGGER profile_delete_receipts_immutable_update
BEFORE UPDATE ON profile_delete_object_receipts
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_receipt_immutable');
END;
CREATE TRIGGER profile_delete_receipts_immutable_delete
BEFORE DELETE ON profile_delete_object_receipts
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_receipt_immutable');
END;
