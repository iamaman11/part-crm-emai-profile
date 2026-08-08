-- Phase 1B: durable notification delivery, replay, catch-up and operational-state foundation.
-- These tables are operational state only. They must never cascade deletion into canonical outbox,
-- business state or immutable audit evidence.

CREATE TABLE notification_deliveries (
    tenant_id TEXT NOT NULL,
    consumer_id TEXT NOT NULL
        CHECK(length(consumer_id) BETWEEN 8 AND 96)
        CHECK(consumer_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    outbox_event_id TEXT NOT NULL,
    delivery_state TEXT NOT NULL
        CHECK(delivery_state IN ('READY', 'RETRY_SCHEDULED', 'DELIVERED', 'DEAD_LETTER')),
    attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 0 AND 64),
    last_attempt_at_ms INTEGER CHECK(last_attempt_at_ms IS NULL OR last_attempt_at_ms >= 0),
    next_attempt_at_ms INTEGER CHECK(next_attempt_at_ms IS NULL OR next_attempt_at_ms >= 0),
    delivered_at_ms INTEGER CHECK(delivered_at_ms IS NULL OR delivered_at_ms >= 0),
    terminal_at_ms INTEGER CHECK(terminal_at_ms IS NULL OR terminal_at_ms >= 0),
    failure_class TEXT CHECK(
        failure_class IS NULL OR failure_class IN (
            'DEPENDENCY_UNAVAILABLE',
            'REJECTED',
            'INTEGRITY_FAILURE',
            'INTERNAL_FAILURE'
        )
    ),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, consumer_id, outbox_event_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, outbox_event_id)
        REFERENCES outbox_events(tenant_id, outbox_event_id) ON DELETE RESTRICT,
    CHECK(last_attempt_at_ms IS NULL OR last_attempt_at_ms >= created_at_ms),
    CHECK(
        (delivery_state = 'READY'
            AND attempt_count = 0
            AND last_attempt_at_ms IS NULL
            AND next_attempt_at_ms IS NULL
            AND delivered_at_ms IS NULL
            AND terminal_at_ms IS NULL
            AND failure_class IS NULL)
        OR
        (delivery_state = 'RETRY_SCHEDULED'
            AND attempt_count BETWEEN 1 AND 63
            AND last_attempt_at_ms IS NOT NULL
            AND next_attempt_at_ms IS NOT NULL
            AND next_attempt_at_ms > last_attempt_at_ms
            AND delivered_at_ms IS NULL
            AND terminal_at_ms IS NULL
            AND failure_class IS NOT NULL)
        OR
        (delivery_state = 'DELIVERED'
            AND attempt_count BETWEEN 1 AND 64
            AND last_attempt_at_ms IS NOT NULL
            AND next_attempt_at_ms IS NULL
            AND delivered_at_ms IS NOT NULL
            AND delivered_at_ms = last_attempt_at_ms
            AND terminal_at_ms IS NULL
            AND failure_class IS NULL)
        OR
        (delivery_state = 'DEAD_LETTER'
            AND attempt_count BETWEEN 1 AND 64
            AND last_attempt_at_ms IS NOT NULL
            AND next_attempt_at_ms IS NULL
            AND delivered_at_ms IS NULL
            AND terminal_at_ms IS NOT NULL
            AND terminal_at_ms = last_attempt_at_ms
            AND failure_class IS NOT NULL)
    )
) STRICT;

CREATE TRIGGER notification_delivery_source_guard
BEFORE INSERT ON notification_deliveries
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_delivery_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE tenant_id = NEW.tenant_id
          AND outbox_event_id = NEW.outbox_event_id
          AND created_at_ms <= NEW.created_at_ms
    );
END;

CREATE TABLE notification_replay_intents (
    tenant_id TEXT NOT NULL,
    replay_id TEXT NOT NULL
        CHECK(length(replay_id) BETWEEN 8 AND 96)
        CHECK(replay_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    consumer_id TEXT NOT NULL
        CHECK(length(consumer_id) BETWEEN 8 AND 96)
        CHECK(consumer_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    outbox_event_id TEXT NOT NULL,
    audit_event_id TEXT NOT NULL
        CHECK(length(audit_event_id) BETWEEN 8 AND 96)
        CHECK(audit_event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    correlation_id TEXT NOT NULL
        CHECK(length(correlation_id) BETWEEN 8 AND 96)
        CHECK(correlation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    requested_by_actor_id TEXT NOT NULL,
    reason_class TEXT NOT NULL CHECK(reason_class IN (
        'DEPENDENCY_RECOVERED',
        'OPERATOR_REMEDIATION',
        'INTEGRITY_REVALIDATED'
    )),
    terminal_attempt_count INTEGER NOT NULL CHECK(terminal_attempt_count BETWEEN 1 AND 64),
    requested_at_ms INTEGER NOT NULL CHECK(requested_at_ms >= 0),
    PRIMARY KEY (tenant_id, replay_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, requested_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, outbox_event_id)
        REFERENCES outbox_events(tenant_id, outbox_event_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE notification_replay_dispatches (
    tenant_id TEXT NOT NULL,
    replay_id TEXT NOT NULL,
    dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('PENDING', 'PUBLISHED')),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    published_at_ms INTEGER CHECK(
        published_at_ms IS NULL OR published_at_ms >= created_at_ms
    ),
    PRIMARY KEY (tenant_id, replay_id),
    FOREIGN KEY (tenant_id, replay_id)
        REFERENCES notification_replay_intents(tenant_id, replay_id) ON DELETE RESTRICT,
    CHECK(
        (dispatch_state = 'PENDING' AND published_at_ms IS NULL)
        OR (dispatch_state = 'PUBLISHED' AND published_at_ms IS NOT NULL)
    )
) STRICT;

CREATE TRIGGER notification_replay_intent_validate
BEFORE INSERT ON notification_replay_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_owner_required')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.requested_by_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'notification_replay_dead_letter_required')
    WHERE NOT EXISTS (
        SELECT 1
        FROM notification_deliveries
        WHERE tenant_id = NEW.tenant_id
          AND consumer_id = NEW.consumer_id
          AND outbox_event_id = NEW.outbox_event_id
          AND delivery_state = 'DEAD_LETTER'
          AND attempt_count = NEW.terminal_attempt_count
          AND terminal_at_ms IS NOT NULL
          AND terminal_at_ms <= NEW.requested_at_ms
    );
END;

CREATE TRIGGER notification_replay_intent_apply
AFTER INSERT ON notification_replay_intents
FOR EACH ROW
BEGIN
    INSERT INTO audit_events (
        tenant_id,
        audit_event_id,
        correlation_id,
        actor_id,
        action,
        resource_type,
        resource_id,
        result_code,
        occurred_at_ms
    ) VALUES (
        NEW.tenant_id,
        NEW.audit_event_id,
        NEW.correlation_id,
        NEW.requested_by_actor_id,
        'notification.replay.requested',
        'notification_delivery',
        NEW.outbox_event_id,
        'prepared',
        NEW.requested_at_ms
    );

    INSERT INTO notification_replay_dispatches (
        tenant_id, replay_id, dispatch_state, created_at_ms, published_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.replay_id, 'PENDING', NEW.requested_at_ms, NULL
    );

    UPDATE notification_deliveries
    SET delivery_state = 'READY',
        attempt_count = 0,
        last_attempt_at_ms = NULL,
        next_attempt_at_ms = NULL,
        delivered_at_ms = NULL,
        terminal_at_ms = NULL,
        failure_class = NULL,
        updated_at_ms = NEW.requested_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND consumer_id = NEW.consumer_id
      AND outbox_event_id = NEW.outbox_event_id
      AND delivery_state = 'DEAD_LETTER'
      AND attempt_count = NEW.terminal_attempt_count;
END;

CREATE TRIGGER notification_replay_intent_immutable_update
BEFORE UPDATE ON notification_replay_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_intent_immutable');
END;

CREATE TRIGGER notification_replay_intent_immutable_delete
BEFORE DELETE ON notification_replay_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_intent_immutable');
END;

CREATE TRIGGER notification_replay_audit_immutable_update
BEFORE UPDATE ON audit_events
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM notification_replay_intents
    WHERE tenant_id = OLD.tenant_id AND audit_event_id = OLD.audit_event_id
)
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_audit_immutable');
END;

CREATE TRIGGER notification_replay_audit_immutable_delete
BEFORE DELETE ON audit_events
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM notification_replay_intents
    WHERE tenant_id = OLD.tenant_id AND audit_event_id = OLD.audit_event_id
)
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_audit_immutable');
END;

CREATE TRIGGER notification_replay_dispatch_transition_guard
BEFORE UPDATE ON notification_replay_dispatches
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_dispatch_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id OR NEW.replay_id <> OLD.replay_id;

    SELECT RAISE(ABORT, 'notification_replay_dispatch_transition_invalid')
    WHERE OLD.dispatch_state <> 'PENDING'
       OR NEW.dispatch_state <> 'PUBLISHED'
       OR NEW.published_at_ms IS NULL
       OR NEW.published_at_ms < OLD.created_at_ms;
END;

CREATE TRIGGER notification_replay_dispatch_delete_guard
BEFORE DELETE ON notification_replay_dispatches
FOR EACH ROW
WHEN OLD.dispatch_state <> 'PUBLISHED'
BEGIN
    SELECT RAISE(ABORT, 'notification_replay_dispatch_not_compactable');
END;

CREATE TRIGGER notification_delivery_transition_guard
BEFORE UPDATE ON notification_deliveries
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_delivery_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.consumer_id <> OLD.consumer_id
       OR NEW.outbox_event_id <> OLD.outbox_event_id;

    SELECT RAISE(ABORT, 'notification_delivery_terminal_immutable')
    WHERE OLD.delivery_state = 'DELIVERED'
       OR (
            OLD.delivery_state = 'DEAD_LETTER'
            AND NOT (
                NEW.delivery_state = 'READY'
                AND NEW.attempt_count = 0
                AND EXISTS (
                    SELECT 1
                    FROM notification_replay_intents AS intent
                    JOIN notification_replay_dispatches AS dispatch
                      ON dispatch.tenant_id = intent.tenant_id
                     AND dispatch.replay_id = intent.replay_id
                     AND dispatch.dispatch_state = 'PENDING'
                    WHERE intent.tenant_id = OLD.tenant_id
                      AND intent.consumer_id = OLD.consumer_id
                      AND intent.outbox_event_id = OLD.outbox_event_id
                      AND intent.terminal_attempt_count = OLD.attempt_count
                      AND intent.requested_at_ms = NEW.updated_at_ms
                )
            )
       );

    SELECT RAISE(ABORT, 'notification_delivery_attempt_sequence_invalid')
    WHERE NOT (OLD.delivery_state = 'DEAD_LETTER' AND NEW.delivery_state = 'READY')
      AND NEW.attempt_count <> OLD.attempt_count + 1;

    SELECT RAISE(ABORT, 'notification_delivery_transition_invalid')
    WHERE NEW.delivery_state = 'READY' AND OLD.delivery_state <> 'DEAD_LETTER';

    SELECT RAISE(ABORT, 'notification_delivery_attempt_time_invalid')
    WHERE NEW.delivery_state <> 'READY'
      AND (
        NEW.last_attempt_at_ms IS NULL
        OR NEW.last_attempt_at_ms < OLD.created_at_ms
        OR (
            OLD.delivery_state = 'RETRY_SCHEDULED'
            AND NEW.last_attempt_at_ms < OLD.next_attempt_at_ms
        )
      );
END;

CREATE TRIGGER notification_delivery_delete_guard
BEFORE DELETE ON notification_deliveries
FOR EACH ROW
WHEN OLD.delivery_state <> 'DELIVERED'
BEGIN
    SELECT RAISE(ABORT, 'notification_delivery_not_compactable');
END;

CREATE INDEX notification_deliveries_due
    ON notification_deliveries(
        delivery_state,
        next_attempt_at_ms,
        updated_at_ms,
        tenant_id,
        outbox_event_id
    );

CREATE INDEX notification_deliveries_tenant_state
    ON notification_deliveries(tenant_id, delivery_state, updated_at_ms, outbox_event_id);

CREATE INDEX notification_replay_dispatch_pending
    ON notification_replay_dispatches(dispatch_state, created_at_ms, tenant_id, replay_id);

CREATE TABLE user_event_cursors (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL
        CHECK(length(actor_id) BETWEEN 8 AND 96)
        CHECK(actor_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    occurred_at_ms INTEGER NOT NULL CHECK(occurred_at_ms >= 0),
    outbox_event_id TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    PRIMARY KEY (tenant_id, actor_id),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, outbox_event_id)
        REFERENCES outbox_events(tenant_id, outbox_event_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER user_event_cursor_source_guard_insert
BEFORE INSERT ON user_event_cursors
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'user_event_cursor_membership_not_active')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'user_event_cursor_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE tenant_id = NEW.tenant_id
          AND outbox_event_id = NEW.outbox_event_id
          AND created_at_ms = NEW.occurred_at_ms
    );
END;

CREATE TRIGGER user_event_cursor_guard_update
BEFORE UPDATE ON user_event_cursors
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'user_event_cursor_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id OR NEW.actor_id <> OLD.actor_id;

    SELECT RAISE(ABORT, 'user_event_cursor_membership_not_active')
    WHERE NOT EXISTS (
        SELECT 1
        FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.actor_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'user_event_cursor_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE tenant_id = NEW.tenant_id
          AND outbox_event_id = NEW.outbox_event_id
          AND created_at_ms = NEW.occurred_at_ms
    );

    SELECT RAISE(ABORT, 'user_event_cursor_rewind')
    WHERE NEW.occurred_at_ms < OLD.occurred_at_ms
       OR (
            NEW.occurred_at_ms = OLD.occurred_at_ms
            AND NEW.outbox_event_id < OLD.outbox_event_id
       );

    SELECT RAISE(ABORT, 'user_event_cursor_updated_at_rewind')
    WHERE NEW.updated_at_ms < OLD.updated_at_ms;
END;

CREATE TRIGGER user_event_cursor_delete_guard
BEFORE DELETE ON user_event_cursors
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM memberships
    WHERE tenant_id = OLD.tenant_id
      AND actor_id = OLD.actor_id
      AND status = 'ACTIVE'
)
BEGIN
    SELECT RAISE(ABORT, 'active_user_event_cursor_not_compactable');
END;

CREATE INDEX user_event_cursors_event_lookup
    ON user_event_cursors(tenant_id, occurred_at_ms, outbox_event_id, actor_id);
