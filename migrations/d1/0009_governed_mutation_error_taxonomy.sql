-- Make legacy governed command failures semantically classifiable without
-- duplicating aggregate existence/state checks in the Worker. Missing or retired
-- resources stay disclosure-neutral; a live aggregate with the wrong expected
-- version is a genuine optimistic-concurrency conflict.

DROP TRIGGER owner_transfer_command_validate;
CREATE TRIGGER owner_transfer_command_validate
BEFORE INSERT ON owner_transfer_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'owner_transfer_current_owner_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.current_owner_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
          AND version = NEW.current_owner_version
    );
    SELECT RAISE(ABORT, 'owner_transfer_successor_mismatch')
    WHERE NEW.current_owner_actor_id = NEW.next_owner_actor_id
       OR NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.next_owner_actor_id
          AND role = 'MEMBER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'owner_transfer_successor_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.next_owner_actor_id
          AND role = 'MEMBER'
          AND status = 'ACTIVE'
          AND version <> NEW.next_owner_version
    );
    SELECT RAISE(ABORT, 'owner_transfer_owner_invariant')
    WHERE (
        SELECT COUNT(*) FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    ) <> 1;
END;

DROP TRIGGER membership_status_command_validate;
CREATE TRIGGER membership_status_command_validate
BEFORE INSERT ON membership_status_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'membership_status_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'membership_status_target_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
    );
    SELECT RAISE(ABORT, 'membership_status_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND version <> NEW.expected_version
    );
    SELECT RAISE(ABORT, 'membership_status_invalid_transition')
    WHERE EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND status = 'REVOKED'
          AND NEW.next_status <> 'REVOKED'
    );
    SELECT RAISE(ABORT, 'last_active_owner')
    WHERE NEW.next_status <> 'ACTIVE'
      AND EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
      )
      AND (
        SELECT COUNT(*) FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
      ) <= 1;
END;

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
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
    );
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
          AND version <> NEW.expected_profile_version
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

DROP TRIGGER client_grant_command_validate;
CREATE TRIGGER client_grant_command_validate
BEFORE INSERT ON client_grant_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_grant_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_grant_target_not_active_member')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND role = 'MEMBER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_grant_client_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status <> 'MERGED'
    );
    SELECT RAISE(ABORT, 'client_grant_version_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status <> 'MERGED'
          AND version <> NEW.expected_client_version
    );
    SELECT RAISE(ABORT, 'client_grant_missing')
    WHERE NEW.operation = 'REVOKE'
      AND NOT EXISTS (
        SELECT 1 FROM client_grants
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND client_id = NEW.client_id
    );
END;
