use application_ports::device_generation_commit::{
    DeviceGenerationCommitError, DeviceGenerationCommitErrorClass, DeviceGenerationCommitRequest,
};
use profile_platform_primitives::ActorContext;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Database;
use worker::query;

const LOAD_DEVICE_GENERATION_COMMIT: &str = r#"
SELECT
    command.command_actor_id,
    command.device_id,
    command.profile_id,
    command.base_generation_id,
    command.generation_id,
    command.object_key,
    command.metadata_digest,
    command.container_digest,
    command.container_bytes,
    command.expected_job_version,
    command.claim_id,
    command.claim_fence,
    command.expected_profile_version,
    command.coordinator_session_id,
    command.coordinator_fencing_token_digest,
    command.coordinator_epoch,
    command.coordinator_version,
    command.coordinator_sequence,
    command.executed_at_ms,
    profile.active_generation_id,
    profile.status AS profile_status,
    profile.version AS profile_version,
    generation.status AS generation_status,
    generation.version AS generation_version,
    generation.object_key AS generation_object_key,
    generation.metadata_digest AS generation_metadata_digest,
    generation.container_digest AS generation_container_digest,
    generation.verification_reference
FROM device_generation_commit_commands AS command
LEFT JOIN browser_profiles AS profile
  ON profile.tenant_id = command.tenant_id
 AND profile.profile_id = command.profile_id
LEFT JOIN profile_generations AS generation
  ON generation.tenant_id = command.tenant_id
 AND generation.profile_id = command.profile_id
 AND generation.generation_id = command.generation_id
WHERE command.tenant_id = ? AND command.job_id = ?
"#;

const INSERT_DEVICE_GENERATION_COMMIT: &str = r#"
INSERT INTO device_generation_commit_commands (
    tenant_id,
    job_id,
    command_actor_id,
    device_id,
    profile_id,
    base_generation_id,
    generation_id,
    object_key,
    metadata_digest,
    container_digest,
    container_bytes,
    expected_job_version,
    claim_id,
    claim_fence,
    expected_profile_version,
    coordinator_session_id,
    coordinator_fencing_token_digest,
    coordinator_epoch,
    coordinator_version,
    coordinator_sequence,
    executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
RETURNING job_id
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceGenerationCommitJournalOutcome {
    Applied,
    ExactReplay,
}

pub struct D1DeviceGenerationCommitJournal {
    database: D1Database,
}

impl D1DeviceGenerationCommitJournal {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn apply(
        &self,
        actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> Result<DeviceGenerationCommitJournalOutcome, DeviceGenerationCommitError> {
        validate_request(actor, request)?;
        let token_digest = fencing_token_digest(request);

        if let Some(row) = self.load(actor, request).await? {
            return classify_existing(&row, actor, request, &token_digest);
        }

        let object = request.object();
        let insert = query!(
            &self.database,
            INSERT_DEVICE_GENERATION_COMMIT,
            actor.tenant_scope().tenant_id().as_str(),
            request.job_id().as_str(),
            actor.actor_id().as_str(),
            request.device_id().as_str(),
            request.profile_id().as_str(),
            request.base_generation_id().as_str(),
            object.generation_id().as_str(),
            object.object_key(),
            object.metadata_digest(),
            object.container_digest(),
            u64_to_i64(object.container_bytes())?,
            u64_to_i64(request.expected_job_version().value())?,
            request.claim_id().as_str(),
            u64_to_i64(request.claim_fence())?,
            u64_to_i64(request.expected_profile_version().value())?,
            request.coordinator().session_id().as_str(),
            token_digest.as_str(),
            u64_to_i64(request.coordinator().epoch())?,
            u64_to_i64(request.coordinator().coordinator_version())?,
            u64_to_i64(request.coordinator().coordinator_sequence())?,
            u64_to_i64(request.observed_at().value())?,
        )
        .map_err(|_| dependency_failure())?
        .first::<String>(Some("job_id"))
        .await;

        match insert {
            Ok(Some(_)) => {
                let row = self.load(actor, request).await?.ok_or_else(integrity_failure)?;
                if exact_row(&row, actor, request, &token_digest) && catalog_is_exact(&row, request) {
                    Ok(DeviceGenerationCommitJournalOutcome::Applied)
                } else {
                    Err(integrity_failure())
                }
            }
            Ok(None) => Err(integrity_failure()),
            Err(error) => {
                if let Some(row) = self.load(actor, request).await? {
                    return classify_existing(&row, actor, request, &token_digest);
                }
                Err(classify_insert_failure(&error.to_string()))
            }
        }
    }

    async fn load(
        &self,
        actor: &ActorContext,
        request: &DeviceGenerationCommitRequest,
    ) -> Result<Option<DeviceGenerationCommitRow>, DeviceGenerationCommitError> {
        query!(
            &self.database,
            LOAD_DEVICE_GENERATION_COMMIT,
            actor.tenant_scope().tenant_id().as_str(),
            request.job_id().as_str(),
        )
        .map_err(|_| dependency_failure())?
        .first::<DeviceGenerationCommitRow>(None)
        .await
        .map_err(|_| dependency_failure())
    }
}

#[derive(Deserialize)]
struct DeviceGenerationCommitRow {
    command_actor_id: String,
    device_id: String,
    profile_id: String,
    base_generation_id: String,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: i64,
    expected_job_version: i64,
    claim_id: String,
    claim_fence: i64,
    expected_profile_version: i64,
    coordinator_session_id: String,
    coordinator_fencing_token_digest: String,
    coordinator_epoch: i64,
    coordinator_version: i64,
    coordinator_sequence: i64,
    executed_at_ms: i64,
    active_generation_id: Option<String>,
    profile_status: Option<String>,
    profile_version: Option<i64>,
    generation_status: Option<String>,
    generation_version: Option<i64>,
    generation_object_key: Option<String>,
    generation_metadata_digest: Option<String>,
    generation_container_digest: Option<String>,
    verification_reference: Option<String>,
}

fn classify_existing(
    row: &DeviceGenerationCommitRow,
    actor: &ActorContext,
    request: &DeviceGenerationCommitRequest,
    token_digest: &str,
) -> Result<DeviceGenerationCommitJournalOutcome, DeviceGenerationCommitError> {
    if !exact_row(row, actor, request, token_digest) {
        return Err(version_conflict());
    }
    if !catalog_is_exact(row, request) {
        return Err(version_conflict());
    }
    Ok(DeviceGenerationCommitJournalOutcome::ExactReplay)
}

fn exact_row(
    row: &DeviceGenerationCommitRow,
    actor: &ActorContext,
    request: &DeviceGenerationCommitRequest,
    token_digest: &str,
) -> bool {
    let object = request.object();
    row.command_actor_id == actor.actor_id().as_str()
        && row.device_id == request.device_id().as_str()
        && row.profile_id == request.profile_id().as_str()
        && row.base_generation_id == request.base_generation_id().as_str()
        && row.generation_id == object.generation_id().as_str()
        && row.object_key == object.object_key()
        && row.metadata_digest == object.metadata_digest()
        && row.container_digest == object.container_digest()
        && i64_matches_u64(row.container_bytes, object.container_bytes())
        && i64_matches_u64(row.expected_job_version, request.expected_job_version().value())
        && row.claim_id == request.claim_id().as_str()
        && i64_matches_u64(row.claim_fence, request.claim_fence())
        && i64_matches_u64(
            row.expected_profile_version,
            request.expected_profile_version().value(),
        )
        && row.coordinator_session_id == request.coordinator().session_id().as_str()
        && row.coordinator_fencing_token_digest == token_digest
        && i64_matches_u64(row.coordinator_epoch, request.coordinator().epoch())
        && i64_matches_u64(
            row.coordinator_version,
            request.coordinator().coordinator_version(),
        )
        && i64_matches_u64(
            row.coordinator_sequence,
            request.coordinator().coordinator_sequence(),
        )
        && i64_matches_u64(row.executed_at_ms, request.observed_at().value())
}

fn catalog_is_exact(row: &DeviceGenerationCommitRow, request: &DeviceGenerationCommitRequest) -> bool {
    let object = request.object();
    row.active_generation_id.as_deref() == Some(object.generation_id().as_str())
        && row.profile_status.as_deref() == Some("READY")
        && row.profile_version.is_some_and(|version| {
            i64_matches_u64(version, request.expected_profile_version().value().saturating_add(1))
        })
        && row.generation_status.as_deref() == Some("VERIFIED")
        && row.generation_version == Some(2)
        && row.generation_object_key.as_deref() == Some(object.object_key())
        && row.generation_metadata_digest.as_deref() == Some(object.metadata_digest())
        && row.generation_container_digest.as_deref() == Some(object.container_digest())
        && row.verification_reference.as_deref()
            == Some(format!("r2sha256:{}", object.container_digest()).as_str())
}

fn validate_request(
    actor: &ActorContext,
    request: &DeviceGenerationCommitRequest,
) -> Result<(), DeviceGenerationCommitError> {
    let object = request.object();
    let next_profile_version = request
        .expected_profile_version()
        .value()
        .checked_add(1)
        .ok_or_else(integrity_failure)?;
    let expected_coordinator_version = request
        .coordinator()
        .coordinator_sequence()
        .checked_add(1)
        .ok_or_else(integrity_failure)?;
    let canonical_key = format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        actor.tenant_scope().tenant_id().as_str(),
        request.profile_id().as_str(),
        object.generation_id().as_str(),
    );
    if next_profile_version == 0
        || object.profile_id() != request.profile_id()
        || object.generation_id() == request.base_generation_id()
        || object.object_key() != canonical_key
        || object.container_bytes() == 0
        || request.claim_fence() == 0
        || request.coordinator().epoch() == 0
        || request.coordinator().coordinator_sequence() == 0
        || request.coordinator().coordinator_version() != expected_coordinator_version
        || !is_sha256_hex(object.metadata_digest())
        || !is_sha256_hex(object.container_digest())
    {
        return Err(integrity_failure());
    }
    Ok(())
}

fn fencing_token_digest(request: &DeviceGenerationCommitRequest) -> String {
    let digest = Sha256::digest(request.coordinator().fencing_token().as_str().as_bytes());
    let mut output = String::with_capacity(64);
    for byte in digest {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn i64_matches_u64(value: i64, expected: u64) -> bool {
    u64::try_from(value).ok() == Some(expected)
}

fn u64_to_i64(value: u64) -> Result<i64, DeviceGenerationCommitError> {
    i64::try_from(value).map_err(|_| integrity_failure())
}

fn classify_insert_failure(message: &str) -> DeviceGenerationCommitError {
    if message.contains("device_generation_commit_actor_inactive")
        || message.contains("device_generation_commit_device_binding_mismatch")
        || message.contains("device_generation_commit_authorization_stale")
        || message.contains("device_generation_commit_claim_stale")
        || message.contains("device_generation_commit_coordinator_stale")
    {
        return stale_authority();
    }
    if message.contains("device_generation_commit_base_generation_stale")
        || message.contains("device_generation_commit_candidate_exists")
        || message.contains("device_generation_commit_apply_incomplete")
        || message.contains("UNIQUE constraint failed")
    {
        return version_conflict();
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("device_generation_commit_verify_incomplete")
    {
        return integrity_failure();
    }
    dependency_failure()
}

fn stale_authority() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::StaleAuthority)
}

fn version_conflict() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::VersionConflict)
}

fn integrity_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

fn dependency_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{classify_insert_failure, is_sha256_hex};
    use application_ports::device_generation_commit::DeviceGenerationCommitErrorClass;

    #[test]
    fn failure_mapping_preserves_authority_version_and_integrity_classes() {
        assert_eq!(
            classify_insert_failure("device_generation_commit_claim_stale").class(),
            DeviceGenerationCommitErrorClass::StaleAuthority
        );
        assert_eq!(
            classify_insert_failure("device_generation_commit_base_generation_stale").class(),
            DeviceGenerationCommitErrorClass::VersionConflict
        );
        assert_eq!(
            classify_insert_failure("CHECK constraint failed").class(),
            DeviceGenerationCommitErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_insert_failure("network unavailable").class(),
            DeviceGenerationCommitErrorClass::DependencyUnavailable
        );
    }

    #[test]
    fn digest_shape_is_canonical_lowercase_sha256() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
    }
}
