-- Phase 1A: version the existing durable outbox and add duplicate-neutral consumer claims.
-- Retry/backoff/DLQ/catch-up are intentionally deferred to Phase 1B.

ALTER TABLE outbox_events
    ADD COLUMN envelope_version INTEGER NOT NULL DEFAULT 1
        CHECK(envelope_version = 1);

ALTER TABLE outbox_events
    ADD COLUMN event_version INTEGER NOT NULL DEFAULT 1
        CHECK(event_version BETWEEN 1 AND 65535);

CREATE TRIGGER outbox_event_payload_guard
BEFORE INSERT ON outbox_events
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbox_payload_invalid')
    WHERE length(NEW.payload_json) > 4096
       OR CASE
            WHEN json_valid(NEW.payload_json) = 1 THEN EXISTS (
                SELECT 1
                FROM json_tree(NEW.payload_json)
                WHERE json_tree.key IS NOT NULL
                  AND lower(CAST(json_tree.key AS TEXT)) IN (
                      'access_token',
                      'authorization',
                      'body_html',
                      'cookie',
                      'cookies',
                      'credential',
                      'display_name',
                      'email',
                      'mail_body',
                      'message_body',
                      'oauth_token',
                      'password',
                      'phone',
                      'proxy_credentials',
                      'raw_message',
                      'recipient',
                      'refresh_token',
                      'secret',
                      'secret_handle',
                      'sender',
                      'snippet',
                      'subject'
                  )
            )
            ELSE 0
          END;
END;

CREATE TRIGGER outbox_event_version_guard
BEFORE INSERT ON outbox_events
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbox_event_version_mismatch')
    WHERE NEW.event_type NOT LIKE ('%.v' || CAST(NEW.event_version AS TEXT));
END;

CREATE TABLE notification_events (
    tenant_id TEXT NOT NULL,
    outbox_event_id TEXT NOT NULL,
    envelope_version INTEGER NOT NULL CHECK(envelope_version = 1),
    aggregate_type TEXT NOT NULL CHECK(length(trim(aggregate_type)) BETWEEN 1 AND 64),
    aggregate_id TEXT NOT NULL CHECK(length(aggregate_id) BETWEEN 8 AND 96),
    aggregate_version INTEGER NOT NULL CHECK(aggregate_version >= 1),
    event_type TEXT NOT NULL CHECK(length(trim(event_type)) BETWEEN 1 AND 160),
    event_version INTEGER NOT NULL CHECK(event_version BETWEEN 1 AND 65535),
    payload_json TEXT NOT NULL
        CHECK(json_valid(payload_json))
        CHECK(length(payload_json) <= 4096),
    occurred_at_ms INTEGER NOT NULL CHECK(occurred_at_ms >= 0),
    persisted_at_ms INTEGER NOT NULL CHECK(persisted_at_ms >= 0),
    PRIMARY KEY (tenant_id, outbox_event_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, outbox_event_id)
        REFERENCES outbox_events(tenant_id, outbox_event_id) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER notification_event_source_guard
BEFORE INSERT ON notification_events
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'notification_event_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE tenant_id = NEW.tenant_id
          AND outbox_event_id = NEW.outbox_event_id
          AND envelope_version = NEW.envelope_version
          AND aggregate_type = NEW.aggregate_type
          AND aggregate_id = NEW.aggregate_id
          AND aggregate_version = NEW.aggregate_version
          AND event_type = NEW.event_type
          AND event_version = NEW.event_version
          AND payload_json = NEW.payload_json
          AND created_at_ms = NEW.occurred_at_ms
    );
END;

CREATE INDEX notification_events_tenant_time
    ON notification_events(tenant_id, occurred_at_ms DESC, outbox_event_id DESC);

CREATE TABLE consumer_idempotency (
    tenant_id TEXT NOT NULL,
    consumer_id TEXT NOT NULL
        CHECK(length(consumer_id) BETWEEN 8 AND 96)
        CHECK(consumer_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    outbox_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(length(trim(event_type)) BETWEEN 1 AND 160),
    event_version INTEGER NOT NULL CHECK(event_version BETWEEN 1 AND 65535),
    consumed_at_ms INTEGER NOT NULL CHECK(consumed_at_ms >= 0),
    PRIMARY KEY (tenant_id, consumer_id, outbox_event_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, outbox_event_id)
        REFERENCES outbox_events(tenant_id, outbox_event_id) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER consumer_idempotency_source_guard
BEFORE INSERT ON consumer_idempotency
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'consumer_event_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE tenant_id = NEW.tenant_id
          AND outbox_event_id = NEW.outbox_event_id
          AND event_type = NEW.event_type
          AND event_version = NEW.event_version
    );
END;

CREATE INDEX consumer_idempotency_event_lookup
    ON consumer_idempotency(tenant_id, outbox_event_id, consumer_id);
