-- Phase 2F trusted device-principal binding.
-- This table contains metadata only. It never stores Access JWTs, service-token
-- secrets, browser profile bytes, mailbox credentials, message content or raw
-- device attestation material.

CREATE TABLE device_actor_bindings (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL
        CHECK(length(device_id) BETWEEN 8 AND 96)
        CHECK(device_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    version INTEGER NOT NULL CHECK(version >= 1),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED')),
    evidence_reference TEXT NOT NULL
        CHECK(length(evidence_reference) BETWEEN 8 AND 256)
        CHECK(evidence_reference NOT GLOB '*[^A-Za-z0-9_:-]*'),
    bound_at_ms INTEGER NOT NULL CHECK(bound_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= bound_at_ms),
    revoked_at_ms INTEGER,
    CHECK(
        (status = 'ACTIVE' AND revoked_at_ms IS NULL)
        OR
        (status = 'REVOKED'
            AND revoked_at_ms IS NOT NULL
            AND revoked_at_ms >= bound_at_ms
            AND revoked_at_ms = updated_at_ms)
    ),
    PRIMARY KEY (tenant_id, actor_id, version),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

-- A verified tenant actor can represent at most one physical device at a time.
-- Revoked rows remain immutable history and therefore do not block rebinding.
CREATE UNIQUE INDEX device_actor_bindings_one_active_actor
    ON device_actor_bindings(tenant_id, actor_id)
    WHERE status = 'ACTIVE';

CREATE INDEX device_actor_bindings_device_lookup
    ON device_actor_bindings(tenant_id, device_id, status, actor_id, version);
