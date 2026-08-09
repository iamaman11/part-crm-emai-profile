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
