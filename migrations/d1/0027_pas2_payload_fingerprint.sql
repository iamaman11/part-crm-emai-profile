-- PAS-2 Transaction B: retire the browser-supplied request digest trust boundary.
--
-- Historical request_digest bytes are deliberately NOT reclassified or copied into
-- payload_fingerprint. Existing idempotency keys and outbound intents are retained as
-- fail-closed tombstones, while every new active row must carry a server-owned SHA-256
-- fingerprint produced after typed command decoding.

ALTER TABLE idempotency_records
    RENAME TO idempotency_records_pas2_legacy;

CREATE TABLE idempotency_records (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_name TEXT NOT NULL CHECK(length(trim(command_name)) BETWEEN 1 AND 120),
    payload_fingerprint TEXT
        CHECK(
            payload_fingerprint IS NULL
            OR (
                length(payload_fingerprint) = 64
                AND payload_fingerprint NOT GLOB '*[^0-9a-f]*'
            )
        ),
    result_code TEXT NOT NULL CHECK(length(trim(result_code)) BETWEEN 1 AND 120),
    result_reference TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, actor_id, idempotency_key),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO idempotency_records (
    tenant_id,
    actor_id,
    idempotency_key,
    command_name,
    payload_fingerprint,
    result_code,
    result_reference,
    created_at_ms,
    expires_at_ms
)
SELECT
    tenant_id,
    actor_id,
    idempotency_key,
    command_name,
    NULL,
    result_code,
    result_reference,
    created_at_ms,
    expires_at_ms
FROM idempotency_records_pas2_legacy;

DROP TABLE idempotency_records_pas2_legacy;

-- NULL is reserved exclusively for rows migrated from the retired trust boundary.
-- New commands must always persist a typed server-owned fingerprint, and a tombstone
-- can never be upgraded later into a replayable trusted row.
CREATE TRIGGER idempotency_payload_fingerprint_required
BEFORE INSERT ON idempotency_records
FOR EACH ROW
WHEN NEW.payload_fingerprint IS NULL
BEGIN
    SELECT RAISE(ABORT, 'idempotency_payload_fingerprint_required');
END;

CREATE TRIGGER idempotency_payload_fingerprint_immutable
BEFORE UPDATE OF payload_fingerprint ON idempotency_records
FOR EACH ROW
WHEN OLD.payload_fingerprint IS NULL
   OR NEW.payload_fingerprint IS NULL
   OR NEW.payload_fingerprint <> OLD.payload_fingerprint
BEGIN
    SELECT RAISE(ABORT, 'idempotency_payload_fingerprint_immutable');
END;

-- outbound_mail_intents carried a second browser-derived request_digest copy. Rebuild
-- the whole FK family so the legacy column is physically removed without weakening the
-- existing dispatch/access constraints. Historical intents retain NULL tombstones; they
-- remain recoverable by intent id/state but cannot establish command replay equivalence.
DROP TRIGGER outbound_mail_intent_validate_access;
DROP TRIGGER outbound_mail_dispatch_claim_validate;
DROP TRIGGER outbound_mail_dispatch_claim_apply;
DROP TRIGGER outbound_mail_dispatch_completion_validate;
DROP TRIGGER outbound_mail_dispatch_completion_apply;
DROP TRIGGER outbound_mail_ambiguity_mark_validate;
DROP TRIGGER outbound_mail_ambiguity_mark_apply;
DROP INDEX outbound_mail_intent_state_lookup;

ALTER TABLE outbound_mail_ambiguity_marks
    RENAME TO outbound_mail_ambiguity_marks_pas2_legacy;
ALTER TABLE outbound_mail_dispatch_completions
    RENAME TO outbound_mail_dispatch_completions_pas2_legacy;
ALTER TABLE outbound_mail_dispatch_claims
    RENAME TO outbound_mail_dispatch_claims_pas2_legacy;
ALTER TABLE outbound_mail_intents
    RENAME TO outbound_mail_intents_pas2_legacy;

CREATE TABLE outbound_mail_intents (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL
        CHECK(length(intent_id) BETWEEN 8 AND 96)
        CHECK(intent_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    payload_fingerprint TEXT
        CHECK(
            payload_fingerprint IS NULL
            OR (
                length(payload_fingerprint) = 64
                AND payload_fingerprint NOT GLOB '*[^0-9a-f]*'
            )
        ),
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

INSERT INTO outbound_mail_intents (
    tenant_id,
    intent_id,
    command_actor_id,
    idempotency_key,
    payload_fingerprint,
    client_id,
    binding_id,
    operation,
    state,
    attempt_count,
    provider_message_reference,
    created_at_ms,
    updated_at_ms
)
SELECT
    tenant_id,
    intent_id,
    command_actor_id,
    idempotency_key,
    NULL,
    client_id,
    binding_id,
    operation,
    state,
    attempt_count,
    provider_message_reference,
    created_at_ms,
    updated_at_ms
FROM outbound_mail_intents_pas2_legacy;

CREATE INDEX outbound_mail_intent_state_lookup
    ON outbound_mail_intents(tenant_id, state, updated_at_ms, intent_id);

CREATE TABLE outbound_mail_dispatch_claims (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 10),
    claimed_at_ms INTEGER NOT NULL CHECK(claimed_at_ms >= 0),
    PRIMARY KEY (tenant_id, intent_id, attempt),
    FOREIGN KEY (tenant_id, intent_id)
        REFERENCES outbound_mail_intents(tenant_id, intent_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO outbound_mail_dispatch_claims (
    tenant_id, intent_id, attempt, claimed_at_ms
)
SELECT tenant_id, intent_id, attempt, claimed_at_ms
FROM outbound_mail_dispatch_claims_pas2_legacy;

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

INSERT INTO outbound_mail_dispatch_completions (
    tenant_id, intent_id, attempt, outcome, provider_message_reference, completed_at_ms
)
SELECT tenant_id, intent_id, attempt, outcome, provider_message_reference, completed_at_ms
FROM outbound_mail_dispatch_completions_pas2_legacy;

CREATE TABLE outbound_mail_ambiguity_marks (
    tenant_id TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 10),
    marked_at_ms INTEGER NOT NULL CHECK(marked_at_ms >= 0),
    PRIMARY KEY (tenant_id, intent_id, attempt),
    FOREIGN KEY (tenant_id, intent_id, attempt)
        REFERENCES outbound_mail_dispatch_claims(tenant_id, intent_id, attempt) ON DELETE RESTRICT
) STRICT;

INSERT INTO outbound_mail_ambiguity_marks (
    tenant_id, intent_id, attempt, marked_at_ms
)
SELECT tenant_id, intent_id, attempt, marked_at_ms
FROM outbound_mail_ambiguity_marks_pas2_legacy;

DROP TABLE outbound_mail_ambiguity_marks_pas2_legacy;
DROP TABLE outbound_mail_dispatch_completions_pas2_legacy;
DROP TABLE outbound_mail_dispatch_claims_pas2_legacy;
DROP TABLE outbound_mail_intents_pas2_legacy;

CREATE TRIGGER outbound_mail_payload_fingerprint_required
BEFORE INSERT ON outbound_mail_intents
FOR EACH ROW
WHEN NEW.payload_fingerprint IS NULL
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_payload_fingerprint_required');
END;

CREATE TRIGGER outbound_mail_payload_fingerprint_immutable
BEFORE UPDATE OF payload_fingerprint ON outbound_mail_intents
FOR EACH ROW
WHEN OLD.payload_fingerprint IS NULL
   OR NEW.payload_fingerprint IS NULL
   OR NEW.payload_fingerprint <> OLD.payload_fingerprint
BEGIN
    SELECT RAISE(ABORT, 'outbound_mail_payload_fingerprint_immutable');
END;

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
