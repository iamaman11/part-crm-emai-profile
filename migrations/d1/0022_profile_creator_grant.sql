-- Pre-2J A2: Profile creation is ACTIVE-member self-service.
--
-- Worker ingress already resolves only ACTIVE memberships, but the governed D1
-- command must re-check that invariant inside the same transaction so a stale
-- actor context cannot leave a Profile/grant half-state. Owner-only policy remains
-- on administrative assignment/grant/revoke commands; only Profile creation changes.

DROP TRIGGER profile_create_command_validate;

CREATE TRIGGER profile_create_command_validate
BEFORE INSERT ON profile_create_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_create_membership_not_active')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND status = 'ACTIVE'
    );
END;
