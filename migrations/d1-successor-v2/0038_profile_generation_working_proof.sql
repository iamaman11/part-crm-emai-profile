-- #802 K2: append-only, metadata-only historical real-browser proof receipts.
-- This belongs to the existing D1 generation owner, NOT a new state machine.
-- A receipt is not an R2 DELETE grant. Every later LKG/prune use still needs
-- fresh CURRENT/ACL/coordinator, old-root key, compatible runtime, exact R2,
-- and no unknown/LOCAL_UNSYNCED Save. This migration performs NO cleanup.

CREATE TABLE profile_generation_working_proofs (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    observed_current_generation_id TEXT NOT NULL,
    metadata_digest TEXT NOT NULL
        CHECK(length(metadata_digest) = 64 AND metadata_digest NOT GLOB '*[^0-9a-f]*'),
    container_digest TEXT NOT NULL
        CHECK(length(container_digest) = 64 AND container_digest NOT GLOB '*[^0-9a-f]*'),
    root_key_id TEXT NOT NULL
        CHECK(length(root_key_id) BETWEEN 8 AND 96
          AND root_key_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    runtime_inventory_sha256 TEXT NOT NULL
        CHECK(length(runtime_inventory_sha256) = 64
          AND runtime_inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
    observed_by_actor_id TEXT NOT NULL,
    observed_by_device_id TEXT NOT NULL,
    coordinator_session_id TEXT NOT NULL
        CHECK(length(coordinator_session_id) BETWEEN 8 AND 96
          AND coordinator_session_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    coordinator_epoch INTEGER NOT NULL CHECK(coordinator_epoch > 0),
    proven_at_ms INTEGER NOT NULL CHECK(proven_at_ms >= 0),
    PRIMARY KEY (tenant_id, profile_id, generation_id),
    CHECK(generation_id <> observed_current_generation_id),
    FOREIGN KEY (tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_id, observed_current_generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, observed_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

-- Admission must be tied to CURRENT's actual governed successor journal,
-- its immutable VERIFIED predecessor, and an active device/actor binding.
-- Application ingress independently re-proves the DO lease/fence and old key;
-- a D1 projection or user-supplied digest alone is never sufficient authority.
CREATE TRIGGER profile_generation_working_proof_validate
BEFORE INSERT ON profile_generation_working_proofs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'working_proof_not_authorized_predecessor')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_profiles AS profile
        JOIN profile_generations AS current
          ON current.tenant_id = profile.tenant_id
         AND current.profile_id = profile.profile_id
         AND current.generation_id = profile.active_generation_id
        JOIN profile_generation_successor_commands AS successor
          ON successor.tenant_id = profile.tenant_id
         AND successor.profile_id = profile.profile_id
         AND successor.generation_id = current.generation_id
        JOIN profile_generations AS predecessor
          ON predecessor.tenant_id = successor.tenant_id
         AND predecessor.profile_id = successor.profile_id
         AND predecessor.generation_id = successor.base_generation_id
        JOIN memberships AS actor
          ON actor.tenant_id = profile.tenant_id
         AND actor.actor_id = NEW.observed_by_actor_id
        JOIN device_actor_bindings AS device
          ON device.tenant_id = actor.tenant_id
         AND device.actor_id = actor.actor_id
         AND device.device_id = NEW.observed_by_device_id
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'READY'
          AND profile.active_generation_id = NEW.observed_current_generation_id
          AND current.status = 'VERIFIED'
          AND current.verification_reference IS NOT NULL
          AND predecessor.generation_id = NEW.generation_id
          AND predecessor.status = 'VERIFIED'
          AND predecessor.verification_reference IS NOT NULL
          AND predecessor.metadata_digest = NEW.metadata_digest
          AND predecessor.container_digest = NEW.container_digest
          AND actor.status = 'ACTIVE'
          AND (
              actor.role = 'TENANT_OWNER'
              OR EXISTS (
                  SELECT 1 FROM profile_grants AS grant
                  WHERE grant.tenant_id = actor.tenant_id
                    AND grant.actor_id = actor.actor_id
                    AND grant.profile_id = profile.profile_id
                    AND grant.role = 'PROFILE_OPERATOR'
              )
          )
          AND device.status = 'ACTIVE'
    );
END;

CREATE TRIGGER profile_generation_working_proof_append_only_update
BEFORE UPDATE ON profile_generation_working_proofs
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'working_proof_append_only');
END;

CREATE TRIGGER profile_generation_working_proof_append_only_delete
BEFORE DELETE ON profile_generation_working_proofs
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'working_proof_append_only');
END;

CREATE INDEX profile_generation_working_proof_history
    ON profile_generation_working_proofs(tenant_id, profile_id, proven_at_ms DESC);
