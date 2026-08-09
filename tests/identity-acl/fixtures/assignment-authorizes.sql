-- Deliberately invalid authorization projection.
-- Permanent CI must reject any design that treats the existence of a business assignment
-- as authority for tenant members, including revoked/suspended memberships.
CREATE VIEW fixture_profile_access AS
SELECT assignment.tenant_id, membership.actor_id, assignment.profile_id
FROM profile_client_assignments AS assignment
JOIN memberships AS membership
  ON membership.tenant_id = assignment.tenant_id
WHERE assignment.closed_at_ms IS NULL;
