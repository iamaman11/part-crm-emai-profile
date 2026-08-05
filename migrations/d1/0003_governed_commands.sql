-- Repository Step 4: governed command records make optimistic preconditions
-- transaction-fatal before idempotency, audit and outbox records can commit.

CREATE TABLE owner_transfer_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    current_owner_actor_id TEXT NOT NULL,
    next_owner_actor_id TEXT NOT NULL,
    current_owner_version INTEGER NOT NULL CHECK(current_owner_version >= 1),
    next_owner_version INTEGER NOT NULL CHECK(next_owner_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, current_owner_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, next_owner_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

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
          AND version = NEW.next_owner_version
    );
    SELECT RAISE(ABORT, 'owner_transfer_owner_invariant')
    WHERE (
        SELECT COUNT(*) FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    ) <> 1;
END;

CREATE TRIGGER owner_transfer_command_apply
AFTER INSERT ON owner_transfer_commands
FOR EACH ROW
BEGIN
    UPDATE memberships
    SET role = 'MEMBER', version = version + 1, updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.current_owner_actor_id;
    UPDATE memberships
    SET role = 'TENANT_OWNER', version = version + 1, updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.next_owner_actor_id;
END;

CREATE TABLE membership_status_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    expected_version INTEGER NOT NULL CHECK(expected_version >= 1),
    next_status TEXT NOT NULL CHECK(next_status IN ('ACTIVE', 'SUSPENDED', 'REVOKED')),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

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
    SELECT RAISE(ABORT, 'membership_status_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.target_actor_id
          AND version = NEW.expected_version
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

CREATE TRIGGER membership_status_command_apply
AFTER INSERT ON membership_status_commands
FOR EACH ROW
BEGIN
    UPDATE memberships
    SET status = NEW.next_status,
        version = version + 1,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id;
END;

CREATE TABLE invitation_create_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    invitation_id TEXT NOT NULL,
    invited_contact_hmac TEXT NOT NULL CHECK(length(invited_contact_hmac) BETWEEN 16 AND 256),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
    expected_tenant_version INTEGER NOT NULL CHECK(expected_tenant_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER invitation_create_command_validate
BEFORE INSERT ON invitation_create_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'invitation_create_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'invitation_create_tenant_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM tenants
        WHERE tenant_id = NEW.tenant_id
          AND version = NEW.expected_tenant_version
    );
    SELECT RAISE(ABORT, 'invitation_create_expired')
    WHERE NEW.expires_at_ms <= NEW.executed_at_ms;
END;

CREATE TRIGGER invitation_create_command_apply
AFTER INSERT ON invitation_create_commands
FOR EACH ROW
BEGIN
    UPDATE tenants
    SET version = version + 1, updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id;
    INSERT INTO invitations (
        tenant_id, invitation_id, invited_contact_hmac, intended_role,
        status, expires_at_ms, created_by_actor_id, created_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.invitation_id, NEW.invited_contact_hmac, 'MEMBER',
        'PENDING', NEW.expires_at_ms, NEW.command_actor_id, NEW.executed_at_ms
    );
END;

CREATE TABLE profile_create_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_create_command_validate
BEFORE INSERT ON profile_create_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_create_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
END;

CREATE TRIGGER profile_create_command_apply
AFTER INSERT ON profile_create_commands
FOR EACH ROW
BEGIN
    INSERT INTO browser_profiles (
        tenant_id, profile_id, status, active_generation_id, version,
        created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.profile_id, 'DRAFT', NULL, 1,
        NEW.command_actor_id, NEW.command_actor_id,
        NEW.executed_at_ms, NEW.executed_at_ms
    );
END;

CREATE TABLE profile_assignment_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    assignment_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version >= 1),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

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
    SELECT RAISE(ABORT, 'profile_assignment_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND version = NEW.expected_profile_version
          AND status <> 'DELETED'
    );
    SELECT RAISE(ABORT, 'profile_assignment_client_not_active')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
    );
END;

CREATE TRIGGER profile_assignment_command_apply
AFTER INSERT ON profile_assignment_commands
FOR EACH ROW
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

CREATE TABLE profile_grant_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('GRANT', 'REVOKE')),
    role TEXT NOT NULL CHECK(role IN ('PROFILE_VIEWER', 'PROFILE_OPERATOR')),
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version >= 1),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT
) STRICT;

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
    SELECT RAISE(ABORT, 'profile_grant_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND version = NEW.expected_profile_version
          AND status <> 'DELETED'
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

CREATE TRIGGER profile_grant_command_apply_grant
AFTER INSERT ON profile_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'GRANT'
BEGIN
    INSERT INTO profile_grants (
        tenant_id, actor_id, profile_id, role,
        granted_by_actor_id, reason, created_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.target_actor_id, NEW.profile_id, NEW.role,
        NEW.command_actor_id, NEW.reason, NEW.executed_at_ms
    )
    ON CONFLICT (tenant_id, actor_id, profile_id) DO UPDATE SET
        role = excluded.role,
        granted_by_actor_id = excluded.granted_by_actor_id,
        reason = excluded.reason,
        created_at_ms = excluded.created_at_ms;
    UPDATE browser_profiles
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

CREATE TRIGGER profile_grant_command_apply_revoke
AFTER INSERT ON profile_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'REVOKE'
BEGIN
    DELETE FROM profile_grants
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id
      AND profile_id = NEW.profile_id;
    UPDATE browser_profiles
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND profile_id = NEW.profile_id;
END;

CREATE TABLE client_grant_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('GRANT', 'REVOKE')),
    role TEXT NOT NULL CHECK(role IN ('CLIENT_VIEWER', 'CLIENT_EDITOR')),
    expected_client_version INTEGER NOT NULL CHECK(expected_client_version >= 1),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

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
    SELECT RAISE(ABORT, 'client_grant_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND version = NEW.expected_client_version
          AND status <> 'MERGED'
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

CREATE TRIGGER client_grant_command_apply_grant
AFTER INSERT ON client_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'GRANT'
BEGIN
    INSERT INTO client_grants (
        tenant_id, actor_id, client_id, role,
        granted_by_actor_id, reason, created_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.target_actor_id, NEW.client_id, NEW.role,
        NEW.command_actor_id, NEW.reason, NEW.executed_at_ms
    )
    ON CONFLICT (tenant_id, actor_id, client_id) DO UPDATE SET
        role = excluded.role,
        granted_by_actor_id = excluded.granted_by_actor_id,
        reason = excluded.reason,
        created_at_ms = excluded.created_at_ms;
    UPDATE clients
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id;
END;

CREATE TRIGGER client_grant_command_apply_revoke
AFTER INSERT ON client_grant_commands
FOR EACH ROW
WHEN NEW.operation = 'REVOKE'
BEGIN
    DELETE FROM client_grants
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.target_actor_id
      AND client_id = NEW.client_id;
    UPDATE clients
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id;
END;
