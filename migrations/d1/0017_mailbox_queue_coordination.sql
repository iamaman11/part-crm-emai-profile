-- Phase 2E queue coordination metadata.
-- These tables contain only opaque identifiers, versions, fences and timestamps.
-- They must never contain mailbox credentials or message content.

CREATE TABLE mailbox_job_queue_dispatches (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    expected_job_version INTEGER NOT NULL CHECK(expected_job_version >= 1),
    published_at_ms INTEGER NOT NULL CHECK(published_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id, job_id, expected_job_version),
    FOREIGN KEY (tenant_id, binding_id, job_id)
        REFERENCES mailbox_jobs(tenant_id, binding_id, job_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE mailbox_job_execution_leases (
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
        REFERENCES mailbox_jobs(tenant_id, binding_id, job_id) ON DELETE RESTRICT,
    CHECK(
        (lease_state = 'ACTIVE' AND completed_at_ms IS NULL)
        OR
        (lease_state = 'COMPLETED' AND completed_at_ms IS NOT NULL
            AND completed_at_ms >= claimed_at_ms)
    )
) STRICT;

CREATE INDEX mailbox_job_execution_active_expiry
    ON mailbox_job_execution_leases(lease_state, lease_expires_at_ms, tenant_id, job_id);

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
