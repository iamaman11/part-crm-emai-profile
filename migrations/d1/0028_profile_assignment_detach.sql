-- CAP-EXEC P1 / CAP12-I1: extend the existing governed Profile/Client assignment
-- command envelope with standalone detach semantics. Relationship/history storage stays
-- canonical in profile_client_assignments; no second relation table or writer is introduced.
--
-- Compatibility invariant: ASSIGN remains the default operation so a previously accepted
-- worker that omits the new column continues to execute the exact legacy attach/reassign path
-- after this migration. DETACH is an additive command mode only.

ALTER TABLE profile_assignment_commands
    ADD COLUMN operation TEXT NOT NULL DEFAULT 'ASSIGN'
        CHECK(operation IN ('ASSIGN', 'DETACH'));

-- 0009 owns the current mutation error taxonomy. Recreate the same validator with
-- operation-specific relationship checks: ASSIGN still requires an active target Client;
-- DETACH instead proves the exact currently-active assignment identity and Client identity.
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
          AND status <> 'DELETED'
    );

    SELECT RAISE(ABORT, 'profile_assignment_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND status <> 'DELETED'
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

-- 0015 adds the current assignment-history guards and a stricter command precondition.
-- Preserve its time-regression protection for both operations while limiting the
-- same-client conflict to ASSIGN. A DETACH necessarily names the current Client.
DROP TRIGGER profile_assignment_phase2c_validate;
CREATE TRIGGER profile_assignment_phase2c_validate
BEFORE INSERT ON profile_assignment_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_same_client')
    WHERE NEW.operation = 'ASSIGN'
      AND EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND client_id = NEW.client_id
          AND closed_at_ms IS NULL
    );

    SELECT RAISE(ABORT, 'profile_assignment_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND closed_at_ms IS NULL
          AND assigned_at_ms > NEW.executed_at_ms
    );
END;

-- The 0015 history insert guard predates the operation discriminator. Rebind it explicitly
-- to ASSIGN so DETACH can only authorize closing the proven active row and can never become
-- authority for creating assignment history, even if a future schema change relaxes another
-- identity constraint.
DROP TRIGGER profile_assignment_history_insert_guard;
CREATE TRIGGER profile_assignment_history_insert_guard
BEFORE INSERT ON profile_client_assignments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_assignment_commands
        WHERE tenant_id = NEW.tenant_id
          AND assignment_id = NEW.assignment_id
          AND profile_id = NEW.profile_id
          AND client_id = NEW.client_id
          AND command_actor_id = NEW.assigned_by_actor_id
          AND executed_at_ms = NEW.assigned_at_ms
          AND trim(reason) = trim(NEW.reason)
          AND operation = 'ASSIGN'
    );
END;

-- Rebind the 0015 history close guard to the operation discriminator and current Profile
-- version. ASSIGN must still be able to close the previous active row before inserting a
-- successor whose identity is necessarily different. DETACH is authority only for the exact
-- server-restored assignment/client identity. The expected Profile version must still be
-- current while the close is executing; the command's version bump therefore expires that
-- history-write authority immediately after the successful mutation.
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
    );
END;

-- Replace the legacy unconditional apply trigger with operation-specific branches. ASSIGN
-- is byte-for-byte equivalent in business effect: close the previous active assignment,
-- insert the next history row, then increment Profile version exactly once.
DROP TRIGGER profile_assignment_command_apply;

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
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

-- DETACH closes only the exact active assignment proven by the BEFORE trigger. It leaves
-- Client and Profile rows intact, inserts no replacement assignment, and increments Profile
-- version exactly once. The operation-aware history update guard independently enforces that
-- DETACH cannot authorize closing any other relationship-history row or reuse a completed
-- command after its expected Profile version has been consumed.
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
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

-- A Client with a current Profile assignment cannot be archived. Otherwise the canonical
-- relationship owner could be left with an active assignment whose Client no longer satisfies
-- the Active invariant required by attach/reassign/detach reconstruction. Resolve the
-- relationship first by DETACH or atomic reassign; then the existing Client archive path works
-- unchanged. This extends the existing 0014 lifecycle admission owner rather than adding a
-- second lifecycle writer.
DROP TRIGGER client_lifecycle_command_validate;
CREATE TRIGGER client_lifecycle_command_validate
BEFORE INSERT ON client_lifecycle_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_lifecycle_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'client_lifecycle_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
          AND version = NEW.expected_client_version
    );

    SELECT RAISE(ABORT, 'client_lifecycle_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND updated_at_ms > NEW.executed_at_ms
    );

    SELECT RAISE(ABORT, 'client_archive_active_assignment_conflict')
    WHERE NEW.operation = 'ARCHIVE'
      AND EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND closed_at_ms IS NULL
    );
END;
