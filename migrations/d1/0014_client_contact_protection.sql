-- Phase 2B: authoritative protected client-contact persistence.
-- Raw contact display values are never stored in D1. Exact lookup uses tenant-scoped,
-- domain-separated HMAC tokens produced outside the database.

CREATE TABLE client_contact_points (
    tenant_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    contact_point_id TEXT NOT NULL
        CHECK(length(contact_point_id) BETWEEN 8 AND 96)
        CHECK(contact_point_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    kind TEXT NOT NULL CHECK(kind IN ('EMAIL', 'PHONE', 'URL')),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'ARCHIVED')),
    normalization_version INTEGER NOT NULL CHECK(normalization_version BETWEEN 1 AND 65535),
    protection_version INTEGER NOT NULL CHECK(protection_version BETWEEN 1 AND 65535),
    ciphertext BLOB NOT NULL CHECK(length(ciphertext) BETWEEN 1 AND 4096),
    nonce BLOB NOT NULL CHECK(length(nonce) BETWEEN 1 AND 64),
    encryption_key_version INTEGER NOT NULL CHECK(encryption_key_version BETWEEN 1 AND 2147483647),
    exact_lookup_token BLOB NOT NULL CHECK(length(exact_lookup_token) = 32),
    lookup_key_version INTEGER NOT NULL CHECK(lookup_key_version BETWEEN 1 AND 2147483647),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, contact_point_id),
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX client_contact_exact_lookup
    ON client_contact_points(
        tenant_id,
        kind,
        normalization_version,
        lookup_key_version,
        exact_lookup_token,
        contact_point_id
    )
    WHERE status = 'ACTIVE';

CREATE INDEX client_contact_client_history
    ON client_contact_points(tenant_id, client_id, status, updated_at_ms, contact_point_id);

CREATE TRIGGER client_contact_active_client_insert_guard
BEFORE INSERT ON client_contact_points
FOR EACH ROW
WHEN NEW.status = 'ACTIVE'
 AND NOT EXISTS (
    SELECT 1
    FROM clients
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id
      AND status = 'ACTIVE'
)
BEGIN
    SELECT RAISE(ABORT, 'client_contact_client_not_active');
END;

CREATE TRIGGER client_contact_update_guard
BEFORE UPDATE ON client_contact_points
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_contact_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.client_id <> OLD.client_id
       OR NEW.contact_point_id <> OLD.contact_point_id
       OR NEW.kind <> OLD.kind
       OR NEW.created_by_actor_id <> OLD.created_by_actor_id
       OR NEW.created_at_ms <> OLD.created_at_ms;

    SELECT RAISE(ABORT, 'client_contact_archived_immutable')
    WHERE OLD.status = 'ARCHIVED' AND NEW.status <> 'ARCHIVED';

    SELECT RAISE(ABORT, 'client_contact_client_not_active')
    WHERE NEW.status = 'ACTIVE'
      AND NOT EXISTS (
        SELECT 1
        FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
      );

    SELECT RAISE(ABORT, 'client_contact_updated_at_rewind')
    WHERE NEW.updated_at_ms < OLD.updated_at_ms;
END;

CREATE TRIGGER client_contact_delete_guard
BEFORE DELETE ON client_contact_points
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_contact_delete_forbidden');
END;

-- Command-intent rows make authorization/version/state failures transaction-fatal before
-- idempotency/audit/outbox can commit. The rows contain no contact plaintext/ciphertext/token.
CREATE TABLE client_lifecycle_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('UPDATE', 'ARCHIVE')),
    expected_client_version INTEGER NOT NULL CHECK(expected_client_version >= 1),
    next_display_name TEXT,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    CHECK(
        (operation = 'UPDATE'
         AND next_display_name IS NOT NULL
         AND length(trim(next_display_name)) BETWEEN 1 AND 200)
        OR
        (operation = 'ARCHIVE' AND next_display_name IS NULL)
    )
) STRICT;

CREATE TRIGGER client_lifecycle_command_validate
BEFORE INSERT ON client_lifecycle_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_lifecycle_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_lifecycle_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
          AND version = NEW.expected_client_version
    );
    SELECT RAISE(ABORT, 'client_lifecycle_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND updated_at_ms > NEW.executed_at_ms
    );
END;

CREATE TRIGGER client_lifecycle_command_apply_update
AFTER INSERT ON client_lifecycle_commands
FOR EACH ROW
WHEN NEW.operation = 'UPDATE'
BEGIN
    UPDATE clients
    SET display_name = NEW.next_display_name,
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id;
END;

CREATE TRIGGER client_lifecycle_command_apply_archive
AFTER INSERT ON client_lifecycle_commands
FOR EACH ROW
WHEN NEW.operation = 'ARCHIVE'
BEGIN
    UPDATE clients
    SET status = 'ARCHIVED',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id;
    UPDATE client_contact_points
    SET status = 'ARCHIVED',
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id
      AND status = 'ACTIVE';
END;

CREATE TABLE client_contact_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    contact_point_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('UPSERT', 'ARCHIVE')),
    kind TEXT NOT NULL CHECK(kind IN ('EMAIL', 'PHONE', 'URL')),
    expected_client_version INTEGER NOT NULL CHECK(expected_client_version >= 1),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER client_contact_command_validate
BEFORE INSERT ON client_contact_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_contact_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_contact_client_version_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND status = 'ACTIVE'
          AND version = NEW.expected_client_version
    );
    SELECT RAISE(ABORT, 'client_contact_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.client_id
          AND updated_at_ms > NEW.executed_at_ms
    );
    SELECT RAISE(ABORT, 'client_contact_identity_mismatch')
    WHERE EXISTS (
        SELECT 1 FROM client_contact_points
        WHERE tenant_id = NEW.tenant_id
          AND contact_point_id = NEW.contact_point_id
          AND (client_id <> NEW.client_id OR kind <> NEW.kind)
    );
    SELECT RAISE(ABORT, 'client_contact_archived_immutable')
    WHERE NEW.operation = 'UPSERT'
      AND EXISTS (
        SELECT 1 FROM client_contact_points
        WHERE tenant_id = NEW.tenant_id
          AND contact_point_id = NEW.contact_point_id
          AND status = 'ARCHIVED'
    );
    SELECT RAISE(ABORT, 'client_contact_missing')
    WHERE NEW.operation = 'ARCHIVE'
      AND NOT EXISTS (
        SELECT 1 FROM client_contact_points
        WHERE tenant_id = NEW.tenant_id
          AND contact_point_id = NEW.contact_point_id
          AND client_id = NEW.client_id
          AND kind = NEW.kind
          AND status = 'ACTIVE'
    );
END;

CREATE TRIGGER client_contact_command_apply
AFTER INSERT ON client_contact_commands
FOR EACH ROW
BEGIN
    UPDATE clients
    SET version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id;
END;
