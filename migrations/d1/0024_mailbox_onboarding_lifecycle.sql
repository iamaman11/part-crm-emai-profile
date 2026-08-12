-- Pre-2J Batch C1: provider-neutral mailbox onboarding lifecycle authority.
-- This schema stores only an opaque credential handle/reference. Raw passwords,
-- OAuth tokens/codes, PKCE material and provider SDK objects do not belong in D1.

CREATE TABLE mailbox_onboarding_state (
    tenant_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP')),
    lifecycle_status TEXT NOT NULL CHECK(lifecycle_status IN (
        'PENDING', 'ACTIVE', 'REAUTH_REQUIRED', 'DISABLED', 'CONFIG_ERROR'
    )),
    credential_handle TEXT,
    status_metadata TEXT CHECK(
        status_metadata IS NULL OR (
            length(status_metadata) BETWEEN 1 AND 128
            AND status_metadata NOT GLOB '*[^A-Za-z0-9_.:/-]*'
        )
    ),
    version INTEGER NOT NULL CHECK(version >= 1),
    updated_by_actor_id TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    PRIMARY KEY (tenant_id, onboarding_id),
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX mailbox_onboarding_state_status_lookup
    ON mailbox_onboarding_state(tenant_id, lifecycle_status, provider, onboarding_id);

CREATE TABLE mailbox_onboarding_history (
    tenant_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    operation TEXT NOT NULL CHECK(operation IN (
        'START', 'ACTIVATE', 'REQUIRE_REAUTH', 'DISABLE', 'CONFIG_ERROR'
    )),
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP')),
    previous_status TEXT CHECK(previous_status IS NULL OR previous_status IN (
        'PENDING', 'ACTIVE', 'REAUTH_REQUIRED', 'DISABLED', 'CONFIG_ERROR'
    )),
    next_status TEXT NOT NULL CHECK(next_status IN (
        'PENDING', 'ACTIVE', 'REAUTH_REQUIRED', 'DISABLED', 'CONFIG_ERROR'
    )),
    previous_credential_handle TEXT,
    next_credential_handle TEXT,
    status_metadata TEXT CHECK(
        status_metadata IS NULL OR (
            length(status_metadata) BETWEEN 1 AND 128
            AND status_metadata NOT GLOB '*[^A-Za-z0-9_.:/-]*'
        )
    ),
    changed_by_actor_id TEXT NOT NULL,
    changed_at_ms INTEGER NOT NULL CHECK(changed_at_ms >= 0),
    PRIMARY KEY (tenant_id, onboarding_id, version),
    FOREIGN KEY (tenant_id, changed_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_onboarding_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP')),
    expected_version INTEGER NOT NULL CHECK(expected_version >= 0),
    next_version INTEGER NOT NULL CHECK(next_version >= 1),
    operation TEXT NOT NULL CHECK(operation IN (
        'START', 'ACTIVATE', 'REQUIRE_REAUTH', 'DISABLE', 'CONFIG_ERROR'
    )),
    previous_status TEXT CHECK(previous_status IS NULL OR previous_status IN (
        'PENDING', 'ACTIVE', 'REAUTH_REQUIRED', 'DISABLED', 'CONFIG_ERROR'
    )),
    next_status TEXT NOT NULL CHECK(next_status IN (
        'PENDING', 'ACTIVE', 'REAUTH_REQUIRED', 'DISABLED', 'CONFIG_ERROR'
    )),
    previous_credential_handle TEXT,
    next_credential_handle TEXT,
    status_metadata TEXT CHECK(
        status_metadata IS NULL OR (
            length(status_metadata) BETWEEN 1 AND 128
            AND status_metadata NOT GLOB '*[^A-Za-z0-9_.:/-]*'
        )
    ),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_onboarding_command_validate
BEFORE INSERT ON mailbox_onboarding_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_onboarding_next_version_invalid')
    WHERE NEW.expected_version >= 9223372036854775807
       OR NEW.next_version <> NEW.expected_version + 1;

    SELECT RAISE(ABORT, 'mailbox_onboarding_start_conflict')
    WHERE NEW.operation = 'START'
      AND EXISTS (
          SELECT 1 FROM mailbox_onboarding_state
          WHERE tenant_id = NEW.tenant_id
            AND onboarding_id = NEW.onboarding_id
      );

    SELECT RAISE(ABORT, 'mailbox_onboarding_not_found')
    WHERE NEW.operation <> 'START'
      AND NOT EXISTS (
          SELECT 1 FROM mailbox_onboarding_state
          WHERE tenant_id = NEW.tenant_id
            AND onboarding_id = NEW.onboarding_id
      );

    SELECT RAISE(ABORT, 'mailbox_onboarding_version_mismatch')
    WHERE NEW.operation <> 'START'
      AND COALESCE((
          SELECT version FROM mailbox_onboarding_state
          WHERE tenant_id = NEW.tenant_id
            AND onboarding_id = NEW.onboarding_id
      ), -1) <> NEW.expected_version;

    SELECT RAISE(ABORT, 'mailbox_onboarding_provider_mismatch')
    WHERE NEW.operation <> 'START'
      AND COALESCE((
          SELECT provider FROM mailbox_onboarding_state
          WHERE tenant_id = NEW.tenant_id
            AND onboarding_id = NEW.onboarding_id
      ), '') <> NEW.provider;

    SELECT RAISE(ABORT, 'mailbox_onboarding_previous_mismatch')
    WHERE NEW.operation <> 'START'
      AND (
          COALESCE((
              SELECT lifecycle_status FROM mailbox_onboarding_state
              WHERE tenant_id = NEW.tenant_id
                AND onboarding_id = NEW.onboarding_id
          ), '') <> COALESCE(NEW.previous_status, '')
          OR COALESCE((
              SELECT credential_handle FROM mailbox_onboarding_state
              WHERE tenant_id = NEW.tenant_id
                AND onboarding_id = NEW.onboarding_id
          ), '') <> COALESCE(NEW.previous_credential_handle, '')
      );

    SELECT RAISE(ABORT, 'mailbox_onboarding_invalid_transition')
    WHERE (NEW.operation = 'START' AND NOT (
              NEW.expected_version = 0
              AND NEW.next_version = 1
              AND NEW.previous_status IS NULL
              AND NEW.next_status = 'PENDING'
              AND NEW.previous_credential_handle IS NULL
              AND NEW.next_credential_handle IS NULL
          ))
       OR (NEW.operation = 'ACTIVATE' AND NOT (
              NEW.previous_status IN ('PENDING', 'REAUTH_REQUIRED')
              AND NEW.next_status = 'ACTIVE'
              AND NEW.next_credential_handle IS NOT NULL
          ))
       OR (NEW.operation = 'REQUIRE_REAUTH' AND NOT (
              NEW.previous_status = 'ACTIVE'
              AND NEW.next_status = 'REAUTH_REQUIRED'
              AND NEW.next_credential_handle IS NEW.previous_credential_handle
          ))
       OR (NEW.operation = 'DISABLE' AND NOT (
              NEW.previous_status IN ('PENDING', 'ACTIVE', 'REAUTH_REQUIRED')
              AND NEW.next_status = 'DISABLED'
              AND NEW.next_credential_handle IS NEW.previous_credential_handle
          ))
       OR (NEW.operation = 'CONFIG_ERROR' AND NOT (
              NEW.previous_status IN ('PENDING', 'ACTIVE', 'REAUTH_REQUIRED')
              AND NEW.next_status = 'CONFIG_ERROR'
              AND NEW.next_credential_handle IS NEW.previous_credential_handle
          ));

    SELECT RAISE(ABORT, 'mailbox_onboarding_time_regression')
    WHERE NEW.operation <> 'START'
      AND EXISTS (
          SELECT 1 FROM mailbox_onboarding_state
          WHERE tenant_id = NEW.tenant_id
            AND onboarding_id = NEW.onboarding_id
            AND NEW.executed_at_ms < updated_at_ms
      );
END;

CREATE TRIGGER mailbox_onboarding_command_apply
AFTER INSERT ON mailbox_onboarding_commands
FOR EACH ROW
BEGIN
    INSERT INTO mailbox_onboarding_state (
        tenant_id, onboarding_id, provider, lifecycle_status, credential_handle,
        status_metadata, version, updated_by_actor_id, updated_at_ms
    )
    SELECT
        NEW.tenant_id, NEW.onboarding_id, NEW.provider, NEW.next_status,
        NEW.next_credential_handle, NEW.status_metadata, NEW.next_version,
        NEW.command_actor_id, NEW.executed_at_ms
    WHERE NEW.operation = 'START';

    UPDATE mailbox_onboarding_state
    SET lifecycle_status = NEW.next_status,
        credential_handle = NEW.next_credential_handle,
        status_metadata = NEW.status_metadata,
        version = NEW.next_version,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND onboarding_id = NEW.onboarding_id
      AND NEW.operation <> 'START';

    INSERT INTO mailbox_onboarding_history (
        tenant_id, onboarding_id, version, operation, provider,
        previous_status, next_status, previous_credential_handle,
        next_credential_handle, status_metadata, changed_by_actor_id, changed_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.onboarding_id, NEW.next_version, NEW.operation, NEW.provider,
        NEW.previous_status, NEW.next_status, NEW.previous_credential_handle,
        NEW.next_credential_handle, NEW.status_metadata, NEW.command_actor_id,
        NEW.executed_at_ms
    );
END;

CREATE TRIGGER mailbox_onboarding_state_insert_governed
BEFORE INSERT ON mailbox_onboarding_state
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_onboarding_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.onboarding_id = NEW.onboarding_id
      AND command.operation = 'START'
      AND command.provider = NEW.provider
      AND command.next_status = NEW.lifecycle_status
      AND command.next_credential_handle IS NEW.credential_handle
      AND command.status_metadata IS NEW.status_metadata
      AND command.next_version = NEW.version
      AND command.command_actor_id = NEW.updated_by_actor_id
      AND command.executed_at_ms = NEW.updated_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_state_not_governed');
END;

CREATE TRIGGER mailbox_onboarding_state_update_governed
BEFORE UPDATE ON mailbox_onboarding_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_state_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.onboarding_id <> OLD.onboarding_id
       OR NEW.provider <> OLD.provider;

    SELECT RAISE(ABORT, 'mailbox_onboarding_state_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_onboarding_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.onboarding_id = OLD.onboarding_id
          AND command.provider = NEW.provider
          AND command.expected_version = OLD.version
          AND command.next_version = NEW.version
          AND command.next_status = NEW.lifecycle_status
          AND command.next_credential_handle IS NEW.credential_handle
          AND command.status_metadata IS NEW.status_metadata
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER mailbox_onboarding_state_delete_forbidden
BEFORE DELETE ON mailbox_onboarding_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_state_delete_forbidden');
END;

CREATE TRIGGER mailbox_onboarding_history_insert_governed
BEFORE INSERT ON mailbox_onboarding_history
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_onboarding_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.onboarding_id = NEW.onboarding_id
      AND command.next_version = NEW.version
      AND command.operation = NEW.operation
      AND command.provider = NEW.provider
      AND command.previous_status IS NEW.previous_status
      AND command.next_status = NEW.next_status
      AND command.previous_credential_handle IS NEW.previous_credential_handle
      AND command.next_credential_handle IS NEW.next_credential_handle
      AND command.status_metadata IS NEW.status_metadata
      AND command.command_actor_id = NEW.changed_by_actor_id
      AND command.executed_at_ms = NEW.changed_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_history_not_governed');
END;

CREATE TRIGGER mailbox_onboarding_history_update_forbidden
BEFORE UPDATE ON mailbox_onboarding_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_history_immutable');
END;

CREATE TRIGGER mailbox_onboarding_history_delete_forbidden
BEFORE DELETE ON mailbox_onboarding_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_history_immutable');
END;

CREATE TRIGGER mailbox_onboarding_commands_update_forbidden
BEFORE UPDATE ON mailbox_onboarding_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_command_immutable');
END;

CREATE TRIGGER mailbox_onboarding_commands_delete_forbidden
BEFORE DELETE ON mailbox_onboarding_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_onboarding_command_immutable');
END;
