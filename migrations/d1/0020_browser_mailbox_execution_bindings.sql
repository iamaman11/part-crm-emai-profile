-- Phase 2F explicit BrowserFallback mailbox-to-profile execution binding.
-- Metadata only: no query text, message references, message bodies, credentials,
-- browser profile bytes, cookies or arbitrary provider telemetry belong here.

ALTER TABLE mailbox_binding_create_commands
ADD COLUMN browser_profile_id TEXT;

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

CREATE TRIGGER browser_mailbox_execution_binding_insert_governed
BEFORE INSERT ON browser_mailbox_execution_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'browser_mailbox_execution_binding_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_binding_create_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.provider = 'BROWSER_FALLBACK'
          AND command.browser_profile_id = NEW.profile_id
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

DROP TRIGGER mailbox_binding_create_command_validate;
DROP TRIGGER mailbox_binding_create_command_apply;

CREATE TRIGGER mailbox_binding_create_command_validate
BEFORE INSERT ON mailbox_binding_create_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_create_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_create_provider_invalid')
    WHERE NEW.provider NOT IN ('GMAIL_API', 'IMAP', 'BROWSER_FALLBACK');
    SELECT RAISE(ABORT, 'mailbox_binding_create_secret_handle_invalid')
    WHERE length(NEW.secret_handle) NOT BETWEEN 8 AND 96
       OR NEW.secret_handle GLOB '*[^A-Za-z0-9_-]*';
    SELECT RAISE(ABORT, 'browser_mailbox_profile_required')
    WHERE NEW.provider = 'BROWSER_FALLBACK'
      AND NEW.browser_profile_id IS NULL;
    SELECT RAISE(ABORT, 'browser_mailbox_profile_forbidden')
    WHERE NEW.provider <> 'BROWSER_FALLBACK'
      AND NEW.browser_profile_id IS NOT NULL;
    SELECT RAISE(ABORT, 'browser_mailbox_profile_missing')
    WHERE NEW.provider = 'BROWSER_FALLBACK'
      AND NOT EXISTS (
          SELECT 1 FROM browser_profiles
          WHERE tenant_id = NEW.tenant_id
            AND profile_id = NEW.browser_profile_id
      );
END;

CREATE TRIGGER mailbox_binding_create_command_apply
AFTER INSERT ON mailbox_binding_create_commands
FOR EACH ROW
BEGIN
    INSERT INTO mailbox_bindings (
        tenant_id, binding_id, provider, secret_handle, status, version,
        created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.provider, NEW.secret_handle, 'ACTIVE', 1,
        NEW.command_actor_id, NEW.command_actor_id, NEW.executed_at_ms, NEW.executed_at_ms
    );

    INSERT INTO browser_mailbox_execution_bindings (
        tenant_id, binding_id, profile_id, created_by_actor_id, created_at_ms
    )
    SELECT
        NEW.tenant_id, NEW.binding_id, NEW.browser_profile_id,
        NEW.command_actor_id, NEW.executed_at_ms
    WHERE NEW.provider = 'BROWSER_FALLBACK';
END;
