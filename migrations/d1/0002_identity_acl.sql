-- Repository Step 4: transactional identity and ACL support.
-- Invitation acceptance is materialized as an append-only governed record so a
-- failed pending/expiry check rolls back the membership and complete envelope.

CREATE TABLE invitation_acceptances (
    tenant_id TEXT NOT NULL,
    invitation_id TEXT NOT NULL,
    identity_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    accepted_at_ms INTEGER NOT NULL CHECK(accepted_at_ms >= 0),
    PRIMARY KEY (tenant_id, invitation_id),
    UNIQUE (tenant_id, actor_id),
    FOREIGN KEY (tenant_id, invitation_id)
        REFERENCES invitations(tenant_id, invitation_id) ON DELETE RESTRICT,
    FOREIGN KEY (identity_id) REFERENCES identities(identity_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER invitation_acceptance_requires_pending_unexpired
BEFORE INSERT ON invitation_acceptances
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1
    FROM invitations
    WHERE tenant_id = NEW.tenant_id
      AND invitation_id = NEW.invitation_id
      AND status = 'PENDING'
      AND expires_at_ms > NEW.accepted_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'invitation_not_pending_or_expired');
END;

CREATE TRIGGER invitation_acceptance_marks_accepted
AFTER INSERT ON invitation_acceptances
FOR EACH ROW
BEGIN
    UPDATE invitations
    SET status = 'ACCEPTED'
    WHERE tenant_id = NEW.tenant_id
      AND invitation_id = NEW.invitation_id;
END;

CREATE TRIGGER final_active_owner_cannot_be_suspended_or_revoked
BEFORE UPDATE OF status ON memberships
FOR EACH ROW
WHEN OLD.role = 'TENANT_OWNER'
 AND OLD.status = 'ACTIVE'
 AND NEW.status <> 'ACTIVE'
 AND (
    SELECT COUNT(*)
    FROM memberships
    WHERE tenant_id = OLD.tenant_id
      AND role = 'TENANT_OWNER'
      AND status = 'ACTIVE'
 ) <= 1
BEGIN
    SELECT RAISE(ABORT, 'last_active_owner');
END;

CREATE TRIGGER active_owner_cannot_be_deleted
BEFORE DELETE ON memberships
FOR EACH ROW
WHEN OLD.role = 'TENANT_OWNER' AND OLD.status = 'ACTIVE'
BEGIN
    SELECT RAISE(ABORT, 'active_owner_delete_forbidden');
END;
