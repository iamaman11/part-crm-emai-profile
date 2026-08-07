-- Mailbox metadata catalog and governed job lifecycle.
-- This schema intentionally stores only secret handles and bounded provider metadata.
-- Raw mailbox credentials, authorization tokens and message bodies are prohibited.

CREATE TABLE mailbox_bindings (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL
        CHECK(length(binding_id) BETWEEN 8 AND 96)
        CHECK(binding_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    provider TEXT NOT NULL CHECK(provider IN ('GMAIL_API', 'IMAP', 'BROWSER_FALLBACK')),
    secret_handle TEXT NOT NULL
        CHECK(length(secret_handle) BETWEEN 8 AND 96)
        CHECK(secret_handle NOT GLOB '*[^A-Za-z0-9_-]*'),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX mailbox_bindings_status_lookup
    ON mailbox_bindings(tenant_id, status, binding_id);

CREATE TABLE mailbox_jobs (
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
    PRIMARY KEY (tenant_id, binding_id, job_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX mailbox_jobs_due_lookup
    ON mailbox_jobs(tenant_id, status, next_run_at_ms, job_id);

CREATE TABLE mailbox_binding_create_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    secret_handle TEXT NOT NULL,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

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
END;

CREATE TABLE mailbox_binding_revoke_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    expected_binding_version INTEGER NOT NULL CHECK(expected_binding_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_binding_revoke_command_validate
BEFORE INSERT ON mailbox_binding_revoke_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_revoke_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    );
    SELECT RAISE(ABORT, 'mailbox_binding_already_revoked')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'REVOKED'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
          AND version = NEW.expected_binding_version
    );
    SELECT RAISE(ABORT, 'mailbox_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND NEW.executed_at_ms < updated_at_ms
    );
END;

CREATE TRIGGER mailbox_binding_revoke_command_apply
AFTER INSERT ON mailbox_binding_revoke_commands
FOR EACH ROW
BEGIN
    UPDATE mailbox_bindings
    SET status = 'REVOKED',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id;
END;

CREATE TABLE mailbox_job_create_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    cursor TEXT,
    scheduled_at_ms INTEGER NOT NULL CHECK(scheduled_at_ms >= 0),
    max_attempts INTEGER NOT NULL CHECK(max_attempts BETWEEN 1 AND 10),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_job_create_command_validate
BEFORE INSERT ON mailbox_job_create_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_create_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    );
    SELECT RAISE(ABORT, 'mailbox_binding_revoked')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_cursor_too_long')
    WHERE NEW.cursor IS NOT NULL AND length(NEW.cursor) > 512;
    SELECT RAISE(ABORT, 'mailbox_time_regression')
    WHERE NEW.scheduled_at_ms < NEW.executed_at_ms;
END;

CREATE TRIGGER mailbox_job_create_command_apply
AFTER INSERT ON mailbox_job_create_commands
FOR EACH ROW
BEGIN
    INSERT INTO mailbox_jobs (
        tenant_id, binding_id, job_id, cursor, status, attempt, max_attempts,
        next_run_at_ms, bounded_item_count, version,
        created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.job_id, NEW.cursor, 'PENDING', 0, NEW.max_attempts,
        NEW.scheduled_at_ms, 0, 1,
        NEW.command_actor_id, NEW.command_actor_id, NEW.executed_at_ms, NEW.executed_at_ms
    );
END;

CREATE TABLE mailbox_job_run_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    outcome_status TEXT NOT NULL CHECK(outcome_status IN ('SUCCEEDED', 'RETRY_PENDING', 'FAILED')),
    next_cursor TEXT,
    provider_status TEXT NOT NULL,
    bounded_item_count INTEGER NOT NULL CHECK(bounded_item_count BETWEEN 0 AND 10000),
    retry_at_ms INTEGER,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_job_run_command_validate
BEFORE INSERT ON mailbox_job_run_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_run_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_binding_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    );
    SELECT RAISE(ABORT, 'mailbox_binding_revoked')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'mailbox_job_missing')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_jobs
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND job_id = NEW.job_id
    );
    SELECT RAISE(ABORT, 'mailbox_job_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_jobs
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND job_id = NEW.job_id
          AND status IN ('PENDING', 'RETRY_PENDING')
          AND version = NEW.expected_job_version
    );
    SELECT RAISE(ABORT, 'mailbox_job_not_due')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_jobs
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND job_id = NEW.job_id
          AND NEW.executed_at_ms < next_run_at_ms
    );
    SELECT RAISE(ABORT, 'mailbox_job_attempts_exhausted')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_jobs
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND job_id = NEW.job_id
          AND attempt >= max_attempts
    );
    SELECT RAISE(ABORT, 'mailbox_job_version_overflow')
    WHERE NEW.expected_job_version > 9223372036854775805;
    SELECT RAISE(ABORT, 'mailbox_provider_status_invalid')
    WHERE length(NEW.provider_status) NOT BETWEEN 1 AND 64
       OR NEW.provider_status GLOB '*[^A-Za-z0-9_.-]*';
    SELECT RAISE(ABORT, 'mailbox_cursor_too_long')
    WHERE NEW.next_cursor IS NOT NULL AND length(NEW.next_cursor) > 512;
    SELECT RAISE(ABORT, 'mailbox_retry_time_invalid')
    WHERE (NEW.outcome_status = 'RETRY_PENDING' AND (
              NEW.retry_at_ms IS NULL OR NEW.retry_at_ms <= NEW.executed_at_ms
          ))
       OR (NEW.outcome_status <> 'RETRY_PENDING' AND NEW.retry_at_ms IS NOT NULL);
    SELECT RAISE(ABORT, 'mailbox_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_jobs
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND job_id = NEW.job_id
          AND NEW.executed_at_ms < updated_at_ms
    );
END;

CREATE TRIGGER mailbox_job_run_command_apply
AFTER INSERT ON mailbox_job_run_commands
FOR EACH ROW
BEGIN
    UPDATE mailbox_jobs
    SET status = NEW.outcome_status,
        attempt = attempt + 1,
        cursor = CASE WHEN NEW.outcome_status = 'SUCCEEDED' THEN NEW.next_cursor ELSE cursor END,
        provider_status = NEW.provider_status,
        bounded_item_count = NEW.bounded_item_count,
        next_run_at_ms = CASE
            WHEN NEW.outcome_status = 'RETRY_PENDING' THEN NEW.retry_at_ms
            ELSE next_run_at_ms
        END,
        version = version + 2,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND binding_id = NEW.binding_id
      AND job_id = NEW.job_id;
END;
