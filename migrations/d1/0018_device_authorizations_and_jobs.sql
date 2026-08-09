-- Phase 2F durable device authorization and device-job coordination.
-- Metadata only: no browser profile bytes, cookies, mailbox credentials, message
-- bodies, raw secrets or arbitrary browser telemetry belong in these tables.

CREATE TABLE device_authorizations (
    tenant_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED')),
    evidence_reference TEXT NOT NULL
        CHECK(length(evidence_reference) BETWEEN 8 AND 256)
        CHECK(evidence_reference NOT GLOB '*[^A-Za-z0-9_:-]*'),
    authorized_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    authorized_at_ms INTEGER NOT NULL CHECK(authorized_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= authorized_at_ms),
    revoked_at_ms INTEGER,
    CHECK(
        (status = 'ACTIVE' AND revoked_at_ms IS NULL)
        OR
        (status = 'REVOKED'
            AND revoked_at_ms IS NOT NULL
            AND revoked_at_ms >= authorized_at_ms
            AND revoked_at_ms = updated_at_ms)
    ),
    PRIMARY KEY (tenant_id, device_id, profile_id, generation_id),
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, authorized_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX device_authorizations_execution_lookup
    ON device_authorizations(
        tenant_id, profile_id, generation_id, status, device_id, version
    );

CREATE TABLE device_jobs (
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL
        CHECK(length(job_id) BETWEEN 8 AND 96)
        CHECK(job_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    aggregate_version INTEGER NOT NULL CHECK(aggregate_version >= 1),
    status TEXT NOT NULL CHECK(status IN (
        'PENDING_DEVICE',
        'PROFILE_BUSY',
        'RUNNING',
        'RETRY_SCHEDULED',
        'AUTH_REQUIRED',
        'RECOVERY_REQUIRED',
        'SUCCEEDED',
        'FAILED',
        'CANCELLED'
    )),
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 0 AND 100),
    max_attempts INTEGER NOT NULL CHECK(max_attempts BETWEEN 1 AND 100),
    last_fence INTEGER NOT NULL CHECK(last_fence >= 0),
    current_claim_id TEXT
        CHECK(current_claim_id IS NULL OR (
            length(current_claim_id) BETWEEN 8 AND 96
            AND current_claim_id NOT GLOB '*[^A-Za-z0-9_-]*'
        )),
    claim_fence INTEGER CHECK(claim_fence IS NULL OR claim_fence > 0),
    claimed_at_ms INTEGER CHECK(claimed_at_ms IS NULL OR claimed_at_ms >= 0),
    claim_heartbeat_at_ms INTEGER
        CHECK(claim_heartbeat_at_ms IS NULL OR claim_heartbeat_at_ms >= 0),
    claim_lease_expires_at_ms INTEGER
        CHECK(claim_lease_expires_at_ms IS NULL OR claim_lease_expires_at_ms >= 0),
    retry_at_ms INTEGER CHECK(retry_at_ms IS NULL OR retry_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    CHECK(attempt <= max_attempts),
    CHECK(last_fence = attempt),
    CHECK(aggregate_version > attempt),
    CHECK(
        status NOT IN (
            'PROFILE_BUSY', 'RUNNING', 'RETRY_SCHEDULED', 'AUTH_REQUIRED',
            'RECOVERY_REQUIRED', 'SUCCEEDED', 'FAILED'
        )
        OR attempt > 0
    ),
    CHECK(
        (
            current_claim_id IS NULL
            AND claim_fence IS NULL
            AND claimed_at_ms IS NULL
            AND claim_heartbeat_at_ms IS NULL
            AND claim_lease_expires_at_ms IS NULL
        )
        OR
        (
            current_claim_id IS NOT NULL
            AND claim_fence IS NOT NULL
            AND claimed_at_ms IS NOT NULL
            AND claim_heartbeat_at_ms IS NOT NULL
            AND claim_lease_expires_at_ms IS NOT NULL
        )
    ),
    CHECK(
        (status = 'RUNNING' AND current_claim_id IS NOT NULL)
        OR
        (status <> 'RUNNING' AND current_claim_id IS NULL)
    ),
    CHECK(
        current_claim_id IS NULL
        OR (
            claim_fence = last_fence
            AND claimed_at_ms <= claim_heartbeat_at_ms
            AND claim_heartbeat_at_ms = updated_at_ms
            AND claim_heartbeat_at_ms < claim_lease_expires_at_ms
        )
    ),
    CHECK(
        (status IN ('PROFILE_BUSY', 'RETRY_SCHEDULED')
            AND retry_at_ms IS NOT NULL
            AND retry_at_ms > updated_at_ms)
        OR
        (status NOT IN ('PROFILE_BUSY', 'RETRY_SCHEDULED')
            AND retry_at_ms IS NULL)
    ),
    PRIMARY KEY (tenant_id, job_id),
    FOREIGN KEY (tenant_id, device_id, profile_id, generation_id)
        REFERENCES device_authorizations(
            tenant_id, device_id, profile_id, generation_id
        ) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX device_jobs_due_lookup
    ON device_jobs(tenant_id, status, retry_at_ms, updated_at_ms, job_id);

CREATE INDEX device_jobs_claimable_device_lookup
    ON device_jobs(
        tenant_id, device_id, status, retry_at_ms, updated_at_ms, job_id
    );

CREATE INDEX device_jobs_target_lookup
    ON device_jobs(
        tenant_id, device_id, profile_id, generation_id, status, updated_at_ms, job_id
    );
