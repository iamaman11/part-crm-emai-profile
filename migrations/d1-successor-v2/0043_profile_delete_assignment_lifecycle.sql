-- #849: repair the existing Profile/Delete and Profile/Client lifecycle.
-- No new relation table, coordinator or deletion service is introduced.
-- D1's original owner-authorized DELETE command is the sole authority.
--
-- 1. Assign/Detach cannot mutate a profile after DELETE freezes its version.
-- 2. DELETE closes the active relationship and freezes R2 keys atomically.
-- 3. Previously completed test-profile Deletes are reconciled only when an
--    exact existing completion command proves the governed transition.

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
          AND version <> NEW.expected_profile_version
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
         AND profile.version = command.expected_profile_version
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

DROP TRIGGER profile_delete_command_apply;
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

    -- Include every permitted R2 key, even when the immutable upload
    -- completed but the D1 generation commit was interrupted. A signed URL
    -- can otherwise leave an unreachable/orphan object after whole-profile
    -- Delete, especially for a new DRAFT with zero registered generations.
    INSERT INTO profile_delete_objects (
        tenant_id, profile_id, generation_id, object_key,
        metadata_digest, container_digest
    )
    SELECT tenant_id, profile_id, generation_id, object_key,
           metadata_digest, container_digest
    FROM profile_generation_upload_reservations
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id
    ON CONFLICT(tenant_id, profile_id, generation_id) DO NOTHING;

    -- Close the existing client relationship in the same D1 statement as
    -- the frozen inventory and DELETING transition. Keep history, not an
    -- orphaned active assignment. CAS/version guards serialize Assign/Detach.
    UPDATE profile_client_assignments
    SET closed_at_ms = NEW.requested_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id
      AND closed_at_ms IS NULL;

    UPDATE browser_profiles
    SET status = 'DELETING',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.requested_at_ms
    WHERE tenant_id = NEW.tenant_id AND profile_id = NEW.profile_id;
END;

-- Reconcile completed history already observed in staging. The history guard
-- above recognizes the exact immutable completion receipt as authority.
UPDATE profile_client_assignments AS assignment
SET closed_at_ms = (
    SELECT completion.completed_at_ms
    FROM profile_delete_completion_commands AS completion
    WHERE completion.tenant_id = assignment.tenant_id
      AND completion.profile_id = assignment.profile_id
)
WHERE assignment.closed_at_ms IS NULL
  AND EXISTS (
      SELECT 1 FROM browser_profiles AS profile
      WHERE profile.tenant_id = assignment.tenant_id
        AND profile.profile_id = assignment.profile_id
        AND profile.status = 'DELETED'
  )
  AND EXISTS (
      SELECT 1 FROM profile_delete_completion_commands AS completion
      WHERE completion.tenant_id = assignment.tenant_id
        AND completion.profile_id = assignment.profile_id
        AND completion.completed_at_ms >= assignment.assigned_at_ms
  );

-- A tenant owner's destructive Delete revokes cloud admission by changing the
-- existing browser_profiles status/version. It does NOT acquire, reset or
-- claim any browser lease. A stale DO projection, outstanding device job or
-- unredeemed launch claim is therefore not a veto on revocation. Their
-- independent operations fail closed against DELETING/DELETED.
-- In-flight signed R2 PUTs remain protected by the original 360-second
-- object deletion/completion fence, which is deliberately unchanged.
DROP TRIGGER profile_delete_admit;
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
    -- A signed candidate is immutable even before it is committed into
    -- profile_generations. A mismatched descriptor may not be hidden by
    -- INSERT OR IGNORE when the frozen inventory is assembled.
    SELECT RAISE(ABORT, 'profile_delete_upload_registry_conflict')
    WHERE EXISTS (
        SELECT 1 FROM profile_generation_upload_reservations AS issued
        JOIN profile_generations AS registered
          ON registered.tenant_id = issued.tenant_id
         AND registered.profile_id = issued.profile_id
         AND registered.generation_id = issued.generation_id
        WHERE issued.tenant_id = NEW.tenant_id
          AND issued.profile_id = NEW.profile_id
          AND (issued.object_key <> registered.object_key
            OR issued.metadata_digest <> registered.metadata_digest
            OR issued.container_digest <> registered.container_digest)
    );
END;

DROP TRIGGER profile_delete_complete_admit;
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
    SELECT RAISE(ABORT, 'profile_delete_presigned_put_not_expired')
    WHERE EXISTS (
        SELECT 1 FROM profile_delete_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.profile_id = NEW.profile_id
          AND NEW.completed_at_ms < MAX(
              command.requested_at_ms + 360000,
              COALESCE((
                  SELECT MAX(reservation.expires_at_ms)
                  FROM profile_generation_upload_reservations AS reservation
                  WHERE reservation.tenant_id = command.tenant_id
                    AND reservation.profile_id = command.profile_id
              ), 0)
          )
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
END;

-- The original D1 launch claim table already holds all short-lived claims.
-- Fence a claim issued concurrently with owner Delete in this same database,
-- without a new claim registry, cancellation service or coordinator.
CREATE TRIGGER profile_launch_claim_delete_fence
BEFORE INSERT ON profile_launch_claims
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM browser_profiles AS profile
    WHERE profile.tenant_id = NEW.tenant_id
      AND profile.profile_id = NEW.profile_id
      AND profile.status IN ('DELETING','DELETED')
)
BEGIN
    SELECT RAISE(ABORT, 'profile_launch_profile_deleted');
END;
