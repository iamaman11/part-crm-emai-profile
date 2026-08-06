-- Repository Step 6: local rebuildable Bridge state and idempotent outbox.
-- This schema is repository-local feasibility evidence, not a production key store.

CREATE TABLE bridge_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    version INTEGER NOT NULL CHECK(version >= 1),
    lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN (
        'idle', 'claimed', 'starting', 'ready', 'closing', 'dirty', 'uncertain'
    )),
    active_session_id TEXT,
    workspace_epoch INTEGER CHECK(workspace_epoch IS NULL OR workspace_epoch >= 1),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    CHECK(
        (lifecycle_state IN ('starting', 'ready', 'closing') AND active_session_id IS NOT NULL)
        OR
        (lifecycle_state NOT IN ('starting', 'ready', 'closing') AND active_session_id IS NULL)
    )
) STRICT;

INSERT INTO bridge_state (
    singleton, version, lifecycle_state, active_session_id, workspace_epoch, updated_at_ms
) VALUES (1, 1, 'idle', NULL, NULL, 0);

CREATE TABLE bridge_commands (
    command_id TEXT PRIMARY KEY
        CHECK(length(command_id) BETWEEN 8 AND 96)
        CHECK(command_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    sequence INTEGER NOT NULL UNIQUE CHECK(sequence >= 1),
    expected_version INTEGER NOT NULL CHECK(expected_version >= 1),
    command_type TEXT NOT NULL CHECK(command_type IN (
        'redeem_claim', 'acquire_workspace', 'start_runtime', 'runtime_ready',
        'request_close', 'runtime_closed', 'runtime_crashed', 'runtime_timed_out',
        'release_workspace', 'recover'
    )),
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    result_json TEXT NOT NULL CHECK(json_valid(result_json)),
    outbox_event_id TEXT NOT NULL UNIQUE
        CHECK(length(outbox_event_id) BETWEEN 8 AND 96)
        CHECK(outbox_event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0)
) STRICT;

CREATE TABLE bridge_outbox (
    outbox_event_id TEXT PRIMARY KEY,
    sequence INTEGER NOT NULL UNIQUE CHECK(sequence >= 1),
    event_type TEXT NOT NULL
        CHECK(length(event_type) BETWEEN 3 AND 96)
        CHECK(event_type NOT GLOB '*[^A-Za-z0-9_.-]*'),
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    delivery_state TEXT NOT NULL CHECK(delivery_state IN ('PENDING', 'DELIVERED')),
    attempts INTEGER NOT NULL CHECK(attempts >= 0),
    next_attempt_at_ms INTEGER CHECK(next_attempt_at_ms IS NULL OR next_attempt_at_ms >= 0),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    delivered_at_ms INTEGER CHECK(delivered_at_ms IS NULL OR delivered_at_ms >= 0),
    FOREIGN KEY (outbox_event_id) REFERENCES bridge_commands(outbox_event_id) ON DELETE RESTRICT,
    CHECK(
        (delivery_state = 'PENDING' AND delivered_at_ms IS NULL)
        OR
        (delivery_state = 'DELIVERED' AND delivered_at_ms IS NOT NULL)
    )
) STRICT;

CREATE TRIGGER bridge_command_validate
BEFORE INSERT ON bridge_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'bridge_command_conflict')
    WHERE EXISTS (
        SELECT 1
        FROM bridge_commands
        WHERE command_id = NEW.command_id
          AND (
            sequence <> NEW.sequence
            OR expected_version <> NEW.expected_version
            OR command_type <> NEW.command_type
            OR payload_json <> NEW.payload_json
            OR result_json <> NEW.result_json
          )
    );

    SELECT RAISE(IGNORE)
    WHERE EXISTS (
        SELECT 1
        FROM bridge_commands
        WHERE command_id = NEW.command_id
          AND sequence = NEW.sequence
          AND expected_version = NEW.expected_version
          AND command_type = NEW.command_type
          AND payload_json = NEW.payload_json
          AND result_json = NEW.result_json
    );

    SELECT RAISE(ABORT, 'bridge_command_stale_version')
    WHERE NEW.expected_version <> (
        SELECT version FROM bridge_state WHERE singleton = 1
    );

    SELECT RAISE(ABORT, 'bridge_command_reordered')
    WHERE NEW.sequence <> (
        SELECT version FROM bridge_state WHERE singleton = 1
    );

    SELECT RAISE(ABORT, 'bridge_command_result_invalid')
    WHERE json_extract(NEW.result_json, '$.state') NOT IN (
        'idle', 'claimed', 'starting', 'ready', 'closing', 'dirty', 'uncertain'
    );
END;

CREATE TRIGGER bridge_command_apply
AFTER INSERT ON bridge_commands
FOR EACH ROW
BEGIN
    UPDATE bridge_state
    SET
        version = version + 1,
        lifecycle_state = json_extract(NEW.result_json, '$.state'),
        active_session_id = json_extract(NEW.result_json, '$.active_session_id'),
        workspace_epoch = CAST(json_extract(NEW.result_json, '$.workspace_epoch') AS INTEGER),
        updated_at_ms = NEW.created_at_ms
    WHERE singleton = 1;

    INSERT INTO bridge_outbox (
        outbox_event_id,
        sequence,
        event_type,
        payload_json,
        delivery_state,
        attempts,
        next_attempt_at_ms,
        created_at_ms,
        delivered_at_ms
    ) VALUES (
        NEW.outbox_event_id,
        NEW.sequence,
        'bridge.' || NEW.command_type || '.v1',
        NEW.result_json,
        'PENDING',
        0,
        NEW.created_at_ms,
        NEW.created_at_ms,
        NULL
    );
END;

CREATE TRIGGER bridge_commands_are_append_only_update
BEFORE UPDATE ON bridge_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'bridge_command_append_only');
END;

CREATE TRIGGER bridge_commands_are_append_only_delete
BEFORE DELETE ON bridge_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'bridge_command_append_only');
END;

CREATE TRIGGER bridge_outbox_immutable_payload
BEFORE UPDATE ON bridge_outbox
FOR EACH ROW
WHEN OLD.outbox_event_id <> NEW.outbox_event_id
  OR OLD.sequence <> NEW.sequence
  OR OLD.event_type <> NEW.event_type
  OR OLD.payload_json <> NEW.payload_json
  OR OLD.created_at_ms <> NEW.created_at_ms
BEGIN
    SELECT RAISE(ABORT, 'bridge_outbox_payload_immutable');
END;

CREATE TRIGGER bridge_outbox_no_delete
BEFORE DELETE ON bridge_outbox
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'bridge_outbox_append_only');
END;
