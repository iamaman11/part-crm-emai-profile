-- Pre-2J C4: provider-neutral outbound mail intent/retry coordination.
--
-- Message content is deliberately absent. Durable state contains only bounded
-- routing/idempotency metadata required to suppress duplicate provider effects
-- and reconcile uncertain outcomes.

CREATE TABLE outbound_mail_intents (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL
        CHECK(length(intent_id) BETWEEN 8 AND 96)
        CHECK(intent_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    request_digest TEXT NOT NULL CHECK(length(request_digest) BETWEEN 16 AND 256),
    client_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('NEW', 'REPLY', 'REPLY_ALL', 'FORWARD')),
    state TEXT NOT NULL CHECK(state IN (
        'PENDING', 'DISPATCHING', 'RETRYABLE', 'SENT', 'AMBIGUOUS', 'REJECTED'
    )),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count BETWEEN 0 AND 10),
    provider_message_reference TEXT
        CHECK(provider_message_reference IS NULL OR (
            length(provider_message_reference) BETWEEN 1 AND 512
        )),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, intent_id),
    UNIQUE (tenant_id, command_actor_id, idempotency_key),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX outbound_mail_intent_state_lookup
    ON outbound_mail_intents(tenant_id, state, updated_at_ms, intent_id);

CREATE TRIGGER outbound_mail_intent_validate_access
BEFORE INSERT ON outbound_mail_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_access_denied')
    WHERE NOT EXISTS (
        SELECT 1
        FROM clients AS client
        JOIN mailbox_bindings AS binding
          ON binding.tenant_id = client.tenant_id
         AND binding.binding_id = NEW.binding_id
        JOIN mailbox_client_association_state AS association
          ON association.tenant_id = binding.tenant_id
         AND association.binding_id = binding.binding_id
         AND association.client_id = client.client_id
        WHERE client.tenant_id = NEW.tenant_id
          AND client.client_id = NEW.client_id
          AND client.status = 'ACTIVE'
          AND binding.status = 'ACTIVE'
          AND binding.execution_status = 'ACTIVE'
          AND binding.provider IN ('GMAIL_API', 'IMAP', 'MICROSOFT_GRAPH')
          AND EXISTS (
              SELECT 1
              FROM memberships AS requester
              WHERE requester.tenant_id = client.tenant_id
                AND requester.actor_id = NEW.command_actor_id
                AND requester.status = 'ACTIVE'
                AND (
                    requester.role = 'TENANT_OWNER'
                    OR (
                        requester.role = 'MEMBER'
                        AND EXISTS (
                            SELECT 1
                            FROM client_grants AS grant_row
                            WHERE grant_row.tenant_id = client.tenant_id
                              AND grant_row.actor_id = requester.actor_id
                              AND grant_row.client_id = client.client_id
                        )
                    )
                )
          )
    );
END;

CREATE TABLE outbound_mail_dispatch_claims (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 10),
    claimed_at_ms INTEGER NOT NULL CHECK(claimed_at_ms >= 0),
    PRIMARY KEY (tenant_id, intent_id, attempt),
    FOREIGN KEY (tenant_id, intent_id)
        REFERENCES outbound_mail_intents(tenant_id, intent_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER outbound_mail_dispatch_claim_validate
BEFORE INSERT ON outbound_mail_dispatch_claims
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_claim_state_invalid')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbound_mail_intents
        WHERE tenant_id = NEW.tenant_id
          AND intent_id = NEW.intent_id
          AND state IN ('PENDING', 'RETRYABLE')
          AND attempt_count + 1 = NEW.attempt
    );

    SELECT RAISE(ABORT, 'outbound_mail_access_denied')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbound_mail_intents AS intent
        JOIN clients AS client
          ON client.tenant_id = intent.tenant_id
         AND client.client_id = intent.client_id
        JOIN mailbox_bindings AS binding
          ON binding.tenant_id = intent.tenant_id
         AND binding.binding_id = intent.binding_id
        JOIN mailbox_client_association_state AS association
          ON association.tenant_id = binding.tenant_id
         AND association.binding_id = binding.binding_id
         AND association.client_id = client.client_id
        WHERE intent.tenant_id = NEW.tenant_id
          AND intent.intent_id = NEW.intent_id
          AND client.status = 'ACTIVE'
          AND binding.status = 'ACTIVE'
          AND binding.execution_status = 'ACTIVE'
          AND binding.provider IN ('GMAIL_API', 'IMAP', 'MICROSOFT_GRAPH')
          AND EXISTS (
              SELECT 1
              FROM memberships AS requester
              WHERE requester.tenant_id = intent.tenant_id
                AND requester.actor_id = intent.command_actor_id
                AND requester.status = 'ACTIVE'
                AND (
                    requester.role = 'TENANT_OWNER'
                    OR (
                        requester.role = 'MEMBER'
                        AND EXISTS (
                            SELECT 1
                            FROM client_grants AS grant_row
                            WHERE grant_row.tenant_id = intent.tenant_id
                              AND grant_row.actor_id = requester.actor_id
                              AND grant_row.client_id = intent.client_id
                        )
                    )
                )
          )
    );
END;

CREATE TRIGGER outbound_mail_dispatch_claim_apply
AFTER INSERT ON outbound_mail_dispatch_claims
FOR EACH ROW
BEGIN
    UPDATE outbound_mail_intents
    SET state = 'DISPATCHING',
        attempt_count = NEW.attempt,
        updated_at_ms = NEW.claimed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND intent_id = NEW.intent_id;
END;

CREATE TABLE outbound_mail_dispatch_completions (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 10),
    outcome TEXT NOT NULL CHECK(outcome IN ('SENT', 'RETRYABLE', 'REJECTED', 'AMBIGUOUS')),
    provider_message_reference TEXT
        CHECK(provider_message_reference IS NULL OR (
            length(provider_message_reference) BETWEEN 1 AND 512
        )),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    PRIMARY KEY (tenant_id, intent_id, attempt),
    FOREIGN KEY (tenant_id, intent_id, attempt)
        REFERENCES outbound_mail_dispatch_claims(tenant_id, intent_id, attempt) ON DELETE RESTRICT,
    CHECK(outcome = 'SENT' OR provider_message_reference IS NULL)
) STRICT;

CREATE TRIGGER outbound_mail_dispatch_completion_validate
BEFORE INSERT ON outbound_mail_dispatch_completions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_completion_state_invalid')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbound_mail_intents
        WHERE tenant_id = NEW.tenant_id
          AND intent_id = NEW.intent_id
          AND attempt_count = NEW.attempt
          AND state IN ('DISPATCHING', 'AMBIGUOUS')
    );
END;

CREATE TRIGGER outbound_mail_dispatch_completion_apply
AFTER INSERT ON outbound_mail_dispatch_completions
FOR EACH ROW
BEGIN
    UPDATE outbound_mail_intents
    SET state = NEW.outcome,
        provider_message_reference = CASE
            WHEN NEW.outcome = 'SENT' THEN NEW.provider_message_reference
            ELSE NULL
        END,
        updated_at_ms = NEW.completed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND intent_id = NEW.intent_id;
END;

CREATE TABLE outbound_mail_ambiguity_marks (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 10),
    marked_at_ms INTEGER NOT NULL CHECK(marked_at_ms >= 0),
    PRIMARY KEY (tenant_id, intent_id, attempt),
    FOREIGN KEY (tenant_id, intent_id, attempt)
        REFERENCES outbound_mail_dispatch_claims(tenant_id, intent_id, attempt) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER outbound_mail_ambiguity_mark_validate
BEFORE INSERT ON outbound_mail_ambiguity_marks
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_ambiguity_state_invalid')
    WHERE NOT EXISTS (
        SELECT 1
        FROM outbound_mail_intents
        WHERE tenant_id = NEW.tenant_id
          AND intent_id = NEW.intent_id
          AND attempt_count = NEW.attempt
          AND state = 'DISPATCHING'
    );
END;

CREATE TRIGGER outbound_mail_ambiguity_mark_apply
AFTER INSERT ON outbound_mail_ambiguity_marks
FOR EACH ROW
BEGIN
    UPDATE outbound_mail_intents
    SET state = 'AMBIGUOUS',
        updated_at_ms = NEW.marked_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND intent_id = NEW.intent_id;
END;
