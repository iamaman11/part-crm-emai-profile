-- Repository Step 5: repairable D1 projection for Durable Object coordinator state.
-- The Durable Object remains authoritative. This command record, projection and
-- outbox event commit together inside D1; no cross-service transaction is claimed.

CREATE TABLE profile_coordinator_projection_commands (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    coordinator_sequence INTEGER NOT NULL CHECK(coordinator_sequence >= 0),
    coordinator_version INTEGER NOT NULL CHECK(coordinator_version >= 1),
    outbox_event_id TEXT NOT NULL
        CHECK(length(outbox_event_id) BETWEEN 8 AND 96)
        CHECK(outbox_event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    outcome TEXT NOT NULL CHECK(outcome IN (
        'snapshot',
        'launch_intent_issued',
        'lease_claimed',
        'heartbeat_accepted',
        'released',
        'drain_started',
        'timed_out',
        'launch_intent_expired',
        'recovered',
        'no_change'
    )),
    projection_json TEXT NOT NULL CHECK(json_valid(projection_json)),
    projected_at_ms INTEGER NOT NULL CHECK(projected_at_ms >= 0),
    PRIMARY KEY (tenant_id, profile_id, coordinator_sequence),
    UNIQUE (tenant_id, outbox_event_id),
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE profile_coordinator_projections (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    coordinator_status TEXT NOT NULL CHECK(coordinator_status IN (
        'idle', 'active', 'draining', 'dirty', 'uncertain'
    )),
    coordinator_version INTEGER NOT NULL CHECK(coordinator_version >= 1),
    coordinator_sequence INTEGER NOT NULL CHECK(coordinator_sequence >= 0),
    next_epoch INTEGER NOT NULL CHECK(next_epoch >= 0),
    active_session_id TEXT,
    active_device_id TEXT,
    active_epoch INTEGER CHECK(active_epoch IS NULL OR active_epoch >= 1),
    idle_expires_at_ms INTEGER CHECK(idle_expires_at_ms IS NULL OR idle_expires_at_ms >= 0),
    hard_expires_at_ms INTEGER CHECK(hard_expires_at_ms IS NULL OR hard_expires_at_ms >= 0),
    drain_deadline_ms INTEGER CHECK(drain_deadline_ms IS NULL OR drain_deadline_ms >= 0),
    pending_launch_intent_id TEXT,
    pending_intent_expires_at_ms INTEGER CHECK(
        pending_intent_expires_at_ms IS NULL OR pending_intent_expires_at_ms >= 0
    ),
    projected_at_ms INTEGER NOT NULL CHECK(projected_at_ms >= 0),
    PRIMARY KEY (tenant_id, profile_id),
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    CHECK(coordinator_version = coordinator_sequence + 1),
    CHECK(
        (coordinator_status IN ('active', 'draining')
            AND active_session_id IS NOT NULL
            AND active_device_id IS NOT NULL
            AND active_epoch IS NOT NULL
            AND idle_expires_at_ms IS NOT NULL
            AND hard_expires_at_ms IS NOT NULL)
        OR
        (coordinator_status NOT IN ('active', 'draining')
            AND active_session_id IS NULL
            AND active_device_id IS NULL
            AND active_epoch IS NULL
            AND idle_expires_at_ms IS NULL
            AND hard_expires_at_ms IS NULL)
    ),
    CHECK(
        (coordinator_status = 'draining' AND drain_deadline_ms IS NOT NULL)
        OR
        (coordinator_status <> 'draining' AND drain_deadline_ms IS NULL)
    ),
    CHECK(
        (pending_launch_intent_id IS NULL AND pending_intent_expires_at_ms IS NULL)
        OR
        (coordinator_status = 'idle'
            AND pending_launch_intent_id IS NOT NULL
            AND pending_intent_expires_at_ms IS NOT NULL)
    )
) STRICT;

CREATE INDEX profile_coordinator_projection_lag_lookup
    ON profile_coordinator_projections(
        tenant_id, coordinator_sequence, profile_id
    );

CREATE UNIQUE INDEX one_profile_coordinator_outbox_per_version
    ON outbox_events(
        tenant_id, aggregate_type, aggregate_id, aggregate_version
    )
    WHERE aggregate_type = 'profile_coordinator';

CREATE TRIGGER profile_coordinator_projection_command_validate
BEFORE INSERT ON profile_coordinator_projection_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'coordinator_projection_identity_mismatch')
    WHERE json_extract(NEW.projection_json, '$.tenant_id') <> NEW.tenant_id
       OR json_extract(NEW.projection_json, '$.profile_id') <> NEW.profile_id;

    SELECT RAISE(ABORT, 'coordinator_projection_sequence_mismatch')
    WHERE CAST(json_extract(NEW.projection_json, '$.sequence') AS INTEGER)
            <> NEW.coordinator_sequence
       OR CAST(json_extract(NEW.projection_json, '$.version') AS INTEGER)
            <> NEW.coordinator_version
       OR NEW.coordinator_version <> NEW.coordinator_sequence + 1;

    SELECT RAISE(ABORT, 'coordinator_projection_stale')
    WHERE EXISTS (
        SELECT 1
        FROM profile_coordinator_projections
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND coordinator_sequence > NEW.coordinator_sequence
    );

    SELECT RAISE(ABORT, 'coordinator_projection_conflict')
    WHERE EXISTS (
        SELECT 1
        FROM profile_coordinator_projections
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND coordinator_sequence = NEW.coordinator_sequence
    )
      AND NOT EXISTS (
        SELECT 1
        FROM profile_coordinator_projection_commands
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND coordinator_sequence = NEW.coordinator_sequence
          AND coordinator_version = NEW.coordinator_version
          AND outcome = NEW.outcome
          AND projection_json = NEW.projection_json
    );
END;

CREATE TRIGGER profile_coordinator_projection_command_apply
AFTER INSERT ON profile_coordinator_projection_commands
FOR EACH ROW
BEGIN
    INSERT INTO profile_coordinator_projections (
        tenant_id,
        profile_id,
        coordinator_status,
        coordinator_version,
        coordinator_sequence,
        next_epoch,
        active_session_id,
        active_device_id,
        active_epoch,
        idle_expires_at_ms,
        hard_expires_at_ms,
        drain_deadline_ms,
        pending_launch_intent_id,
        pending_intent_expires_at_ms,
        projected_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.profile_id,
        json_extract(NEW.projection_json, '$.status'),
        NEW.coordinator_version,
        NEW.coordinator_sequence,
        CAST(json_extract(NEW.projection_json, '$.next_epoch') AS INTEGER),
        json_extract(NEW.projection_json, '$.active_session_id'),
        json_extract(NEW.projection_json, '$.active_device_id'),
        CAST(json_extract(NEW.projection_json, '$.active_epoch') AS INTEGER),
        CAST(json_extract(NEW.projection_json, '$.idle_expires_at_ms') AS INTEGER),
        CAST(json_extract(NEW.projection_json, '$.hard_expires_at_ms') AS INTEGER),
        CAST(json_extract(NEW.projection_json, '$.drain_deadline_ms') AS INTEGER),
        json_extract(NEW.projection_json, '$.pending_launch_intent_id'),
        CAST(json_extract(NEW.projection_json, '$.pending_intent_expires_at_ms') AS INTEGER),
        NEW.projected_at_ms
    )
    ON CONFLICT (tenant_id, profile_id) DO UPDATE SET
        coordinator_status = excluded.coordinator_status,
        coordinator_version = excluded.coordinator_version,
        coordinator_sequence = excluded.coordinator_sequence,
        next_epoch = excluded.next_epoch,
        active_session_id = excluded.active_session_id,
        active_device_id = excluded.active_device_id,
        active_epoch = excluded.active_epoch,
        idle_expires_at_ms = excluded.idle_expires_at_ms,
        hard_expires_at_ms = excluded.hard_expires_at_ms,
        drain_deadline_ms = excluded.drain_deadline_ms,
        pending_launch_intent_id = excluded.pending_launch_intent_id,
        pending_intent_expires_at_ms = excluded.pending_intent_expires_at_ms,
        projected_at_ms = excluded.projected_at_ms
    WHERE excluded.coordinator_sequence
        > profile_coordinator_projections.coordinator_sequence;

    INSERT INTO outbox_events (
        tenant_id,
        outbox_event_id,
        aggregate_type,
        aggregate_id,
        aggregate_version,
        event_type,
        payload_json,
        created_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.outbox_event_id,
        'profile_coordinator',
        NEW.profile_id,
        NEW.coordinator_version,
        'profile_coordinator.' || NEW.outcome || '.v1',
        NEW.projection_json,
        NEW.projected_at_ms
    );
END;

CREATE TRIGGER profile_coordinator_projection_commands_are_append_only_update
BEFORE UPDATE ON profile_coordinator_projection_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'coordinator_projection_command_append_only');
END;

CREATE TRIGGER profile_coordinator_projection_commands_are_append_only_delete
BEFORE DELETE ON profile_coordinator_projection_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'coordinator_projection_command_append_only');
END;
