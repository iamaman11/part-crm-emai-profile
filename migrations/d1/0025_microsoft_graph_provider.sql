-- Pre-2J C3G: durable Microsoft Graph provider discriminator.
--
-- SQLite cannot widen an existing CHECK constraint in place. Rebuild the complete
-- inbound mailbox FK graph with deferred enforcement so accepted Gmail/IMAP/
-- BrowserFallback rows survive intact while MICROSOFT_GRAPH becomes first-class.
-- No credentials, OAuth tokens/codes, PKCE material or message content are added.

PRAGMA defer_foreign_keys = ON;

CREATE TABLE mailbox_bindings_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL
        CHECK(length(binding_id) BETWEEN 8 AND 96)
        CHECK(binding_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    provider TEXT NOT NULL CHECK(provider IN (
        'GMAIL_API', 'IMAP', 'BROWSER_FALLBACK', 'MICROSOFT_GRAPH'
    )),
    secret_handle TEXT NOT NULL
        CHECK(length(secret_handle) BETWEEN 8 AND 96)
        CHECK(secret_handle NOT GLOB '*[^A-Za-z0-9_-]*'),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    execution_status TEXT NOT NULL DEFAULT 'ACTIVE'
        CHECK(execution_status IN ('ACTIVE', 'AUTH_REQUIRED', 'SUSPENDED')),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_jobs_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL
        CHECK(length(job_id) BETWEEN 8 AND 96)
        CHECK(job_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    cursor TEXT CHECK(cursor IS NULL OR length(cursor) <= 512),
    status TEXT NOT NULL CHECK(status IN ('PENDING', 'RUNNING', 'RETRY_PENDING', 'SUCCEEDED', 'FAILED')),
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 0 AND 10),
    max_attempts INTEGER NOT NULL CHECK(max_attempts BETWEEN 1 AND 10),
    next_run_at_ms INTEGER NOT NULL CHECK(next_run_at_ms >= 0),
    provider_status TEXT
        CHECK(provider_status IS NULL OR (
            length(provider_status) BETWEEN 1 AND 64
            AND provider_status NOT GLOB '*[^A-Za-z0-9_.-]*'
        )),
    bounded_item_count INTEGER NOT NULL DEFAULT 0 CHECK(bounded_item_count BETWEEN 0 AND 10000),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    lifecycle_status TEXT NOT NULL DEFAULT 'SCHEDULED'
        CHECK(lifecycle_status IN (
            'SCHEDULED', 'QUEUED', 'RUNNING', 'RETRY_PENDING',
            'AUTH_REQUIRED', 'SUSPENDED', 'SUCCEEDED', 'FAILED'
        )),
    PRIMARY KEY (tenant_id, binding_id, job_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings_c3g(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_job_queue_dispatches_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    published_at_ms INTEGER NOT NULL CHECK(published_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id, job_id, expected_job_version),
    FOREIGN KEY (tenant_id, binding_id, job_id)
        REFERENCES mailbox_jobs_c3g(tenant_id, binding_id, job_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_job_execution_leases_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    fence INTEGER NOT NULL CHECK(fence BETWEEN 1 AND 9223372036854775807),
    lease_state TEXT NOT NULL CHECK(lease_state IN ('ACTIVE', 'COMPLETED')),
    claimed_at_ms INTEGER NOT NULL CHECK(claimed_at_ms >= 0),
    lease_expires_at_ms INTEGER NOT NULL CHECK(lease_expires_at_ms > claimed_at_ms),
    completed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, binding_id, job_id, expected_job_version),
    FOREIGN KEY (tenant_id, binding_id, job_id)
        REFERENCES mailbox_jobs_c3g(tenant_id, binding_id, job_id) ON DELETE RESTRICT,
    CHECK(
        (lease_state = 'ACTIVE' AND completed_at_ms IS NULL)
        OR
        (lease_state = 'COMPLETED' AND completed_at_ms IS NOT NULL
            AND completed_at_ms >= claimed_at_ms)
    )
) STRICT;

CREATE TABLE browser_mailbox_execution_bindings_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    created_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings_c3g(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_client_association_state_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    client_id TEXT,
    version INTEGER NOT NULL CHECK(version >= 1),
    updated_by_actor_id TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings_c3g(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_client_association_history_c3g (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    operation TEXT NOT NULL CHECK(operation IN ('BIND', 'REBIND', 'UNBIND')),
    previous_client_id TEXT,
    next_client_id TEXT,
    changed_by_actor_id TEXT NOT NULL,
    changed_at_ms INTEGER NOT NULL CHECK(changed_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id, version),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings_c3g(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, previous_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, next_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, changed_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_client_association_commands_c3g (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    expected_relationship_version INTEGER NOT NULL CHECK(expected_relationship_version >= 0),
    next_relationship_version INTEGER NOT NULL CHECK(next_relationship_version >= 1),
    operation TEXT NOT NULL CHECK(operation IN ('BIND', 'REBIND', 'UNBIND')),
    previous_client_id TEXT,
    next_client_id TEXT,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings_c3g(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, previous_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, next_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_onboarding_state_c3g (
    tenant_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP', 'MICROSOFT_GRAPH')),
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

CREATE TABLE mailbox_onboarding_history_c3g (
    tenant_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    operation TEXT NOT NULL CHECK(operation IN (
        'START', 'ACTIVATE', 'REQUIRE_REAUTH', 'DISABLE', 'CONFIG_ERROR'
    )),
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP', 'MICROSOFT_GRAPH')),
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

CREATE TABLE mailbox_onboarding_commands_c3g (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    onboarding_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP', 'MICROSOFT_GRAPH')),
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

INSERT INTO mailbox_bindings_c3g (
    tenant_id, binding_id, provider, secret_handle, status, version,
    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms,
    execution_status
)
SELECT tenant_id, binding_id, provider, secret_handle, status, version,
       created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms,
       execution_status
FROM mailbox_bindings;

INSERT INTO mailbox_jobs_c3g (
    tenant_id, binding_id, job_id, cursor, status, attempt, max_attempts,
    next_run_at_ms, provider_status, bounded_item_count, version,
    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms,
    lifecycle_status
)
SELECT tenant_id, binding_id, job_id, cursor, status, attempt, max_attempts,
       next_run_at_ms, provider_status, bounded_item_count, version,
       created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms,
       lifecycle_status
FROM mailbox_jobs;

INSERT INTO mailbox_job_queue_dispatches_c3g SELECT * FROM mailbox_job_queue_dispatches;
INSERT INTO mailbox_job_execution_leases_c3g SELECT * FROM mailbox_job_execution_leases;
INSERT INTO browser_mailbox_execution_bindings_c3g SELECT * FROM browser_mailbox_execution_bindings;
INSERT INTO mailbox_client_association_state_c3g SELECT * FROM mailbox_client_association_state;
INSERT INTO mailbox_client_association_history_c3g SELECT * FROM mailbox_client_association_history;
INSERT INTO mailbox_client_association_commands_c3g SELECT * FROM mailbox_client_association_commands;
INSERT INTO mailbox_onboarding_state_c3g SELECT * FROM mailbox_onboarding_state;
INSERT INTO mailbox_onboarding_history_c3g SELECT * FROM mailbox_onboarding_history;
INSERT INTO mailbox_onboarding_commands_c3g SELECT * FROM mailbox_onboarding_commands;

-- Remove every old child before its RESTRICT parent. DROP TABLE does not execute
-- the governed DELETE triggers; no application mutation semantics are invoked.
DROP TABLE mailbox_job_queue_dispatches;
DROP TABLE mailbox_job_execution_leases;
DROP TABLE browser_mailbox_execution_bindings;
DROP TABLE mailbox_client_association_state;
DROP TABLE mailbox_client_association_history;
DROP TABLE mailbox_client_association_commands;
DROP TABLE mailbox_jobs;
DROP TABLE mailbox_bindings;

DROP TABLE mailbox_onboarding_state;
DROP TABLE mailbox_onboarding_history;
DROP TABLE mailbox_onboarding_commands;

ALTER TABLE mailbox_bindings_c3g RENAME TO mailbox_bindings;
ALTER TABLE mailbox_jobs_c3g RENAME TO mailbox_jobs;
ALTER TABLE mailbox_job_queue_dispatches_c3g RENAME TO mailbox_job_queue_dispatches;
ALTER TABLE mailbox_job_execution_leases_c3g RENAME TO mailbox_job_execution_leases;
ALTER TABLE browser_mailbox_execution_bindings_c3g RENAME TO browser_mailbox_execution_bindings;
ALTER TABLE mailbox_client_association_state_c3g RENAME TO mailbox_client_association_state;
ALTER TABLE mailbox_client_association_history_c3g RENAME TO mailbox_client_association_history;
ALTER TABLE mailbox_client_association_commands_c3g RENAME TO mailbox_client_association_commands;
ALTER TABLE mailbox_onboarding_state_c3g RENAME TO mailbox_onboarding_state;
ALTER TABLE mailbox_onboarding_history_c3g RENAME TO mailbox_onboarding_history;
ALTER TABLE mailbox_onboarding_commands_c3g RENAME TO mailbox_onboarding_commands;

CREATE INDEX mailbox_bindings_status_lookup
    ON mailbox_bindings(tenant_id, status, binding_id);
CREATE INDEX mailbox_jobs_due_lookup
    ON mailbox_jobs(tenant_id, status, next_run_at_ms, job_id);
CREATE INDEX mailbox_jobs_lifecycle_due_lookup
    ON mailbox_jobs(tenant_id, lifecycle_status, next_run_at_ms, job_id);
CREATE INDEX mailbox_job_execution_active_expiry
    ON mailbox_job_execution_leases(lease_state, lease_expires_at_ms, tenant_id, job_id);
CREATE INDEX browser_mailbox_execution_profile_lookup
    ON browser_mailbox_execution_bindings(tenant_id, profile_id, binding_id);
CREATE INDEX mailbox_client_association_client_lookup
    ON mailbox_client_association_state(tenant_id, client_id, binding_id)
    WHERE client_id IS NOT NULL;
CREATE INDEX mailbox_client_association_history_client_lookup
    ON mailbox_client_association_history(tenant_id, next_client_id, changed_at_ms, binding_id)
    WHERE next_client_id IS NOT NULL;
CREATE INDEX mailbox_onboarding_state_status_lookup
    ON mailbox_onboarding_state(tenant_id, lifecycle_status, provider, onboarding_id);

-- The create command is the only accepted provider admission gate that must widen.
DROP TRIGGER mailbox_binding_create_command_validate;
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
    WHERE NEW.provider NOT IN (
        'GMAIL_API', 'IMAP', 'BROWSER_FALLBACK', 'MICROSOFT_GRAPH'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_create_secret_handle_invalid')
    WHERE length(NEW.secret_handle) NOT BETWEEN 8 AND 96
       OR NEW.secret_handle GLOB '*[^A-Za-z0-9_-]*';
END;

-- Recreate table-owned guards exactly at their final pre-C3G semantics.
CREATE TRIGGER mailbox_bindings_insert_governed
BEFORE INSERT ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_insert_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_binding_create_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.provider = NEW.provider
          AND command.secret_handle = NEW.secret_handle
          AND command.command_actor_id = NEW.created_by_actor_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.created_at_ms
          AND command.executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER mailbox_bindings_update_governed
BEFORE UPDATE ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.binding_id <> OLD.binding_id
       OR NEW.provider <> OLD.provider
       OR NEW.secret_handle <> OLD.secret_handle
       OR NEW.created_by_actor_id <> OLD.created_by_actor_id
       OR NEW.created_at_ms <> OLD.created_at_ms;

    SELECT RAISE(ABORT, 'mailbox_binding_update_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_binding_revoke_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.expected_binding_version = OLD.version
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    )
    AND NOT EXISTS (
        SELECT 1
        FROM mailbox_job_run_commands_v2 AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.outcome_status IN ('AUTH_REQUIRED', 'SUSPENDED')
          AND command.outcome_status = NEW.execution_status
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    );

    SELECT RAISE(ABORT, 'mailbox_binding_update_invalid_transition')
    WHERE NOT (
        OLD.status = 'ACTIVE'
        AND NEW.status = 'REVOKED'
        AND NEW.execution_status = OLD.execution_status
        AND NEW.version = OLD.version + 1
    )
    AND NOT (
        OLD.status = 'ACTIVE'
        AND NEW.status = 'ACTIVE'
        AND OLD.execution_status = 'ACTIVE'
        AND NEW.execution_status IN ('AUTH_REQUIRED', 'SUSPENDED')
        AND NEW.version = OLD.version + 1
    );
END;

CREATE TRIGGER mailbox_bindings_delete_governed
BEFORE DELETE ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_delete_not_governed');
END;

CREATE TRIGGER mailbox_jobs_insert_governed
BEFORE INSERT ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_insert_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_job_create_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.job_id = NEW.job_id
          AND command.cursor IS NEW.cursor
          AND command.scheduled_at_ms = NEW.next_run_at_ms
          AND command.max_attempts = NEW.max_attempts
          AND command.command_actor_id = NEW.created_by_actor_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.created_at_ms
          AND command.executed_at_ms = NEW.updated_at_ms
    );

    SELECT RAISE(ABORT, 'mailbox_job_insert_invalid_state')
    WHERE NEW.status <> 'PENDING'
       OR NEW.attempt <> 0
       OR NEW.bounded_item_count <> 0
       OR NEW.version <> 1;
END;

CREATE TRIGGER mailbox_jobs_update_governed
BEFORE UPDATE ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.binding_id <> OLD.binding_id
       OR NEW.job_id <> OLD.job_id
       OR NEW.max_attempts <> OLD.max_attempts
       OR NEW.created_by_actor_id <> OLD.created_by_actor_id
       OR NEW.created_at_ms <> OLD.created_at_ms;

    SELECT RAISE(ABORT, 'mailbox_job_update_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_job_run_commands_v2 AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.job_id = OLD.job_id
          AND command.expected_job_version = OLD.version
          AND command.outcome_status = NEW.lifecycle_status
          AND NEW.status = CASE command.outcome_status
              WHEN 'RETRY_PENDING' THEN 'RETRY_PENDING'
              WHEN 'SUCCEEDED' THEN 'SUCCEEDED'
              WHEN 'FAILED' THEN 'FAILED'
              ELSE 'PENDING'
          END
          AND command.provider_status = NEW.provider_status
          AND command.bounded_item_count = NEW.bounded_item_count
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
          AND (
              (command.outcome_status = 'SUCCEEDED' AND command.next_cursor IS NEW.cursor)
              OR (command.outcome_status <> 'SUCCEEDED' AND NEW.cursor IS OLD.cursor)
          )
          AND (
              (command.outcome_status = 'RETRY_PENDING' AND command.retry_at_ms = NEW.next_run_at_ms)
              OR (command.outcome_status <> 'RETRY_PENDING' AND NEW.next_run_at_ms = OLD.next_run_at_ms)
          )
    );

    SELECT RAISE(ABORT, 'mailbox_job_update_invalid_transition')
    WHERE OLD.lifecycle_status NOT IN ('SCHEDULED', 'RETRY_PENDING')
       OR NEW.lifecycle_status NOT IN (
           'SUCCEEDED', 'RETRY_PENDING', 'AUTH_REQUIRED', 'SUSPENDED', 'FAILED'
       )
       OR NEW.attempt <> OLD.attempt + 1
       OR NEW.version <> OLD.version + 3;
END;

CREATE TRIGGER mailbox_jobs_delete_governed
BEFORE DELETE ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_delete_not_governed');
END;

CREATE TRIGGER mailbox_job_queue_dispatch_delete_forbidden
BEFORE DELETE ON mailbox_job_queue_dispatches
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_queue_dispatch_delete_forbidden');
END;

CREATE TRIGGER mailbox_job_queue_dispatch_update_forbidden
BEFORE UPDATE ON mailbox_job_queue_dispatches
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_queue_dispatch_update_forbidden');
END;

CREATE TRIGGER mailbox_job_execution_lease_delete_forbidden
BEFORE DELETE ON mailbox_job_execution_leases
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_execution_lease_delete_forbidden');
END;

-- BrowserFallback stays a separate lane: its command predicate from 0020 remains
-- provider = 'BROWSER_FALLBACK' and is deliberately not widened to Graph.
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

CREATE TRIGGER mailbox_client_association_command_validate
BEFORE INSERT ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_binding_not_executable')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
          AND execution_status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_version_mismatch')
    WHERE COALESCE((
        SELECT version FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    ), 0) <> NEW.expected_relationship_version;

    SELECT RAISE(ABORT, 'mailbox_client_association_next_version_invalid')
    WHERE NEW.expected_relationship_version >= 9223372036854775807
       OR NEW.next_relationship_version <> NEW.expected_relationship_version + 1;

    SELECT RAISE(ABORT, 'mailbox_client_association_previous_mismatch')
    WHERE COALESCE((
        SELECT client_id FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    ), '') <> COALESCE(NEW.previous_client_id, '');

    SELECT RAISE(ABORT, 'mailbox_client_association_target_not_active')
    WHERE NEW.next_client_id IS NOT NULL
      AND NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.next_client_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_invalid_transition')
    WHERE (NEW.operation = 'BIND' AND NOT (
              NEW.previous_client_id IS NULL AND NEW.next_client_id IS NOT NULL
          ))
       OR (NEW.operation = 'REBIND' AND NOT (
              NEW.previous_client_id IS NOT NULL
              AND NEW.next_client_id IS NOT NULL
              AND NEW.previous_client_id <> NEW.next_client_id
          ))
       OR (NEW.operation = 'UNBIND' AND NOT (
              NEW.previous_client_id IS NOT NULL AND NEW.next_client_id IS NULL
          ));

    SELECT RAISE(ABORT, 'mailbox_client_association_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND NEW.executed_at_ms < updated_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_command_apply
AFTER INSERT ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    INSERT INTO mailbox_client_association_state (
        tenant_id, binding_id, client_id, version, updated_by_actor_id, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.next_client_id,
        NEW.next_relationship_version, NEW.command_actor_id, NEW.executed_at_ms
    )
    ON CONFLICT (tenant_id, binding_id) DO UPDATE SET
        client_id = excluded.client_id,
        version = excluded.version,
        updated_by_actor_id = excluded.updated_by_actor_id,
        updated_at_ms = excluded.updated_at_ms;

    INSERT INTO mailbox_client_association_history (
        tenant_id, binding_id, version, operation,
        previous_client_id, next_client_id, changed_by_actor_id, changed_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.next_relationship_version, NEW.operation,
        NEW.previous_client_id, NEW.next_client_id, NEW.command_actor_id, NEW.executed_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_state_insert_governed
BEFORE INSERT ON mailbox_client_association_state
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_client_association_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.binding_id = NEW.binding_id
      AND command.next_relationship_version = NEW.version
      AND command.next_client_id IS NEW.client_id
      AND command.command_actor_id = NEW.updated_by_actor_id
      AND command.executed_at_ms = NEW.updated_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_not_governed');
END;

CREATE TRIGGER mailbox_client_association_state_update_governed
BEFORE UPDATE ON mailbox_client_association_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id OR NEW.binding_id <> OLD.binding_id;

    SELECT RAISE(ABORT, 'mailbox_client_association_state_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_client_association_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.expected_relationship_version = OLD.version
          AND command.next_relationship_version = NEW.version
          AND command.previous_client_id IS OLD.client_id
          AND command.next_client_id IS NEW.client_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_state_delete_forbidden
BEFORE DELETE ON mailbox_client_association_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_delete_forbidden');
END;

CREATE TRIGGER mailbox_client_association_history_insert_governed
BEFORE INSERT ON mailbox_client_association_history
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_client_association_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.binding_id = NEW.binding_id
      AND command.next_relationship_version = NEW.version
      AND command.operation = NEW.operation
      AND command.previous_client_id IS NEW.previous_client_id
      AND command.next_client_id IS NEW.next_client_id
      AND command.command_actor_id = NEW.changed_by_actor_id
      AND command.executed_at_ms = NEW.changed_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_not_governed');
END;

CREATE TRIGGER mailbox_client_association_history_update_forbidden
BEFORE UPDATE ON mailbox_client_association_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_immutable');
END;

CREATE TRIGGER mailbox_client_association_history_delete_forbidden
BEFORE DELETE ON mailbox_client_association_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_immutable');
END;

CREATE TRIGGER mailbox_client_association_commands_update_forbidden
BEFORE UPDATE ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_command_immutable');
END;

CREATE TRIGGER mailbox_client_association_commands_delete_forbidden
BEFORE DELETE ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_command_immutable');
END;

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

-- D1 keeps foreign-key enforcement enabled; switching deferred checks back on is
-- the migration's fail-closed integrity boundary for the rebuilt graph.
PRAGMA defer_foreign_keys = OFF;
