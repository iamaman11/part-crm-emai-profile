-- Defense-in-depth for the profile generation registry.
-- State transitions remain possible only through the governed command tables
-- introduced by 0006. Immutable object identity and digests cannot be rewritten.

CREATE TRIGGER profile_generation_insert_requires_register_command
BEFORE INSERT ON profile_generations
FOR EACH ROW
WHEN NEW.status <> 'REGISTERED'
  OR NEW.version <> 1
  OR NEW.verification_reference IS NOT NULL
  OR NEW.verified_by_actor_id IS NOT NULL
  OR NEW.verified_at_ms IS NOT NULL
  OR NEW.quarantined_by_actor_id IS NOT NULL
  OR NEW.quarantined_at_ms IS NOT NULL
  OR NOT EXISTS (
      SELECT 1
      FROM profile_generation_register_commands AS command
      WHERE command.tenant_id = NEW.tenant_id
        AND command.profile_id = NEW.profile_id
        AND command.generation_id = NEW.generation_id
        AND command.object_key = NEW.object_key
        AND command.metadata_digest = NEW.metadata_digest
        AND command.container_digest = NEW.container_digest
        AND command.command_actor_id = NEW.registered_by_actor_id
        AND command.executed_at_ms = NEW.created_at_ms
        AND command.executed_at_ms = NEW.updated_at_ms
  )
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_insert_not_governed');
END;

CREATE TRIGGER profile_generation_immutable_identity
BEFORE UPDATE OF tenant_id, profile_id, generation_id, object_key,
                 metadata_digest, container_digest, registered_by_actor_id,
                 created_at_ms
ON profile_generations
FOR EACH ROW
WHEN NEW.tenant_id <> OLD.tenant_id
  OR NEW.profile_id <> OLD.profile_id
  OR NEW.generation_id <> OLD.generation_id
  OR NEW.object_key <> OLD.object_key
  OR NEW.metadata_digest <> OLD.metadata_digest
  OR NEW.container_digest <> OLD.container_digest
  OR NEW.registered_by_actor_id <> OLD.registered_by_actor_id
  OR NEW.created_at_ms <> OLD.created_at_ms
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_identity_immutable');
END;

CREATE TRIGGER profile_generation_transition_requires_command
BEFORE UPDATE OF status, version, verification_reference,
                 verified_by_actor_id, verified_at_ms,
                 quarantined_by_actor_id, quarantined_at_ms, updated_at_ms
ON profile_generations
FOR EACH ROW
WHEN NOT (
    OLD.status = 'REGISTERED'
    AND NEW.status = 'VERIFIED'
    AND NEW.version = OLD.version + 1
    AND NEW.verification_reference IS NOT NULL
    AND NEW.verified_by_actor_id IS NOT NULL
    AND NEW.verified_at_ms = NEW.updated_at_ms
    AND NEW.quarantined_by_actor_id IS NULL
    AND NEW.quarantined_at_ms IS NULL
    AND EXISTS (
        SELECT 1
        FROM profile_generation_verify_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.profile_id = OLD.profile_id
          AND command.generation_id = OLD.generation_id
          AND command.expected_generation_version = OLD.version
          AND command.verification_reference = NEW.verification_reference
          AND command.command_actor_id = NEW.verified_by_actor_id
          AND command.executed_at_ms = NEW.verified_at_ms
    )
) AND NOT (
    OLD.status IN ('REGISTERED', 'VERIFIED')
    AND NEW.status = 'QUARANTINED'
    AND NEW.version = OLD.version + 1
    AND NEW.verification_reference IS OLD.verification_reference
    AND NEW.verified_by_actor_id IS OLD.verified_by_actor_id
    AND NEW.verified_at_ms IS OLD.verified_at_ms
    AND NEW.quarantined_by_actor_id IS NOT NULL
    AND NEW.quarantined_at_ms = NEW.updated_at_ms
    AND EXISTS (
        SELECT 1
        FROM profile_generation_quarantine_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.profile_id = OLD.profile_id
          AND command.generation_id = OLD.generation_id
          AND command.expected_generation_version = OLD.version
          AND command.command_actor_id = NEW.quarantined_by_actor_id
          AND command.executed_at_ms = NEW.quarantined_at_ms
    )
)
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_transition_not_governed');
END;

CREATE TRIGGER profile_active_generation_insert_must_be_verified
BEFORE INSERT ON browser_profiles
FOR EACH ROW
WHEN NEW.active_generation_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM profile_generations AS generation
     WHERE generation.tenant_id = NEW.tenant_id
       AND generation.profile_id = NEW.profile_id
       AND generation.generation_id = NEW.active_generation_id
       AND generation.status = 'VERIFIED'
 )
BEGIN
    SELECT RAISE(ABORT, 'active_profile_generation_not_verified');
END;

CREATE TRIGGER profile_active_generation_update_must_be_verified
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW
WHEN NEW.active_generation_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM profile_generations AS generation
     WHERE generation.tenant_id = NEW.tenant_id
       AND generation.profile_id = NEW.profile_id
       AND generation.generation_id = NEW.active_generation_id
       AND generation.status = 'VERIFIED'
 )
BEGIN
    SELECT RAISE(ABORT, 'active_profile_generation_not_verified');
END;

CREATE TRIGGER profile_generation_activation_requires_command
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW
WHEN NEW.active_generation_id IS NOT OLD.active_generation_id
 AND NEW.active_generation_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM profile_generation_activate_commands AS command
     WHERE command.tenant_id = OLD.tenant_id
       AND command.profile_id = OLD.profile_id
       AND command.generation_id = NEW.active_generation_id
       AND command.expected_profile_version = OLD.version
       AND command.command_actor_id = NEW.updated_by_actor_id
       AND command.executed_at_ms = NEW.updated_at_ms
 )
BEGIN
    SELECT RAISE(ABORT, 'profile_generation_activation_not_governed');
END;

-- Validate any pre-existing non-null pointer when this migration is applied.
UPDATE browser_profiles
SET active_generation_id = active_generation_id
WHERE active_generation_id IS NOT NULL;
