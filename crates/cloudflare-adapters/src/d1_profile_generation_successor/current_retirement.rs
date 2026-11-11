//! The existing D1 generation-successor Catalog owner's exact cold CURRENT
//! retirement journal. No R2 effects or browser process ownership here.
//! A single immutable intent binds original-current recovery, successor CAS,
//! old exact descriptor, authentic device, and delayed R2 DELETE eligibility.

use super::D1ProfileGenerationSuccessorCommitJournal;
use application_ports::generation_objects::GenerationObjectCatalogReference;
use profile_platform_primitives::{ActorContext, DeviceId, TenantScope};
use serde::Deserialize;
use worker::{Error, Result, query};

const ADMIT: &str = r#"
INSERT INTO profile_generation_retirement_intents (
 tenant_id,profile_id,generation_id,observed_current_generation_id,
 expected_profile_version,object_key,metadata_digest,container_digest,
 current_object_key,current_metadata_digest,current_container_digest,
 sealed_current_native_sha256,restored_current_native_sha256,
 admitted_by_actor_id,admitted_by_device_id,admitted_at_ms,not_before_at_ms
) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
"#;

const LOAD_EXACT: &str = r#"
SELECT generation_id,observed_current_generation_id,expected_profile_version,
 object_key,metadata_digest,container_digest,
 current_object_key,current_metadata_digest,current_container_digest,
 sealed_current_native_sha256,restored_current_native_sha256,
 admitted_by_actor_id,admitted_by_device_id
FROM profile_generation_retirement_intents
WHERE tenant_id=? AND profile_id=? AND generation_id=?
"#;

const DUE: &str = r#"
SELECT tenant_id,profile_id,generation_id,object_key,metadata_digest,
 container_digest,not_before_at_ms
FROM profile_generation_retirement_intents
WHERE deleted_at_ms IS NULL AND not_before_at_ms <= ?
ORDER BY not_before_at_ms,tenant_id,profile_id,generation_id
LIMIT 8
"#;

const RECORD_ABSENCE: &str = r#"
UPDATE profile_generation_retirement_intents
SET deleted_at_ms=?
WHERE tenant_id=? AND profile_id=? AND generation_id=?
 AND object_key=? AND metadata_digest=? AND container_digest=?
 AND deleted_at_ms IS NULL AND not_before_at_ms <= ?
RETURNING generation_id
"#;

const REPLAY_ABSENCE: &str = r#"
SELECT generation_id FROM profile_generation_retirement_intents
WHERE tenant_id=? AND profile_id=? AND generation_id=?
 AND object_key=? AND metadata_digest=? AND container_digest=?
 AND deleted_at_ms IS NOT NULL
"#;

#[derive(Deserialize)]
struct Existing {
    generation_id: String,
    observed_current_generation_id: String,
    expected_profile_version: i64,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    current_object_key: String,
    current_metadata_digest: String,
    current_container_digest: String,
    sealed_current_native_sha256: String,
    restored_current_native_sha256: String,
    admitted_by_actor_id: String,
    admitted_by_device_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DueCurrentRetirement {
    pub tenant_id: String,
    pub profile_id: String,
    pub generation_id: String,
    pub object_key: String,
    pub metadata_digest: String,
    pub container_digest: String,
    pub not_before_at_ms: i64,
}

impl D1ProfileGenerationSuccessorCommitJournal {
    /// Worker must first validate BPGC metadata against verified CURRENT
    /// and the device's *independent* complete native cold-restore digest.
    /// The SQL trigger re-proves tenant/version/CURRENT, active binding,
    /// device grants, idle coordinator and absence of other-device recovery
    /// grants/jobs, then revokes only this machine's obsolete old grant.
    #[allow(clippy::too_many_arguments)]
    pub async fn admit_cold_current_retirement(
        &self,
        actor: &ActorContext,
        device: &DeviceId,
        old: &GenerationObjectCatalogReference,
        current: &GenerationObjectCatalogReference,
        expected_version: u64,
        sealed_native_sha256: &str,
        restored_native_sha256: &str,
        now_ms: u64,
    ) -> Result<()> {
        if old.profile_id() != current.profile_id()
            || old.generation_id() == current.generation_id()
            || sealed_native_sha256 != restored_native_sha256
        {
            return Err(invalid("invalid cold CURRENT retirement identity"));
        }
        let version = signed(expected_version)?;
        let now = signed(now_ms)?;
        let not_before = now
            .checked_add(360_000)
            .ok_or_else(|| invalid("cold retirement time overflow"))?;
        let tenant = actor.tenant_scope().tenant_id().as_str();
        let profile = current.profile_id().as_str();
        let old_id = old.generation_id().as_str();
        let result = query!(
            &self.database,
            ADMIT,
            tenant,
            profile,
            old_id,
            current.generation_id().as_str(),
            version,
            old.object_key(),
            old.metadata_digest(),
            old.container_digest(),
            current.object_key(),
            current.metadata_digest(),
            current.container_digest(),
            sealed_native_sha256,
            restored_native_sha256,
            actor.actor_id().as_str(),
            device.as_str(),
            now,
            not_before,
        )?
        .run()
        .await;
        if result.is_ok() {
            return Ok(());
        }
        // A lost HTTP reply is replay-safe only for an exact immutable D1
        // identity. Never grant a second DELETE on changed CURRENT/authority.
        let existing = query!(&self.database, LOAD_EXACT, tenant, profile, old_id)?
            .first::<Existing>(None)
            .await?;
        if let Some(row) = existing
            && row.generation_id == old_id
            && row.observed_current_generation_id == current.generation_id().as_str()
            && row.expected_profile_version == version
            && row.object_key == old.object_key()
            && row.metadata_digest == old.metadata_digest()
            && row.container_digest == old.container_digest()
            && row.current_object_key == current.object_key()
            && row.current_metadata_digest == current.metadata_digest()
            && row.current_container_digest == current.container_digest()
            && row.sealed_current_native_sha256 == sealed_native_sha256
            && row.restored_current_native_sha256 == restored_native_sha256
            && row.admitted_by_actor_id == actor.actor_id().as_str()
            && row.admitted_by_device_id == device.as_str()
        {
            return Ok(());
        }
        Err(result
            .err()
            .unwrap_or_else(|| invalid("retirement admission missing")))
    }

    /// The existing single Worker scheduler replays only D1-admitted, fully
    /// expired exact-key retirement intents. No separate cron is introduced.
    pub async fn due_cold_current_retirements(
        &self,
        now_ms: u64,
    ) -> Result<Vec<DueCurrentRetirement>> {
        query!(&self.database, DUE, signed(now_ms)?)?
            .all()
            .await?
            .results::<DueCurrentRetirement>()
    }

    /// Called only after the existing R2 adapter performed exact HEAD,
    /// DELETE and confirmed strongly consistent HEAD absence. A lost D1
    /// reply replays the same terminal receipt without another generation.
    pub async fn record_cold_retirement_absence(
        &self,
        scope: &TenantScope,
        old: &GenerationObjectCatalogReference,
        now_ms: u64,
    ) -> Result<()> {
        let tenant = scope.tenant_id().as_str();
        let profile = old.profile_id().as_str();
        let generation = old.generation_id().as_str();
        let now = signed(now_ms)?;
        let receipt = query!(
            &self.database,
            RECORD_ABSENCE,
            now,
            tenant,
            profile,
            generation,
            old.object_key(),
            old.metadata_digest(),
            old.container_digest(),
            now,
        )?
        .first::<String>(Some("generation_id"))
        .await?;
        if receipt.as_deref() == Some(generation) {
            return Ok(());
        }
        let previous = query!(
            &self.database,
            REPLAY_ABSENCE,
            tenant,
            profile,
            generation,
            old.object_key(),
            old.metadata_digest(),
            old.container_digest(),
        )?
        .first::<String>(Some("generation_id"))
        .await?;
        if previous.as_deref() == Some(generation) {
            Ok(())
        } else {
            Err(invalid("cold retirement absence receipt was not admitted"))
        }
    }
}

fn invalid(message: &str) -> Error {
    Error::RustError(message.to_owned())
}

fn signed(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| invalid("cold retirement integer overflow"))
}
