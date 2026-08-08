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
       OR substr(trim(NEW.payload_json), 1, 1) <> '{'
       OR substr(trim(NEW.payload_json), -1, 1) <> '}'
       OR instr(lower(NEW.payload_json), '"access_token"') > 0
       OR instr(lower(NEW.payload_json), '"authorization"') > 0
       OR instr(lower(NEW.payload_json), '"body_html"') > 0
       OR instr(lower(NEW.payload_json), '"cookie"') > 0
       OR instr(lower(NEW.payload_json), '"cookies"') > 0
       OR instr(lower(NEW.payload_json), '"credential"') > 0
       OR instr(lower(NEW.payload_json), '"display_name"') > 0
       OR instr(lower(NEW.payload_json), '"email"') > 0
       OR instr(lower(NEW.payload_json), '"mail_body"') > 0
       OR instr(lower(NEW.payload_json), '"message_body"') > 0
       OR instr(lower(NEW.payload_json), '"oauth_token"') > 0
       OR instr(lower(NEW.payload_json), '"password"') > 0
       OR instr(lower(NEW.payload_json), '"phone"') > 0
       OR instr(lower(NEW.payload_json), '"proxy_credentials"') > 0
       OR instr(lower(NEW.payload_json), '"raw_message"') > 0
       OR instr(lower(NEW.payload_json), '"recipient"') > 0
       OR instr(lower(NEW.payload_json), '"refresh_token"') > 0
       OR instr(lower(NEW.payload_json), '"secret"') > 0
       OR instr(lower(NEW.payload_json), '"secret_handle"') > 0
       OR instr(lower(NEW.payload_json), '"sender"') > 0
       OR instr(lower(NEW.payload_json), '"snippet"') > 0
       OR instr(lower(NEW.payload_json), '"subject"') > 0;
END;

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

CREATE INDEX consumer_idempotency_event_lookup
    ON consumer_idempotency(tenant_id, outbox_event_id, consumer_id);
