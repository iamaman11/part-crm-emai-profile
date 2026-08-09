-- Phase 2E canonical cloud mailbox lifecycle.
-- Existing Phase 2A/2D columns remain as compatibility projections; new lifecycle
-- columns carry the provider-neutral execution states without rebuilding accepted D1 tables.
-- No credentials or mailbox content are stored here.

ALTER TABLE mailbox_bindings
ADD COLUMN execution_status TEXT NOT NULL DEFAULT 'ACTIVE'
    CHECK(execution_status IN ('ACTIVE', 'AUTH_REQUIRED', 'SUSPENDED'));

ALTER TABLE mailbox_jobs
ADD COLUMN lifecycle_status TEXT NOT NULL DEFAULT 'SCHEDULED'
    CHECK(lifecycle_status IN (
        'SCHEDULED', 'QUEUED', 'RUNNING', 'RETRY_PENDING',
        'AUTH_REQUIRED', 'SUSPENDED', 'SUCCEEDED', 'FAILED'
    ));

UPDATE mailbox_jobs
SET lifecycle_status = CASE status
    WHEN 'PENDING' THEN 'SCHEDULED'
    ELSE status
END;

CREATE INDEX mailbox_jobs_lifecycle_due_lookup
    ON mailbox_jobs(tenant_id, lifecycle_status, next_run_at_ms, job_id);

-- Phase 2E run commands persist the three canonical domain transitions
-- Scheduled/RetryPending -> Queued -> Running -> outcome atomically. The legacy
-- status column is maintained only as a compatibility projection.
CREATE TABLE mailbox_job_run_commands_v2 (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    outcome_status TEXT NOT NULL CHECK(outcome_status IN (
        'RETRY_PENDING', 'AUTH_REQUIRED', 'SUSPENDED', 'SUCCEEDED', 'FAILED'
    )),
    next_cursor TEXT,
    provider_status TEXT NOT NULL,
    bounded_item_count INTEGER NOT NULL CHECK(bounded_item_count BETWEEN 0 AND 10000),
    retry_at_ms INTEGER,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_job_run_v2_command_validate
BEFORE INSERT ON mailbox_job_run_commands_v2
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
    SELECT RAISE(ABORT, 'mailbox_binding_not_executable')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
          AND execution_status = 'ACTIVE'
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
          AND lifecycle_status IN ('SCHEDULED', 'RETRY_PENDING')
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
    WHERE NEW.expected_job_version > 9223372036854775804;
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

CREATE TRIGGER mailbox_job_run_v2_command_apply
AFTER INSERT ON mailbox_job_run_commands_v2
FOR EACH ROW
BEGIN
    UPDATE mailbox_jobs
    SET lifecycle_status = NEW.outcome_status,
        status = CASE NEW.outcome_status
            WHEN 'RETRY_PENDING' THEN 'RETRY_PENDING'
            WHEN 'SUCCEEDED' THEN 'SUCCEEDED'
            WHEN 'FAILED' THEN 'FAILED'
            ELSE 'PENDING'
        END,
        attempt = attempt + 1,
        cursor = CASE WHEN NEW.outcome_status = 'SUCCEEDED' THEN NEW.next_cursor ELSE cursor END,
        provider_status = NEW.provider_status,
        bounded_item_count = NEW.bounded_item_count,
        next_run_at_ms = CASE
            WHEN NEW.outcome_status = 'RETRY_PENDING' THEN NEW.retry_at_ms
            ELSE next_run_at_ms
        END,
        version = version + 3,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND binding_id = NEW.binding_id
      AND job_id = NEW.job_id;

    UPDATE mailbox_bindings
    SET execution_status = CASE NEW.outcome_status
            WHEN 'AUTH_REQUIRED' THEN 'AUTH_REQUIRED'
            WHEN 'SUSPENDED' THEN 'SUSPENDED'
            ELSE execution_status
        END,
        updated_by_actor_id = CASE
            WHEN NEW.outcome_status IN ('AUTH_REQUIRED', 'SUSPENDED') THEN NEW.command_actor_id
            ELSE updated_by_actor_id
        END,
        updated_at_ms = CASE
            WHEN NEW.outcome_status IN ('AUTH_REQUIRED', 'SUSPENDED') THEN NEW.executed_at_ms
            ELSE updated_at_ms
        END
    WHERE tenant_id = NEW.tenant_id
      AND binding_id = NEW.binding_id
      AND status = 'ACTIVE';
END;

-- Fail closed for binaries that still try to use the pre-Phase-2E run command.
DROP TRIGGER mailbox_job_run_command_validate;
DROP TRIGGER mailbox_job_run_command_apply;

CREATE TRIGGER mailbox_job_run_command_legacy_reject
BEFORE INSERT ON mailbox_job_run_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_run_v2_required');
END;
