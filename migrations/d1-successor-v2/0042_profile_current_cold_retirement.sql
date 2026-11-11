-- #849: one-CURRENT cloud retention, inside the existing Catalog D1 owner.
-- This ADDITIVE migration has no cleanup side effects. Historical 0039/0040
-- WORKING/LKG records are never a grant to this lifecycle.
-- The only R2 DELETE owner is the existing Worker scheduler + exact-key adapter.
-- No extra generation, service, browser controller, or local persistent state.

-- Exactly one immutable old generation may be retired after a genuine
-- machine-authenticated cold Restore of the new verified CURRENT. The Worker
-- checks actual R2 descriptors, authenticated BPGC metadata prelude, original
-- plaintext SHA, exact device identity and full local restored byte equality.
-- D1 rechecks CURRENT CAS, permissions, grants, jobs, coordinator, and old
-- signed PUT expiry. A self-reported digest alone MUST NEVER reach INSERT.
CREATE TABLE profile_generation_current_retirement_intents (
    tenant_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    observed_current_generation_id TEXT NOT NULL,
    observed_profile_version INTEGER NOT NULL CHECK(observed_profile_version > 0),
    object_key TEXT NOT NULL,
    metadata_digest TEXT NOT NULL
      CHECK(length(metadata_digest)=64 AND metadata_digest NOT GLOB '*[^0-9a-f]*'),
    container_digest TEXT NOT NULL
      CHECK(length(container_digest)=64 AND container_digest NOT GLOB '*[^0-9a-f]*'),
    current_object_key TEXT NOT NULL,
    current_metadata_digest TEXT NOT NULL
      CHECK(length(current_metadata_digest)=64 AND current_metadata_digest NOT GLOB '*[^0-9a-f]*'),
    current_container_digest TEXT NOT NULL
      CHECK(length(current_container_digest)=64 AND current_container_digest NOT GLOB '*[^0-9a-f]*'),
    sealed_current_native_sha256 TEXT NOT NULL
      CHECK(length(sealed_current_native_sha256)=64 AND sealed_current_native_sha256 NOT GLOB '*[^0-9a-f]*'),
    restored_current_native_sha256 TEXT NOT NULL
      CHECK(length(restored_current_native_sha256)=64 AND restored_current_native_sha256 NOT GLOB '*[^0-9a-f]*'),
    admitted_by_actor_id TEXT NOT NULL,
    admitted_by_device_id TEXT NOT NULL,
    admitted_at_ms INTEGER NOT NULL CHECK(admitted_at_ms >= 0),
    not_before_at_ms INTEGER NOT NULL CHECK(not_before_at_ms >= admitted_at_ms + 360000),
    deleted_at_ms INTEGER CHECK(deleted_at_ms IS NULL OR deleted_at_ms >= not_before_at_ms),
    PRIMARY KEY (tenant_id, profile_id, generation_id),
    CHECK(generation_id <> observed_current_generation_id),
    CHECK(sealed_current_native_sha256 = restored_current_native_sha256),
    FOREIGN KEY(tenant_id, profile_id, generation_id)
      REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, profile_id, observed_current_generation_id)
      REFERENCES profile_generations(tenant_id, profile_id, generation_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id, admitted_by_actor_id)
      REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER profile_generation_current_retirement_admit
BEFORE INSERT ON profile_generation_current_retirement_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT,'cold_current_retirement_not_authorized')
    WHERE NOT EXISTS (
        SELECT 1
        FROM browser_profiles AS profile
        JOIN profile_generations AS current
          ON current.tenant_id=profile.tenant_id
         AND current.profile_id=profile.profile_id
         AND current.generation_id=profile.active_generation_id
        JOIN profile_generations AS old
          ON old.tenant_id=profile.tenant_id
         AND old.profile_id=profile.profile_id
         AND old.generation_id=NEW.generation_id
        JOIN profile_generation_successor_commands AS successor
          ON successor.tenant_id=profile.tenant_id
         AND successor.profile_id=profile.profile_id
         AND successor.generation_id=current.generation_id
         AND successor.base_generation_id=old.generation_id
        JOIN memberships AS actor
          ON actor.tenant_id=profile.tenant_id
         AND actor.actor_id=NEW.admitted_by_actor_id
        JOIN device_actor_bindings AS binding
          ON binding.tenant_id=actor.tenant_id
         AND binding.actor_id=actor.actor_id
         AND binding.device_id=NEW.admitted_by_device_id
        JOIN device_authorizations AS grant
          ON grant.tenant_id=profile.tenant_id
         AND grant.profile_id=profile.profile_id
         AND grant.generation_id=current.generation_id
         AND grant.device_id=NEW.admitted_by_device_id
        WHERE profile.tenant_id=NEW.tenant_id
          AND profile.profile_id=NEW.profile_id
          AND profile.status='READY'
          AND profile.version=NEW.observed_profile_version
          AND profile.active_generation_id=NEW.observed_current_generation_id
          AND profile.updated_at_ms<=NEW.admitted_at_ms
          AND current.status='VERIFIED'
          AND current.verification_reference IS NOT NULL
          AND current.object_key=NEW.current_object_key
          AND current.metadata_digest=NEW.current_metadata_digest
          AND current.container_digest=NEW.current_container_digest
          AND current.updated_at_ms<=NEW.admitted_at_ms
          AND old.status='VERIFIED'
          AND old.verification_reference IS NOT NULL
          AND old.object_key=NEW.object_key
          AND old.metadata_digest=NEW.metadata_digest
          AND old.container_digest=NEW.container_digest
          AND old.updated_at_ms<=NEW.admitted_at_ms
          AND actor.status='ACTIVE'
          AND (actor.role='TENANT_OWNER' OR EXISTS (
              SELECT 1 FROM profile_grants AS acl
              WHERE acl.tenant_id=actor.tenant_id
                AND acl.profile_id=profile.profile_id
                AND acl.actor_id=actor.actor_id
                AND acl.role='PROFILE_OPERATOR'
          ))
          AND binding.status='ACTIVE'
          AND binding.bound_at_ms<=NEW.admitted_at_ms
          AND grant.status='ACTIVE'
          AND grant.revoked_at_ms IS NULL
          AND grant.authorized_at_ms<=NEW.admitted_at_ms
          -- The original candidate grant may still be active on the same
          -- restored machine. It will be revoked *atomically* below.
          -- Other devices can be offline and must keep their own recovery.
          AND NOT EXISTS (
              SELECT 1 FROM device_authorizations AS other
              WHERE other.tenant_id=profile.tenant_id
                AND other.profile_id=profile.profile_id
                AND other.generation_id=old.generation_id
                AND other.status='ACTIVE'
                AND other.device_id<>NEW.admitted_by_device_id
          )
          AND NOT EXISTS (
              SELECT 1 FROM device_jobs AS job
              WHERE job.tenant_id=profile.tenant_id
                AND job.profile_id=profile.profile_id
                AND job.generation_id=old.generation_id
                AND job.status NOT IN ('SUCCEEDED','FAILED','CANCELLED')
          )
          AND NOT EXISTS (
              SELECT 1 FROM profile_coordinator_projections AS coordinator
              WHERE coordinator.tenant_id=profile.tenant_id
                AND coordinator.profile_id=profile.profile_id
                AND (coordinator.coordinator_status<>'idle'
                  OR coordinator.pending_launch_intent_id IS NOT NULL)
          )
          AND NOT EXISTS (
              SELECT 1 FROM profile_delete_commands AS deleting
              WHERE deleting.tenant_id=profile.tenant_id
                AND deleting.profile_id=profile.profile_id
          )
          -- Every PUT ever signed for the retiring generation must expire
          -- before any R2 effect. Also reserve 360 s for pre-ledger URLs.
          AND NEW.not_before_at_ms>=COALESCE((
              SELECT MAX(issued.expires_at_ms)
              FROM profile_generation_upload_reservations AS issued
              WHERE issued.tenant_id=profile.tenant_id
                AND issued.profile_id=profile.profile_id
                AND issued.generation_id=old.generation_id
          ),0)
    );
END;

CREATE TRIGGER profile_generation_current_retirement_apply
AFTER INSERT ON profile_generation_current_retirement_intents
FOR EACH ROW
BEGIN
    -- The just-recovered machine no longer needs its own old grant. The
    -- original device authorization row stays in D1 as REVOKED history.
    -- No other device grant can be changed by this transaction.
    UPDATE device_authorizations
    SET status='REVOKED',
        version=version+1,
        updated_by_actor_id=NEW.admitted_by_actor_id,
        updated_at_ms=NEW.admitted_at_ms,
        revoked_at_ms=NEW.admitted_at_ms
    WHERE tenant_id=NEW.tenant_id
      AND profile_id=NEW.profile_id
      AND generation_id=NEW.generation_id
      AND device_id=NEW.admitted_by_device_id
      AND status='ACTIVE';
END;

CREATE TRIGGER profile_generation_current_retirement_append_only_update
BEFORE UPDATE ON profile_generation_current_retirement_intents
FOR EACH ROW WHEN NOT (
    OLD.deleted_at_ms IS NULL
    AND NEW.deleted_at_ms IS NOT NULL
    AND NEW.deleted_at_ms >= OLD.not_before_at_ms
    AND NEW.tenant_id=OLD.tenant_id
    AND NEW.profile_id=OLD.profile_id
    AND NEW.generation_id=OLD.generation_id
    AND NEW.observed_current_generation_id=OLD.observed_current_generation_id
    AND NEW.observed_profile_version=OLD.observed_profile_version
    AND NEW.object_key=OLD.object_key
    AND NEW.metadata_digest=OLD.metadata_digest
    AND NEW.container_digest=OLD.container_digest
    AND NEW.current_object_key=OLD.current_object_key
    AND NEW.current_metadata_digest=OLD.current_metadata_digest
    AND NEW.current_container_digest=OLD.current_container_digest
    AND NEW.sealed_current_native_sha256=OLD.sealed_current_native_sha256
    AND NEW.restored_current_native_sha256=OLD.restored_current_native_sha256
    AND NEW.admitted_by_actor_id=OLD.admitted_by_actor_id
    AND NEW.admitted_by_device_id=OLD.admitted_by_device_id
    AND NEW.admitted_at_ms=OLD.admitted_at_ms
    AND NEW.not_before_at_ms=OLD.not_before_at_ms
)
BEGIN SELECT RAISE(ABORT,'current_retirement_append_only'); END;

CREATE TRIGGER profile_generation_current_retirement_append_only_delete
BEFORE DELETE ON profile_generation_current_retirement_intents
BEGIN SELECT RAISE(ABORT,'current_retirement_append_only'); END;

-- Once a D1 retirement intent exists, the original key can never be
-- reactivated or reissued, even after an offline restart or lost HTTP reply.
CREATE TRIGGER profile_generation_current_retired_pointer_forbidden
BEFORE UPDATE OF active_generation_id ON browser_profiles
FOR EACH ROW WHEN NEW.active_generation_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.active_generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_current'); END;

CREATE TRIGGER profile_generation_current_retired_activation_forbidden
BEFORE INSERT ON profile_generation_activate_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_activated'); END;

CREATE TRIGGER profile_generation_current_retired_successor_forbidden
BEFORE INSERT ON profile_generation_successor_commands
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id IN (NEW.base_generation_id,NEW.generation_id)
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_successor'); END;

CREATE TRIGGER profile_generation_current_retired_grant_forbidden
BEFORE INSERT ON device_authorizations
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_authorized'); END;

CREATE TRIGGER profile_generation_current_retired_regrant_forbidden
BEFORE UPDATE OF status ON device_authorizations
FOR EACH ROW WHEN NEW.status='ACTIVE' AND EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_authorized'); END;

CREATE TRIGGER profile_generation_current_retired_job_forbidden
BEFORE INSERT ON device_jobs
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_be_device_job_target'); END;

CREATE TRIGGER profile_generation_current_retired_upload_insert_forbidden
BEFORE INSERT ON profile_generation_upload_reservations
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_reissue_put'); END;

CREATE TRIGGER profile_generation_current_retired_upload_update_forbidden
BEFORE UPDATE ON profile_generation_upload_reservations
FOR EACH ROW WHEN EXISTS (
    SELECT 1 FROM profile_generation_current_retirement_intents AS retired
    WHERE retired.tenant_id=NEW.tenant_id
      AND retired.profile_id=NEW.profile_id
      AND retired.generation_id=NEW.generation_id
)
BEGIN SELECT RAISE(ABORT,'retired_generation_cannot_reissue_put'); END;

CREATE INDEX profile_generation_current_retirement_pending
ON profile_generation_current_retirement_intents(deleted_at_ms,not_before_at_ms);
