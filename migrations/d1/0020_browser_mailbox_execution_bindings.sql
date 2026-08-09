-- Phase 2F explicit BrowserFallback mailbox-to-profile execution binding.
-- Metadata only: no query text, message references, message bodies, credentials,
-- browser profile bytes, cookies or arbitrary provider telemetry belong here.

CREATE TABLE browser_mailbox_execution_bind_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE browser_mailbox_execution_bindings (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    created_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX browser_mailbox_execution_profile_lookup
    ON browser_mailbox_execution_bindings(tenant_id, profile_id, binding_id);

CREATE TRIGGER browser_mailbox_execution_bind_command_validate
BEFORE INSERT ON browser_mailbox_execution_bind_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_bind_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'browser_mailbox_binding_not_executable')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND provider = 'BROWSER_FALLBACK'
          AND status = 'ACTIVE'
          AND execution_status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'browser_mailbox_profile_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
    );
END;

CREATE TRIGGER browser_mailbox_execution_bind_command_update_forbidden
BEFORE UPDATE ON browser_mailbox_execution_bind_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_bind_command_append_only');
END;

CREATE TRIGGER browser_mailbox_execution_bind_command_delete_forbidden
BEFORE DELETE ON browser_mailbox_execution_bind_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_bind_command_append_only');
END;

CREATE TRIGGER browser_mailbox_execution_binding_insert_governed
BEFORE INSERT ON browser_mailbox_execution_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_binding_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_mailbox_execution_bind_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.profile_id = NEW.profile_id
          AND command.command_actor_id = NEW.created_by_actor_id
          AND command.executed_at_ms = NEW.created_at_ms
    );
END;

CREATE TRIGGER browser_mailbox_execution_binding_update_forbidden
BEFORE UPDATE ON browser_mailbox_execution_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_binding_immutable');
END;

CREATE TRIGGER browser_mailbox_execution_binding_delete_forbidden
BEFORE DELETE ON browser_mailbox_execution_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_binding_immutable');
END;

CREATE TRIGGER browser_mailbox_execution_bind_command_apply
AFTER INSERT ON browser_mailbox_execution_bind_commands
FOR EACH ROW
BEGIN
    INSERT INTO browser_mailbox_execution_bindings (
        tenant_id, binding_id, profile_id, created_by_actor_id, created_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.profile_id,
        NEW.command_actor_id, NEW.executed_at_ms
    );
END;
