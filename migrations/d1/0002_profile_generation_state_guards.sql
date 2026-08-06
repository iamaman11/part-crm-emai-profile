-- Repository quality hardening: impossible live profile states fail closed in D1.
-- A generation registry is introduced by a later bounded capability; this migration
-- enforces the invariant that every live/coordinatable state names an active
-- generation without claiming that the referenced remote object is verified here.

CREATE TRIGGER live_profile_insert_requires_active_generation
BEFORE INSERT ON browser_profiles
FOR EACH ROW
WHEN NEW.status IN ('READY', 'IN_USE', 'DIRTY_LOCAL', 'SYNCING')
 AND NEW.active_generation_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'live_profile_requires_active_generation');
END;

CREATE TRIGGER live_profile_update_requires_active_generation
BEFORE UPDATE OF status, active_generation_id ON browser_profiles
FOR EACH ROW
WHEN NEW.status IN ('READY', 'IN_USE', 'DIRTY_LOCAL', 'SYNCING')
 AND NEW.active_generation_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'live_profile_requires_active_generation');
END;
