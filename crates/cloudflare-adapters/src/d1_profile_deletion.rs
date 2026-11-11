//! D1-owned profile delete journal. No R2 effects occur in this module.
//! The one Worker route resumes the exact frozen per-profile inventory.
use application_ports::generation_objects::GenerationObjectCatalogReference;
use profile_platform_primitives::{ActorContext, GenerationId, ProfileId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result, query};

const LOAD_DELETE_COMMAND: &str = r#"
SELECT command_actor_id, expected_profile_version FROM profile_delete_commands
WHERE tenant_id=? AND profile_id=?
"#;
const BEGIN_DELETE: &str = r#"
INSERT INTO profile_delete_commands (
  tenant_id, profile_id, command_id, command_actor_id,
  expected_profile_version, requested_at_ms
) VALUES (?, ?, ?, ?, ?, ?)
"#;
const NEXT_PENDING: &str = r#"
SELECT object.generation_id, object.object_key,
       object.metadata_digest, object.container_digest
FROM profile_delete_objects AS object
LEFT JOIN profile_delete_object_receipts AS receipt
  ON receipt.tenant_id=object.tenant_id AND receipt.profile_id=object.profile_id
 AND receipt.generation_id=object.generation_id
WHERE object.tenant_id=? AND object.profile_id=?
 AND receipt.generation_id IS NULL
ORDER BY object.generation_id LIMIT 1
"#;
const COMPLETE_OBJECT: &str = r#"
INSERT INTO profile_delete_object_receipts (
  tenant_id,profile_id,generation_id,object_key,
  metadata_digest,container_digest,deleted_at_ms
) VALUES (?,?,?,?,?,?,?)
ON CONFLICT(tenant_id,profile_id,generation_id) DO NOTHING
"#;
const PENDING_COUNT: &str = r#"
SELECT COUNT(*) AS pending
FROM profile_delete_objects AS object
WHERE object.tenant_id=? AND object.profile_id=?
 AND NOT EXISTS (
  SELECT 1 FROM profile_delete_object_receipts AS receipt
  WHERE receipt.tenant_id=object.tenant_id AND receipt.profile_id=object.profile_id
   AND receipt.generation_id=object.generation_id
 )
"#;
const COMPLETE_DELETE: &str = r#"
INSERT INTO profile_delete_completion_commands (
 tenant_id,profile_id,command_actor_id,expected_profile_version,completed_at_ms
) VALUES (?,?,?,?,?)
ON CONFLICT(tenant_id,profile_id) DO NOTHING
"#;
// Every old signed PUT is accounted for before deleting a frozen key.
// At least 360s from freeze also drains URLs issued before migration 0041.
// The existing Worker cron can resume the same frozen Delete command; no
// second deletion owner or Cloudflare service is required.
const DUE_FROZEN_DELETES: &str = r#"
SELECT command.tenant_id, command.profile_id, command.command_actor_id,
       command.expected_profile_version
FROM profile_delete_commands AS command
JOIN browser_profiles AS profile
  ON profile.tenant_id=command.tenant_id AND profile.profile_id=command.profile_id
JOIN memberships AS original_owner
  ON original_owner.tenant_id=command.tenant_id
 AND original_owner.actor_id=command.command_actor_id
 AND original_owner.status='ACTIVE'
 AND original_owner.role='TENANT_OWNER'
WHERE profile.status='DELETING'
  AND MAX(command.requested_at_ms + 360000,
          COALESCE((
              SELECT MAX(reservation.expires_at_ms)
              FROM profile_generation_upload_reservations AS reservation
              WHERE reservation.tenant_id = command.tenant_id
                AND reservation.profile_id = command.profile_id
          ),0)) <= ?
ORDER BY command.requested_at_ms, command.tenant_id, command.profile_id
LIMIT 8
"#;
const PROFILE_DELETE_STATUS: &str = r#"
SELECT status FROM browser_profiles WHERE tenant_id=? AND profile_id=?
"#;

#[derive(Deserialize)]
struct StoredCommand {
    command_actor_id: String,
    expected_profile_version: i64,
}
#[derive(Deserialize)]
struct StoredObject {
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
}
#[derive(Deserialize)]
struct Pending {
    pending: i64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct DueProfileDelete {
    pub tenant_id: String,
    pub profile_id: String,
    pub command_actor_id: String,
    pub expected_profile_version: i64,
}
#[derive(Deserialize)]
struct StoredStatus {
    status: String,
}

pub struct D1ProfileDeletion {
    database: D1Database,
}
impl D1ProfileDeletion {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn begin_or_replay(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        expected_version: u64,
        now_ms: u64,
    ) -> Result<()> {
        let tenant = actor.tenant_scope().tenant_id().as_str();
        let profile = profile_id.as_str();
        let version = signed(expected_version)?;
        let existing = self.load_command(actor.tenant_scope(), profile_id).await?;
        if let Some(row) = existing {
            return exact_existing(&row, actor, version);
        }
        let command_id = format!("profile_delete_{}", profile);
        let result = query!(
            &self.database,
            BEGIN_DELETE,
            tenant,
            profile,
            command_id.as_str(),
            actor.actor_id().as_str(),
            version,
            signed(now_ms)?
        )?
        .run()
        .await;
        if let Err(original) = result {
            // Concurrent exact same DELETE is an idempotent continuation.
            if let Some(row) = self.load_command(actor.tenant_scope(), profile_id).await? {
                return exact_existing(&row, actor, version);
            }
            return Err(original);
        }
        Ok(())
    }

    async fn load_command(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<StoredCommand>> {
        query!(
            &self.database,
            LOAD_DELETE_COMMAND,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<StoredCommand>(None)
        .await
    }

    pub async fn next_pending(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<GenerationObjectCatalogReference>> {
        let row = query!(
            &self.database,
            NEXT_PENDING,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<StoredObject>(None)
        .await?;
        row.map(|object| {
            Ok(GenerationObjectCatalogReference::new(
                profile_id.clone(),
                GenerationId::parse(object.generation_id.as_str())
                    .map_err(|_| Error::RustError("invalid frozen generation ID".to_owned()))?,
                object.object_key,
                object.metadata_digest,
                object.container_digest,
            ))
        })
        .transpose()
    }

    pub async fn record_absent_exact(
        &self,
        scope: &TenantScope,
        object: &GenerationObjectCatalogReference,
        now_ms: u64,
    ) -> Result<()> {
        query!(
            &self.database,
            COMPLETE_OBJECT,
            scope.tenant_id().as_str(),
            object.profile_id().as_str(),
            object.generation_id().as_str(),
            object.object_key(),
            object.metadata_digest(),
            object.container_digest(),
            signed(now_ms)?
        )?
        .run()
        .await?;
        Ok(())
    }

    pub async fn pending_count(&self, scope: &TenantScope, profile_id: &ProfileId) -> Result<u64> {
        let count = query!(
            &self.database,
            PENDING_COUNT,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<Pending>(None)
        .await?
        .ok_or_else(|| Error::RustError("profile delete inventory unreadable".to_owned()))?;
        u64::try_from(count.pending)
            .map_err(|_| Error::RustError("invalid pending delete count".to_owned()))
    }

    /// Return only expired, already-admitted DELETE commands. No new
    /// authorization or D1 destructive command is created by the scheduler.
    pub async fn due_frozen_deletes(&self, now_ms: u64) -> Result<Vec<DueProfileDelete>> {
        query!(&self.database, DUE_FROZEN_DELETES, signed(now_ms)?)?
            .all()
            .await?
            .results::<DueProfileDelete>()
    }

    pub async fn complete(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        expected_version: u64,
        now_ms: u64,
    ) -> Result<bool> {
        self.complete_frozen(
            actor.tenant_scope(),
            profile_id,
            actor.actor_id().as_str(),
            expected_version,
            now_ms,
        )
        .await
    }

    /// The signed owner was frozen by D1 in the original Delete command.
    /// Scheduled replay is not a new user authorization; the D1 completion
    /// trigger still verifies that this exact owner is active.
    pub async fn complete_frozen(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        command_actor_id: &str,
        expected_version: u64,
        now_ms: u64,
    ) -> Result<bool> {
        if self.pending_count(scope, profile_id).await? != 0 {
            return Ok(false);
        }
        if self.status(scope, profile_id).await?.as_deref() == Some("DELETED") {
            return Ok(true);
        }
        query!(
            &self.database,
            COMPLETE_DELETE,
            scope.tenant_id().as_str(),
            profile_id.as_str(),
            command_actor_id,
            signed(expected_version.checked_add(1).ok_or_else(|| {
                Error::RustError("profile delete version overflow".to_owned())
            })?)?,
            signed(now_ms)?
        )?
        .run()
        .await?;
        Ok(self.status(scope, profile_id).await?.as_deref() == Some("DELETED"))
    }

    pub async fn status(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<String>> {
        Ok(query!(
            &self.database,
            PROFILE_DELETE_STATUS,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<StoredStatus>(None)
        .await?
        .map(|v| v.status))
    }
}

fn signed(value: u64) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| Error::RustError("invalid profile deletion integer".to_owned()))
}
fn exact_existing(row: &StoredCommand, actor: &ActorContext, expected: i64) -> Result<()> {
    if row.command_actor_id == actor.actor_id().as_str() && row.expected_profile_version == expected
    {
        Ok(())
    } else {
        Err(Error::RustError("profile deletion conflict".to_owned()))
    }
}
