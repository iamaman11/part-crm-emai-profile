-- Deliberately invalid authorization projection.
-- Permanent CI must reject any design that treats historical assignment as ACL.
CREATE VIEW fixture_profile_access AS
SELECT tenant_id, assigned_by_actor_id AS actor_id, profile_id
FROM profile_client_assignments
WHERE closed_at_ms IS NULL;
