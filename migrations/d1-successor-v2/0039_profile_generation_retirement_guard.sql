-- #802 K2: exact D1 retirement admission and one-way R2 effect receipt.
-- The local Firefox/Camoufox user_data remains primary and is NEVER an
-- input to the R2 retention operation. CURRENT, the selected browser-proven
-- LKG, and generations with live cloud grants/jobs cannot be retired.
-- A retired generation cannot be reactivated or granted to a new device.
-- Exactly one terminal deleted_at_ms is allowed after exact R2 absence.

CREATE TABLE profile_generation_retirement_intents (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    observed_current_generation_id TEXT NOT NULL,
    retained_working_generation_id TEXT NOT NULL,
    expected_profile_version INTEGER NOT NULL CHECK(expected_profile_version > 0),
    object_key TEXT NOT NULL,
    metadata_digest TEXT NOT NULL CHECK(length(metadata_digest) = 64
        AND metadata_digest NOT GLOB '*[^0-9a-f]*'),
    container_digest TEXT NOT NULL CHECK(length(container_digest) = 64
        AND container_digest NOT GLOB '*[^0-9a-f]*'),
    approved_runtime_inventory_sha256 TEXT NOT NULL
        CHECK(length(approved_runtime_inventory_sha256) = 64
          AND approved_runtime_inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
    admitted_by_actor_id TEXT NOT NULL,
    admitted_by_device_id TEXT NOT NULL,
    coordinator_session_id TEXT NOT NULL
        CHECK(length(coordinator_session_id) BETWEEN 8 AND 96
          AND coordinator_session_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    coordinator_epoch INTEGER NOT NULL CHECK(coordinator_epoch > 0),
    admitted_at_ms INTEGER NOT NULL CHECK(admitted_at_ms >= 0),
    deleted_at_ms INTEGER CHECK(deleted_at_ms IS NULL OR deleted_at_ms >= admitted_at_ms),
    PRIMARY KEY(tenant_id, profile_id, generation_id),
    CHECK(generation_id <> observed_current_generation_id),
    CHECK(generation_id <> retained_working_generation_id),
    CHECK(observed_current_generation_id <> retained_working_generation_id),
    FOREIGN KEY(tenant_id, profile_id, generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, profile_id, observed_current_generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, profile_id, retained_working_generation_id)
        REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, admitted_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

-- Exactly one transactional read of CURRENT, protected independently proven
-- N-k, and unproven candidate. Only an active TENANT_OWNER with a live device
-- binding AND exact active-generation authorization can admit a receipt.
-- The external Coordinator lease, original key,
-- runtime bundle and local dirty-state require independent checks; no R2 effect
-- is wired to this table.
CREATE TRIGGER profile_generation_retirement_validate
BEFORE INSERT ON profile_generation_retirement_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'retirement_requires_exact_protected_working_and_current')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        JOIN profile_generations AS current
          ON current.tenant_id = profile.tenant_id
         AND current.profile_id = profile.profile_id
         AND current.generation_id = profile.active_generation_id
        JOIN profile_generations AS candidate
          ON candidate.tenant_id = profile.tenant_id
         AND candidate.profile_id = profile.profile_id
         AND candidate.generation_id = NEW.generation_id
        JOIN profile_generation_working_proofs AS proof
          ON proof.tenant_id = profile.tenant_id
         AND proof.profile_id = profile.profile_id
         AND proof.generation_id = NEW.retained_working_generation_id
        JOIN profile_generations AS working
          ON working.tenant_id = proof.tenant_id
         AND working.profile_id = proof.profile_id
         AND working.generation_id = proof.generation_id
        JOIN memberships AS actor
          ON actor.tenant_id = profile.tenant_id
         AND actor.actor_id = NEW.admitted_by_actor_id
        JOIN device_actor_bindings AS device
          ON device.tenant_id = actor.tenant_id
         AND device.actor_id = actor.actor_id
         AND device.device_id = NEW.admitted_by_device_id
        -- A live actor/device binding is NOT permission to operate this
        -- exact CURRENT generation. Old grants cannot retire recovery bytes.
        JOIN device_authorizations AS authorized
          ON authorized.tenant_id = profile.tenant_id
         AND authorized.profile_id = profile.profile_id
         AND authorized.generation_id = current.generation_id
         AND authorized.device_id = NEW.admitted_by_device_id
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'READY'
          AND profile.active_generation_id = NEW.observed_current_generation_id
          AND profile.version = NEW.expected_profile_version
          AND current.status = 'VERIFIED'
          AND current.verification_reference IS NOT NULL
          AND candidate.status = 'VERIFIED'
          AND candidate.verification_reference IS NOT NULL
          AND candidate.object_key = NEW.object_key
          AND candidate.metadata_digest = NEW.metadata_digest
          AND candidate.container_digest = NEW.container_digest
          AND working.status = 'VERIFIED'
          AND working.verification_reference IS NOT NULL
          AND proof.metadata_digest = working.metadata_digest
          AND proof.container_digest = working.container_digest
          AND proof.runtime_inventory_sha256 = NEW.approved_runtime_inventory_sha256
          AND actor.role = 'TENANT_OWNER'
          AND actor.status = 'ACTIVE'
          AND device.status = 'ACTIVE'
          AND authorized.status = 'ACTIVE'
          AND authorized.revoked_at_ms IS NULL
          -- Never treat evidence from the future as an admitted operation.
          AND NEW.admitted_at_ms >= authorized.authorized_at_ms
          AND NEW.admitted_at_ms >= profile.updated_at_ms
          AND NEW.admitted_at_ms >= current.updated_at_ms
          AND NEW.admitted_at_ms >= candidate.updated_at_ms
          AND NEW.admitted_at_ms >= working.updated_at_ms
          AND NEW.admitted_at_ms >= proof.proven_at_ms
          AND NOT EXISTS (
              SELECT 1 FROM profile_generation_working_proofs AS previous
              WHERE previous.tenant_id = candidate.tenant_id
                AND previous.profile_id = candidate.profile_id
                AND previous.generation_id = candidate.generation_id
          )
          -- A historical ACTIVE grant allows another (possibly offline)
          -- device to rely on this generation. A clean Coordinator session
          -- on the retiring device cannot speak for that other device.
          AND NOT EXISTS (
              SELECT 1 FROM device_authorizations AS historical_grant
              WHERE historical_grant.tenant_id = candidate.tenant_id
                AND historical_grant.profile_id = candidate.profile_id
                AND historical_grant.generation_id = candidate.generation_id
                AND historical_grant.status = 'ACTIVE'
          )
          -- A pending or uncertain device job may still need its original
          -- generation to reconcile after an offline Windows restart.
          -- A D1 retirement receipt does not prove all other devices clean.
          AND NOT EXISTS (
              SELECT 1 FROM device_jobs AS pending
              WHERE pending.tenant_id = candidate.tenant_id
                AND pending.profile_id = candidate.profile_id
                AND pending.generation_id = candidate.generation_id
                AND pending.status <> 'SUCCEEDED'
          )
    );
END;

-- One terminal NULL -> deleted timestamp is permitted; all original
-- authorization/identity fields remain immutable.
CREATE TRIGGER profile_generation_retirement_append_only_update
BEFORE UPDATE ON profile_generation_retirement_intents
FOR EACH ROW WHEN NOT (
    OLD.deleted_at_ms IS NULL
    AND NEW.deleted_at_ms IS NOT NULL
    AND NEW.tenant_id = OLD.tenant_id
    AND NEW.profile_id = OLD.profile_id
    AND NEW.generation_id = OLD.generation_id
    AND NEW.observed_current_generation_id = OLD.observed_current_generation_id
    AND NEW.retained_working_generation_id = OLD.retained_working_generation_id
    AND NEW.expected_profile_version = OLD.expected_profile_version
    AND NEW.object_key = OLD.object_key
    AND NEW.metadata_digest = OLD.metadata_digest
    AND NEW.container_digest = OLD.container_digest
    AND NEW.approved_runtime_inventory_sha256 = OLD.approved_runtime_inventory_sha256
    AND NEW.admitted_by_actor_id = OLD.admitted_by_actor_id
    AND NEW.admitted_by_device_id = OLD.admitted_by_device_id
    AND NEW.coordinator_session_id = OLD.coordinator_session_id
    AND NEW.coordinator_epoch = OLD.coordinator_epoch
    AND NEW.admitted_at_ms = OLD.admitted_at_ms
) BEGIN
    SELECT RAISE(ABORT, 'retirement_append_only');
END;
CREATE TRIGGER profile_generation_retirement_append_only_delete
BEFORE DELETE ON profile_generation_retirement_intents
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'retirement_append_only');
END;

-- Both historical activation routes must be fenced. The existing activation
-- and successor tables remain the sole actual lifecycle owners.
CREATE TRIGGER profile_generation_retired_activation_command_forbidden
BEFORE INSERT ON profile_generation_activate_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_activated');
END;

CREATE TRIGGER profile_generation_retired_successor_forbidden
BEFORE INSERT ON profile_generation_successor_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id IN (NEW.base_generation_id, NEW.generation_id)
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_successor');
END;

CREATE TRIGGER profile_generation_retired_current_pointer_forbidden
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW WHEN NEW.active_generation_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.active_generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_current');
END;

CREATE TRIGGER profile_generation_retired_working_proof_forbidden
BEFORE INSERT ON profile_generation_working_proofs
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_working');
END;

-- Never resurrect retired history by issuing a new device grant/job.
CREATE TRIGGER profile_generation_retired_device_grant_forbidden
BEFORE INSERT ON device_authorizations
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_authorized');
END;

CREATE TRIGGER profile_generation_retired_device_regrant_forbidden
BEFORE UPDATE OF status ON device_authorizations
FOR EACH ROW WHEN NEW.status = 'ACTIVE' AND EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_authorized');
END;

CREATE TRIGGER profile_generation_retired_device_job_forbidden
BEFORE INSERT ON device_jobs
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_retirement_intents AS retired
    WHERE retired.tenant_id = NEW.tenant_id
      AND retired.profile_id = NEW.profile_id
      AND retired.generation_id = NEW.generation_id
) BEGIN
    SELECT RAISE(ABORT, 'retired_generation_cannot_be_job_target');
END;

CREATE INDEX profile_generation_retirement_history
ON profile_generation_retirement_intents(tenant_id, profile_id, admitted_at_ms DESC);
