-- #802 K2: exact, append-only D1 retirement admission. NOT R2 deletion.
-- The local Firefox/Camoufox user_data remains primary. A server-side
-- catalog/WORKING snapshot CANNOT prove absence of offline LOCAL_UNSYNCED Save;
-- R2 DELETE therefore remains disabled until live Coordinator/Bridge checks
-- and durable, retryable exact-object effects are separately accepted.
-- A retired generation must never be reactivated by EITHER Catalog command
-- or the interactive successor journal, including after a lost response.

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

CREATE TRIGGER profile_generation_retirement_append_only_update
BEFORE UPDATE ON profile_generation_retirement_intents
FOR EACH ROW BEGIN
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

CREATE INDEX profile_generation_retirement_history
ON profile_generation_retirement_intents(tenant_id, profile_id, admitted_at_ms DESC);
