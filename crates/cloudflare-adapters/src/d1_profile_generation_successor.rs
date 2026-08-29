use application_ports::profile_generation_successor::{
    ProfileGenerationSuccessorCommitError, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitPort,
    ProfileGenerationSuccessorCommitRequest,
};
use profile_platform_primitives::{ActorContext, FencingToken, TenantId};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Database;
use worker::query;

const LOAD_SUCCESSOR: &str = r#"
SELECT
    command.command_actor_id,
    command.device_id,
    command.authority_kind,
    command.profile_id,
    command.base_generation_id,
    command.generation_id,
    command.object_key,
    command.metadata_digest,
    command.container_digest,
    command.container_bytes,
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
FROM profile_generation_successor_commands AS command
LEFT JOIN browser_profiles AS profile
  ON profile.tenant_id = command.tenant_id
 AND profile.profile_id = command.profile_id
LEFT JOIN profile_generations AS generation
  ON generation.tenant_id = command.tenant_id
 AND generation.profile_id = command.profile_id
 AND generation.generation_id = command.generation_id
WHERE command.tenant_id = ?
  AND command.profile_id = ?
  AND command.base_generation_id = ?
"#;

const INSERT_INTERACTIVE_SUCCESSOR: &str = r#"
INSERT INTO profile_generation_successor_commands (
    tenant_id,
    profile_id,
    base_generation_id,
    generation_id,
    command_actor_id,
    device_id,
    authority_kind,
    object_key,
    metadata_digest,
    container_digest,
    container_bytes,
    expected_profile_version,
    coordinator_session_id,
    coordinator_fencing_token_digest,
    coordinator_epoch,
    coordinator_version,
    coordinator_sequence,
    executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, 'INTERACTIVE_LAUNCH', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
RETURNING generation_id
"#;

pub struct D1ProfileGenerationSuccessorCommitJournal {
    database: D1Database,
}

impl D1ProfileGenerationSuccessorCommitJournal {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn load(
        &self,
        tenant_id: &TenantId,
        request: &ProfileGenerationSuccessorCommitRequest,
    ) -> Result<Option<SuccessorRow>, ProfileGenerationSuccessorCommitError> {
        query!(
            &self.database,
            LOAD_SUCCESSOR,
            tenant_id.as_str(),
            request.profile_id().as_str(),
            request.base_generation_id().as_str(),
        )
        .map_err(|_| dependency_failure())?
        .first::<SuccessorRow>(None)
        .await
        .map_err(|_| dependency_failure())
    }

    async fn apply_interactive(
        &self,
        actor: &ActorContext,
        request: &ProfileGenerationSuccessorCommitRequest,
    ) -> Result<ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitError>
    {
        validate_request(actor, request)?;
        let token_digest = fencing_token_digest(request.coordinator().fencing_token());

        if let Some(row) = self.load(actor.tenant_scope().tenant_id(), request).await? {
            return classify_existing(&row, actor, request, &token_digest);
        }

        let object = request.object();
        let container_bytes = u64_to_i64(object.container_bytes())?;
        let expected_profile_version = u64_to_i64(request.expected_profile_version().value())?;
        let coordinator_epoch = u64_to_i64(request.coordinator().epoch())?;
        let coordinator_version = u64_to_i64(request.coordinator().coordinator_version())?;
        let coordinator_sequence = u64_to_i64(request.coordinator().coordinator_sequence())?;
        let executed_at_ms = u64_to_i64(request.observed_at().value())?;

        let insert = query!(
            &self.database,
            INSERT_INTERACTIVE_SUCCESSOR,
            actor.tenant_scope().tenant_id().as_str(),
            request.profile_id().as_str(),
            request.base_generation_id().as_str(),
            object.generation_id().as_str(),
            actor.actor_id().as_str(),
            request.device_id().as_str(),
            object.object_key(),
            object.metadata_digest(),
            object.container_digest(),
            container_bytes,
            expected_profile_version,
            request.coordinator().session_id().as_str(),
            token_digest.as_str(),
            coordinator_epoch,
            coordinator_version,
            coordinator_sequence,
            executed_at_ms,
        )
        .map_err(|_| dependency_failure())?
        .first::<String>(Some("generation_id"))
        .await;

        match insert {
            Ok(Some(_)) => {
                let row = self
                    .load(actor.tenant_scope().tenant_id(), request)
                    .await?
                    .ok_or_else(integrity_failure)?;
                if exact_row(&row, actor, request, &token_digest)
                    && committed_state_is_exact(&row)
                {
                    Ok(ProfileGenerationSuccessorCommitOutcome::Activated)
                } else {
                    Err(integrity_failure())
                }
            }
            Ok(None) => Err(integrity_failure()),
            Err(error) => {
                if let Some(row) = self.load(actor.tenant_scope().tenant_id(), request).await? {
                    return classify_existing(&row, actor, request, &token_digest);
                }
                Err(classify_insert_failure(&error.to_string()))
            }
        }
    }
}

impl ProfileGenerationSuccessorCommitPort for D1ProfileGenerationSuccessorCommitJournal {
    async fn commit_profile_generation_successor(
        &self,
        actor: &ActorContext,
        request: &ProfileGenerationSuccessorCommitRequest,
    ) -> Result<ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitError>
    {
        self.apply_interactive(actor, request).await
    }
}

#[derive(Deserialize)]
struct SuccessorRow {
    command_actor_id: String,
    device_id: String,
    authority_kind: String,
    profile_id: String,
    base_generation_id: String,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: i64,
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
    row: &SuccessorRow,
    actor: &ActorContext,
    request: &ProfileGenerationSuccessorCommitRequest,
    token_digest: &str,
) -> Result<ProfileGenerationSuccessorCommitOutcome, ProfileGenerationSuccessorCommitError> {
    if !exact_row(row, actor, request, token_digest) || !committed_state_is_exact(row) {
        return Err(version_conflict());
    }
    Ok(ProfileGenerationSuccessorCommitOutcome::AlreadyActive)
}

fn exact_row(
    row: &SuccessorRow,
    actor: &ActorContext,
    request: &ProfileGenerationSuccessorCommitRequest,
    token_digest: &str,
) -> bool {
    let object = request.object();
    row.authority_kind == "INTERACTIVE_LAUNCH"
        && row.command_actor_id == actor.actor_id().as_str()
        && row.device_id == request.device_id().as_str()
        && row.profile_id == request.profile_id().as_str()
        && row.base_generation_id == request.base_generation_id().as_str()
        && row.generation_id == object.generation_id().as_str()
        && row.object_key == object.object_key()
        && row.metadata_digest == object.metadata_digest()
        && row.container_digest == object.container_digest()
        && i64_matches_u64(row.container_bytes, object.container_bytes())
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

fn committed_state_is_exact(row: &SuccessorRow) -> bool {
    let Some(expected_profile_version) =
        non_negative_u64(row.expected_profile_version).and_then(|value| value.checked_add(1))
    else {
        return false;
    };
    let expected_verification = format!("r2sha256:{}", row.container_digest);

    row.active_generation_id.as_deref() == Some(row.generation_id.as_str())
        && row.profile_status.as_deref() == Some("READY")
        && row
            .profile_version
            .is_some_and(|value| i64_matches_u64(value, expected_profile_version))
        && row.generation_status.as_deref() == Some("VERIFIED")
        && row.generation_version == Some(2)
        && row.generation_object_key.as_deref() == Some(row.object_key.as_str())
        && row.generation_metadata_digest.as_deref() == Some(row.metadata_digest.as_str())
        && row.generation_container_digest.as_deref() == Some(row.container_digest.as_str())
        && row.verification_reference.as_deref() == Some(expected_verification.as_str())
}

fn validate_request(
    actor: &ActorContext,
    request: &ProfileGenerationSuccessorCommitRequest,
) -> Result<(), ProfileGenerationSuccessorCommitError> {
    let object = request.object();
    let expected_coordinator_version = request
        .coordinator()
        .coordinator_sequence()
        .checked_add(1)
        .ok_or_else(integrity_failure)?;
    let canonical = format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        actor.tenant_scope().tenant_id().as_str(),
        request.profile_id().as_str(),
        object.generation_id().as_str(),
    );
    if object.profile_id() != request.profile_id()
        || object.generation_id() == request.base_generation_id()
        || object.object_key() != canonical
        || object.container_bytes() == 0
        || !is_sha256_hex(object.metadata_digest())
        || !is_sha256_hex(object.container_digest())
        || request
            .expected_profile_version()
            .value()
            .checked_add(1)
            .is_none()
        || request.coordinator().epoch() == 0
        || request.coordinator().coordinator_sequence() == 0
        || request.coordinator().coordinator_version() != expected_coordinator_version
    {
        return Err(integrity_failure());
    }
    Ok(())
}

fn fencing_token_digest(token: &FencingToken) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"profile-generation-successor-fencing-token-v1\n");
    hasher.update(token.as_str().as_bytes());
    hex_digest(hasher.finalize().into())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_digest(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn u64_to_i64(value: u64) -> Result<i64, ProfileGenerationSuccessorCommitError> {
    i64::try_from(value).map_err(|_| integrity_failure())
}

fn non_negative_u64(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

fn i64_matches_u64(left: i64, right: u64) -> bool {
    non_negative_u64(left) == Some(right)
}

fn classify_insert_failure(message: &str) -> ProfileGenerationSuccessorCommitError {
    if message.contains("profile_generation_successor_actor_inactive")
        || message.contains("profile_generation_successor_profile_access_denied")
        || message.contains("profile_generation_successor_device_binding_mismatch")
        || message.contains("profile_generation_successor_device_authorization_stale")
        || message.contains("profile_generation_successor_coordinator_stale")
    {
        return stale_authority();
    }
    if message.contains("profile_generation_successor_base_generation_stale")
        || message.contains("profile_generation_successor_candidate_exists")
        || message.contains("UNIQUE constraint failed")
    {
        return version_conflict();
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("profile_generation_successor_device_job_authority_missing")
        || message.contains("profile_generation_insert_not_governed")
        || message.contains("profile_generation_transition_not_governed")
        || message.contains("profile_generation_activation_not_governed")
        || message.contains("profile_generation_successor_verify_incomplete")
        || message.contains("profile_generation_successor_activate_incomplete")
    {
        return integrity_failure();
    }
    dependency_failure()
}

const fn stale_authority() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::StaleAuthority,
    )
}

const fn version_conflict() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::VersionConflict,
    )
}

const fn integrity_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure,
    )
}

const fn dependency_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::DependencyUnavailable,
    )
}

#[cfg(test)]
mod tests {
    use super::{is_sha256_hex, validate_request};
    use application_ports::generation_objects::GenerationObjectDescriptor;
    use application_ports::profile_generation_successor::{
        ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitRequest,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken,
        GenerationId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
    };

    #[test]
    fn request_requires_exact_successor_and_coordinator_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_successor_01")?;
        let profile_id = ProfileId::parse("profile_successor_01")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse("actor_successor_01")?,
            CorrelationId::parse("corr_successor_01")?,
        );
        let request = ProfileGenerationSuccessorCommitRequest::new(
            DeviceId::parse("device_successor_01")?,
            profile_id.clone(),
            GenerationId::parse("generation_successor_base_01")?,
            GenerationObjectDescriptor::new(
                profile_id.clone(),
                GenerationId::parse("generation_successor_next_01")?,
                format!(
                    "tenants/{}/profiles/{}/generations/generation_successor_next_01.bpgc",
                    tenant_id.as_str(),
                    profile_id.as_str(),
                ),
                "a".repeat(64),
                "b".repeat(64),
                4096,
            ),
            AggregateVersion::new(7)?,
            ProfileGenerationCommitWitness::new(
                SessionId::parse("session_successor_01")?,
                FencingToken::parse("fence_successor_01")?,
                3,
                11,
                10,
            ),
            UnixMillis::new(100),
        );
        assert!(validate_request(&actor, &request).is_ok());
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)));
        Ok(())
    }
}
