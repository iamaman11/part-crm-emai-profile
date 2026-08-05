-- Repository Step 3: authoritative standalone catalog foundation.
-- D1 enables foreign keys by default. Composite tenant keys are deliberate.

CREATE TABLE tenants (
    tenant_id TEXT PRIMARY KEY
        CHECK(length(tenant_id) BETWEEN 8 AND 96)
        CHECK(tenant_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) BETWEEN 1 AND 200),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'SUSPENDED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms)
) STRICT;

CREATE TABLE identities (
    identity_id TEXT PRIMARY KEY
        CHECK(length(identity_id) BETWEEN 8 AND 96)
        CHECK(identity_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    access_subject TEXT NOT NULL UNIQUE CHECK(length(trim(access_subject)) BETWEEN 1 AND 512),
    verified_contact_hint TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0)
) STRICT;

CREATE TABLE memberships (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL
        CHECK(length(actor_id) BETWEEN 8 AND 96)
        CHECK(actor_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    identity_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('TENANT_OWNER', 'MEMBER')),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'SUSPENDED', 'REVOKED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, actor_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (identity_id) REFERENCES identities(identity_id) ON DELETE RESTRICT
) STRICT;

CREATE UNIQUE INDEX one_active_owner_per_tenant
    ON memberships(tenant_id)
    WHERE role = 'TENANT_OWNER' AND status = 'ACTIVE';

CREATE INDEX memberships_identity_lookup
    ON memberships(identity_id, tenant_id, status);

CREATE TABLE invitations (
    tenant_id TEXT NOT NULL,
    invitation_id TEXT NOT NULL
        CHECK(length(invitation_id) BETWEEN 8 AND 96)
        CHECK(invitation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    invited_contact_hmac TEXT NOT NULL CHECK(length(invited_contact_hmac) BETWEEN 16 AND 256),
    intended_role TEXT NOT NULL CHECK(intended_role IN ('MEMBER')),
    status TEXT NOT NULL CHECK(status IN ('PENDING', 'ACCEPTED', 'EXPIRED', 'REVOKED')),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
    created_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    PRIMARY KEY (tenant_id, invitation_id),
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE clients (
    tenant_id TEXT NOT NULL,
    client_id TEXT NOT NULL
        CHECK(length(client_id) BETWEEN 8 AND 96)
        CHECK(client_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    kind TEXT NOT NULL CHECK(kind IN ('PERSON', 'ORGANIZATION')),
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) BETWEEN 1 AND 200),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'ARCHIVED', 'MERGED')),
    version INTEGER NOT NULL CHECK(version >= 1),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, client_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX clients_status_lookup ON clients(tenant_id, status, client_id);

CREATE TABLE browser_profiles (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL
        CHECK(length(profile_id) BETWEEN 8 AND 96)
        CHECK(profile_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    status TEXT NOT NULL CHECK(status IN (
        'DRAFT', 'QUARANTINED', 'READY', 'IN_USE', 'DIRTY_LOCAL',
        'SYNCING', 'SUSPENDED', 'DELETING', 'DELETED'
    )),
    active_generation_id TEXT,
    version INTEGER NOT NULL CHECK(version >= 1),
    created_by_actor_id TEXT NOT NULL,
    updated_by_actor_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, profile_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, created_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX profiles_status_lookup ON browser_profiles(tenant_id, status, profile_id);

CREATE TABLE profile_client_assignments (
    tenant_id TEXT NOT NULL,
    assignment_id TEXT NOT NULL
        CHECK(length(assignment_id) BETWEEN 8 AND 96)
        CHECK(assignment_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    profile_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    assigned_by_actor_id TEXT NOT NULL,
    assigned_at_ms INTEGER NOT NULL CHECK(assigned_at_ms >= 0),
    closed_at_ms INTEGER CHECK(closed_at_ms IS NULL OR closed_at_ms >= assigned_at_ms),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    PRIMARY KEY (tenant_id, assignment_id),
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, assigned_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE UNIQUE INDEX one_active_primary_assignment_per_profile
    ON profile_client_assignments(tenant_id, profile_id)
    WHERE closed_at_ms IS NULL;

CREATE INDEX assignments_client_history
    ON profile_client_assignments(tenant_id, client_id, assigned_at_ms, assignment_id);

CREATE TRIGGER active_client_required_for_assignment
BEFORE INSERT ON profile_client_assignments
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1
    FROM clients
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.client_id
      AND status = 'ACTIVE'
)
BEGIN
    SELECT RAISE(ABORT, 'assignment_client_not_active');
END;

CREATE TABLE profile_grants (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('PROFILE_VIEWER', 'PROFILE_OPERATOR')),
    granted_by_actor_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    PRIMARY KEY (tenant_id, actor_id, profile_id),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, profile_id)
        REFERENCES browser_profiles(tenant_id, profile_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, granted_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE client_grants (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('CLIENT_VIEWER', 'CLIENT_EDITOR')),
    granted_by_actor_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    PRIMARY KEY (tenant_id, actor_id, client_id),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, granted_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER active_membership_required_for_profile_grant
BEFORE INSERT ON profile_grants
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1
    FROM memberships
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND status = 'ACTIVE'
)
BEGIN
    SELECT RAISE(ABORT, 'profile_grant_membership_not_active');
END;

CREATE TRIGGER active_membership_required_for_client_grant
BEFORE INSERT ON client_grants
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1
    FROM memberships
    WHERE tenant_id = NEW.tenant_id
      AND actor_id = NEW.actor_id
      AND status = 'ACTIVE'
)
BEGIN
    SELECT RAISE(ABORT, 'client_grant_membership_not_active');
END;

CREATE TABLE idempotency_records (
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK(length(idempotency_key) BETWEEN 8 AND 96)
        CHECK(idempotency_key NOT GLOB '*[^A-Za-z0-9_-]*'),
    command_name TEXT NOT NULL CHECK(length(trim(command_name)) BETWEEN 1 AND 120),
    request_digest TEXT NOT NULL CHECK(length(request_digest) BETWEEN 16 AND 256),
    result_code TEXT NOT NULL CHECK(length(trim(result_code)) BETWEEN 1 AND 120),
    result_reference TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= created_at_ms),
    PRIMARY KEY (tenant_id, actor_id, idempotency_key),
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE audit_events (
    tenant_id TEXT NOT NULL,
    audit_event_id TEXT NOT NULL
        CHECK(length(audit_event_id) BETWEEN 8 AND 96)
        CHECK(audit_event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    correlation_id TEXT NOT NULL
        CHECK(length(correlation_id) BETWEEN 8 AND 96)
        CHECK(correlation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(length(trim(action)) BETWEEN 1 AND 120),
    resource_type TEXT NOT NULL CHECK(length(trim(resource_type)) BETWEEN 1 AND 80),
    resource_id TEXT NOT NULL
        CHECK(length(resource_id) BETWEEN 8 AND 96)
        CHECK(resource_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    result_code TEXT NOT NULL CHECK(length(trim(result_code)) BETWEEN 1 AND 120),
    occurred_at_ms INTEGER NOT NULL CHECK(occurred_at_ms >= 0),
    PRIMARY KEY (tenant_id, audit_event_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX audit_resource_lookup
    ON audit_events(tenant_id, resource_type, resource_id, occurred_at_ms);
CREATE INDEX audit_correlation_lookup
    ON audit_events(tenant_id, correlation_id, occurred_at_ms);

CREATE TABLE outbox_events (
    tenant_id TEXT NOT NULL,
    outbox_event_id TEXT NOT NULL
        CHECK(length(outbox_event_id) BETWEEN 8 AND 96)
        CHECK(outbox_event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    aggregate_type TEXT NOT NULL CHECK(length(trim(aggregate_type)) BETWEEN 1 AND 80),
    aggregate_id TEXT NOT NULL
        CHECK(length(aggregate_id) BETWEEN 8 AND 96)
        CHECK(aggregate_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    aggregate_version INTEGER NOT NULL CHECK(aggregate_version >= 1),
    event_type TEXT NOT NULL CHECK(length(trim(event_type)) BETWEEN 1 AND 160),
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    published_at_ms INTEGER CHECK(published_at_ms IS NULL OR published_at_ms >= created_at_ms),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
    PRIMARY KEY (tenant_id, outbox_event_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX outbox_pending_delivery
    ON outbox_events(tenant_id, created_at_ms, outbox_event_id)
    WHERE published_at_ms IS NULL;
