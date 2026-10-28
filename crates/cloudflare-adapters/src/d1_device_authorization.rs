use application_ports::device_jobs::{
    DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError, DeviceJobPortErrorClass,
};
use device_domain::DeviceJobTarget;
use profile_platform_primitives::{ActorContext, DeviceId, GenerationId, ProfileId, UnixMillis};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_AUTHORIZATION: &str = r#"
SELECT authorization.version AS grant_version, membership.role
FROM device_authorizations AS authorization
JOIN memberships AS membership
  ON membership.tenant_id = authorization.tenant_id
 AND membership.actor_id = ?
 AND membership.status = 'ACTIVE'
WHERE authorization.tenant_id = ?
  AND authorization.device_id = ?
  AND authorization.profile_id = ?
  AND authorization.generation_id = ?
  AND authorization.status = 'ACTIVE'
  AND (
      membership.role = 'TENANT_OWNER'
      OR (
          membership.role = 'MEMBER'
          AND EXISTS (
              SELECT 1
              FROM profile_grants AS grant
              WHERE grant.tenant_id = authorization.tenant_id
                AND grant.profile_id = authorization.profile_id
                AND grant.actor_id = membership.actor_id
          )
      )
  )
"#;

const ADMIT_PROFILE_DEVICE: &str = r#"
INSERT INTO device_authorizations (
    tenant_id, device_id, profile_id, generation_id, version, status,
    evidence_reference, authorized_by_actor_id, updated_by_actor_id,
    authorized_at_ms, updated_at_ms, revoked_at_ms
)
SELECT profile.tenant_id, binding.device_id, profile.profile_id, generation.generation_id,
    1, 'ACTIVE', 'r2sha256:' || generation.container_digest, member.actor_id, member.actor_id,
    ?4, ?4, NULL
FROM browser_profiles AS profile
JOIN profile_generations AS generation ON generation.tenant_id = profile.tenant_id
 AND generation.profile_id = profile.profile_id AND generation.generation_id = profile.active_generation_id
JOIN memberships AS member ON member.tenant_id = profile.tenant_id AND member.actor_id = ?2
JOIN device_actor_bindings AS binding ON binding.tenant_id = member.tenant_id AND binding.actor_id = member.actor_id
WHERE profile.tenant_id = ?1 AND profile.profile_id = ?3
 AND profile.status = 'READY' AND generation.status = 'VERIFIED'
 AND generation.verification_reference IS NOT NULL
 AND member.status = 'ACTIVE' AND binding.status = 'ACTIVE' AND binding.device_id = ?5
 AND binding.evidence_reference LIKE 'p256_spki_der:%'
 AND (member.role = 'TENANT_OWNER' OR (member.role = 'MEMBER' AND EXISTS (
     SELECT 1 FROM profile_grants AS grant WHERE grant.tenant_id = profile.tenant_id
       AND grant.profile_id = profile.profile_id AND grant.actor_id = member.actor_id
       AND grant.role = 'PROFILE_OPERATOR'
 )))
ON CONFLICT(tenant_id, device_id, profile_id, generation_id) DO NOTHING
"#;

/// Existing D1 authorization owner releases obsolete cloud grants.
/// Old generations are already stale for Open; a pending device job keeps its
/// grant, and hence its R2 recovery copy. No local Firefox files are touched.
const RELEASE_UNUSED_HISTORICAL_GRANTS: &str = r#"
UPDATE device_authorizations AS obsolete
SET status = 'REVOKED',
    version = obsolete.version + 1,
    updated_by_actor_id = ?4,
    updated_at_ms = ?5,
    revoked_at_ms = ?5
WHERE obsolete.tenant_id = ?1
  AND obsolete.profile_id = ?2
  AND obsolete.status = 'ACTIVE'
  AND obsolete.generation_id <> ?3
  AND obsolete.generation_id <> ?6
  AND obsolete.updated_at_ms <= ?5
  AND EXISTS (
      SELECT 1 FROM browser_profiles AS profile
      JOIN profile_generations AS current
        ON current.tenant_id = profile.tenant_id
       AND current.profile_id = profile.profile_id
       AND current.generation_id = profile.active_generation_id
      JOIN profile_generation_working_proofs AS proof
        ON proof.tenant_id = profile.tenant_id
       AND proof.profile_id = profile.profile_id
       AND proof.generation_id = ?6
      JOIN profile_generations AS reserve
        ON reserve.tenant_id = proof.tenant_id
       AND reserve.profile_id = proof.profile_id
       AND reserve.generation_id = proof.generation_id
      JOIN memberships AS owner
        ON owner.tenant_id = profile.tenant_id AND owner.actor_id = ?4
      WHERE profile.tenant_id = obsolete.tenant_id
        AND profile.profile_id = obsolete.profile_id
        AND profile.status = 'READY'
        AND profile.active_generation_id = ?3
        AND current.status = 'VERIFIED'
        AND current.verification_reference IS NOT NULL
        AND reserve.status = 'VERIFIED'
        AND reserve.verification_reference IS NOT NULL
        AND reserve.generation_id <> current.generation_id
        AND proof.metadata_digest = reserve.metadata_digest
        AND proof.container_digest = reserve.container_digest
        AND proof.runtime_inventory_sha256 = ?7
        AND owner.role = 'TENANT_OWNER'
        AND owner.status = 'ACTIVE'
        AND NOT EXISTS (
            SELECT 1 FROM profile_generation_retirement_intents AS retired
            WHERE retired.tenant_id = reserve.tenant_id
              AND retired.profile_id = reserve.profile_id
              AND retired.generation_id = reserve.generation_id
        )
  )
  AND NOT EXISTS (
      SELECT 1 FROM device_jobs AS job
      WHERE job.tenant_id = obsolete.tenant_id
        AND job.profile_id = obsolete.profile_id
        AND job.generation_id = obsolete.generation_id
        AND job.device_id = obsolete.device_id
        AND job.status <> 'SUCCEEDED'
  )
"#;

#[derive(Deserialize)]
struct AuthorizationRow {
    grant_version: i64,
    role: String,
}

pub struct D1DeviceJobAuthorization {
    database: D1Database,
}

impl D1DeviceJobAuthorization {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
    /// Called only after the browser proved an independent WORKING reserve.
    /// An old device authorization is not a permanent cloud keep-grant after
    /// CURRENT advances. Any unfinished job retains its exact authorization.
    /// CURRENT and selected LKG are never revoked by this maintenance.
    pub async fn release_unused_historical_grants(
        &self,
        actor: &ActorContext,
        profile: &ProfileId,
        current: &GenerationId,
        working: &GenerationId,
        runtime_inventory_sha256: &str,
        now: UnixMillis,
    ) -> Result<(), DeviceJobPortError> {
        if current == working {
            return Err(integrity_failure());
        }
        let now = i64::try_from(now.value()).map_err(|_| integrity_failure())?;
        query!(
            &self.database,
            RELEASE_UNUSED_HISTORICAL_GRANTS,
            actor.tenant_scope().tenant_id().as_str(),
            profile.as_str(),
            current.as_str(),
            actor.actor_id().as_str(),
            now,
            working.as_str(),
            runtime_inventory_sha256,
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        Ok(())
    }

    /// Materialize execution evidence from current user/Profile/device permission. No overwrite of
    /// revoked evidence and no permission transfer from assignment, creator or previous writer.
    pub async fn admit_profile_device(
        &self,
        actor: &ActorContext,
        profile: &ProfileId,
        device: &DeviceId,
        now: UnixMillis,
    ) -> Result<(), DeviceJobPortError> {
        let now = i64::try_from(now.value()).map_err(|_| integrity_failure())?;
        query!(
            &self.database,
            ADMIT_PROFILE_DEVICE,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
            profile.as_str(),
            now,
            device.as_str()
        )
        .map_err(map_worker_error)?
        .run()
        .await
        .map_err(map_worker_error)?;
        Ok(())
    }
}

impl DeviceJobAuthorizationPort for D1DeviceJobAuthorization {
    async fn is_device_job_authorized(
        &self,
        actor: &ActorContext,
        target: &DeviceJobTarget,
        capability: DeviceJobCapability,
    ) -> Result<bool, DeviceJobPortError> {
        if actor.tenant_scope().tenant_id() != target.tenant_id() {
            return Ok(false);
        }
        let row = query!(
            &self.database,
            LOAD_AUTHORIZATION,
            actor.actor_id().as_str(),
            target.tenant_id().as_str(),
            target.device_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<AuthorizationRow>(None)
        .await
        .map_err(map_worker_error)?;
        let Some(row) = row else {
            return Ok(false);
        };
        let version = u64::try_from(row.grant_version).map_err(|_| integrity_failure())?;
        if version == 0 || !matches!(row.role.as_str(), "TENANT_OWNER" | "MEMBER") {
            return Err(integrity_failure());
        }
        match capability {
            DeviceJobCapability::Issue
            | DeviceJobCapability::Claim
            | DeviceJobCapability::Heartbeat
            | DeviceJobCapability::Complete
            | DeviceJobCapability::Recover
            | DeviceJobCapability::Cancel => Ok(true),
        }
    }
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::LOAD_AUTHORIZATION;

    #[test]
    fn authorization_query_is_tenant_profile_grant_and_device_scoped() {
        for required in [
            "authorization.tenant_id = ?",
            "authorization.device_id = ?",
            "authorization.profile_id = ?",
            "authorization.generation_id = ?",
            "authorization.status = 'ACTIVE'",
            "membership.status = 'ACTIVE'",
            "profile_grants",
        ] {
            assert!(LOAD_AUTHORIZATION.contains(required));
        }
        assert!(!LOAD_AUTHORIZATION.contains("profile_assignments"));
    }
}
