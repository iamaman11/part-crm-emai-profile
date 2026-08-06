use crate::d1_identity_acl::{MutationEnvelope, ResolvedMembershipRole};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, GenerationId, ProfileId, TenantScope,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1PreparedStatement, D1Result};
use worker::{Error, Result, query};

const REGISTER_COMMAND: &str = r#"
INSERT INTO profile_generation_register_commands (
    tenant_id, command_id, command_actor_id, profile_id,
    generation_id, object_key, metadata_digest, container_digest, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const VERIFY_COMMAND: &str = r#"
INSERT INTO profile_generation_verify_commands (
    tenant_id, command_id, command_actor_id, profile_id,
    generation_id, expected_generation_version,
    verification_reference, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

const ACTIVATE_COMMAND: &str = r#"
INSERT INTO profile_generation_activate_commands (
    tenant_id, command_id, command_actor_id, profile_id,
    generation_id, expected_profile_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const DEACTIVATE_COMMAND: &str = r#"
INSERT INTO profile_generation_deactivate_commands (
    tenant_id, command_id, command_actor_id, profile_id,
    generation_id, expected_profile_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const QUARANTINE_COMMAND: &str = r#"
INSERT INTO profile_generation_quarantine_commands (
    tenant_id, command_id, command_actor_id, profile_id,
    generation_id, expected_generation_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

pub struct RegisterGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub object_key: &'a str,
    pub metadata_digest: &'a str,
    pub container_digest: &'a str,
    pub envelope: MutationEnvelope<'a>,
}

pub struct VerifyGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub expected_generation_version: AggregateVersion,
    pub verification_reference: &'a str,
    pub envelope: MutationEnvelope<'a>,
}

pub struct ActivateGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub expected_profile_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct DeactivateGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub expected_profile_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct QuarantineGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub expected_generation_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationStatus {
    Registered,
    Verified,
    Quarantined,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationProjection {
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    status: GenerationStatus,
    version: AggregateVersion,
    verification_reference: Option<String>,
}

impl GenerationProjection {
    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn status(&self) -> GenerationStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub fn verification_reference(&self) -> Option<&str> {
        self.verification_reference.as_deref()
    }
}

struct CommandEvidence<'a> {
    command_name: &'static str,
    result_code: &'static str,
    resource_id: &'a str,
    aggregate_type: &'static str,
    aggregate_id: &'a str,
    aggregate_version: i64,
    event_type: &'static str,
}

pub struct D1ProfileGenerationRepository {
    database: D1Database,
}

impl D1ProfileGenerationRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn register(
        &self,
        actor: &ActorContext,
        mutation: RegisterGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        validate_object_key(mutation.object_key)?;
        validate_digest(mutation.metadata_digest)?;
        validate_digest(mutation.container_digest)?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let command = query!(
            &self.database,
            REGISTER_COMMAND,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.envelope.idempotency_key.as_str(),
            actor.actor_id().as_str(),
            mutation.profile_id.as_str(),
            mutation.generation_id.as_str(),
            mutation.object_key,
            mutation.metadata_digest,
            mutation.container_digest,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            CommandEvidence {
                command_name: "profile_generation.register",
                result_code: "registered",
                resource_id: mutation.generation_id.as_str(),
                aggregate_type: "profile_generation",
                aggregate_id: mutation.generation_id.as_str(),
                aggregate_version: 1,
                event_type: "profile_generation.registered.v1",
            },
        )
        .await
    }

    pub async fn verify(
        &self,
        actor: &ActorContext,
        mutation: VerifyGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        validate_verification_reference(mutation.verification_reference)?;
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let aggregate_version = next_version_value(mutation.expected_generation_version)?;
        let command = query!(
            &self.database,
            VERIFY_COMMAND,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.envelope.idempotency_key.as_str(),
            actor.actor_id().as_str(),
            mutation.profile_id.as_str(),
            mutation.generation_id.as_str(),
            sqlite_version(mutation.expected_generation_version)?,
            mutation.verification_reference,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            CommandEvidence {
                command_name: "profile_generation.verify",
                result_code: "verified",
                resource_id: mutation.generation_id.as_str(),
                aggregate_type: "profile_generation",
                aggregate_id: mutation.generation_id.as_str(),
                aggregate_version,
                event_type: "profile_generation.verified.v1",
            },
        )
        .await
    }

    pub async fn activate(
        &self,
        actor: &ActorContext,
        mutation: ActivateGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let aggregate_version = next_version_value(mutation.expected_profile_version)?;
        let command = query!(
            &self.database,
            ACTIVATE_COMMAND,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.envelope.idempotency_key.as_str(),
            actor.actor_id().as_str(),
            mutation.profile_id.as_str(),
            mutation.generation_id.as_str(),
            sqlite_version(mutation.expected_profile_version)?,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            CommandEvidence {
                command_name: "profile_generation.activate",
                result_code: "activated",
                resource_id: mutation.generation_id.as_str(),
                aggregate_type: "profile",
                aggregate_id: mutation.profile_id.as_str(),
                aggregate_version,
                event_type: "profile.generation_activated.v1",
            },
        )
        .await
    }

    pub async fn deactivate(
        &self,
        actor: &ActorContext,
        mutation: DeactivateGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let aggregate_version = next_version_value(mutation.expected_profile_version)?;
        let command = query!(
            &self.database,
            DEACTIVATE_COMMAND,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.envelope.idempotency_key.as_str(),
            actor.actor_id().as_str(),
            mutation.profile_id.as_str(),
            mutation.generation_id.as_str(),
            sqlite_version(mutation.expected_profile_version)?,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            CommandEvidence {
                command_name: "profile_generation.deactivate",
                result_code: "deactivated",
                resource_id: mutation.generation_id.as_str(),
                aggregate_type: "profile",
                aggregate_id: mutation.profile_id.as_str(),
                aggregate_version,
                event_type: "profile.generation_deactivated.v1",
            },
        )
        .await
    }

    pub async fn quarantine(
        &self,
        actor: &ActorContext,
        mutation: QuarantineGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let aggregate_version = next_version_value(mutation.expected_generation_version)?;
        let command = query!(
            &self.database,
            QUARANTINE_COMMAND,
            actor.tenant_scope().tenant_id().as_str(),
            mutation.envelope.idempotency_key.as_str(),
            actor.actor_id().as_str(),
            mutation.profile_id.as_str(),
            mutation.generation_id.as_str(),
            sqlite_version(mutation.expected_generation_version)?,
            now
        )?;
        self.execute(
            actor,
            &mutation.envelope,
            command,
            CommandEvidence {
                command_name: "profile_generation.quarantine",
                result_code: "quarantined",
                resource_id: mutation.generation_id.as_str(),
                aggregate_type: "profile_generation",
                aggregate_id: mutation.generation_id.as_str(),
                aggregate_version,
                event_type: "profile_generation.quarantined.v1",
            },
        )
        .await
    }

    async fn execute(
        &self,
        actor: &ActorContext,
        envelope: &MutationEnvelope<'_>,
        command: D1PreparedStatement,
        evidence: CommandEvidence<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(envelope.now.value())?;
        let expires_at = sqlite_integer(envelope.idempotency_expires_at.value())?;
        let statements = vec![
            command,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                evidence.command_name,
                evidence.result_code,
                evidence.resource_id,
                envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                evidence.command_name,
                evidence.resource_id,
                evidence.result_code,
                envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                evidence.aggregate_type,
                evidence.aggregate_id,
                evidence.aggregate_version,
                evidence.event_type,
                envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn find_visible(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: ResolvedMembershipRole,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<Option<GenerationProjection>> {
        let owner = i32::from(role == ResolvedMembershipRole::TenantOwner);
        let row = query!(
            &self.database,
            r#"
            SELECT
                generation_id, object_key, metadata_digest, container_digest,
                status, version, verification_reference
            FROM profile_generations AS generation
            WHERE generation.tenant_id = ?
              AND generation.profile_id = ?
              AND generation.generation_id = ?
              AND (
                  ? = 1
                  OR EXISTS (
                      SELECT 1 FROM profile_grants AS grant
                      WHERE grant.tenant_id = generation.tenant_id
                        AND grant.profile_id = generation.profile_id
                        AND grant.actor_id = ?
                  )
              )
            "#,
            scope.tenant_id().as_str(),
            profile_id.as_str(),
            generation_id.as_str(),
            owner,
            actor_id.as_str()
        )?
        .first::<GenerationProjectionRow>(None)
        .await?;
        row.map(generation_projection).transpose()
    }
}

#[derive(Deserialize)]
struct GenerationProjectionRow {
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    status: String,
    version: i64,
    verification_reference: Option<String>,
}

fn generation_projection(row: GenerationProjectionRow) -> Result<GenerationProjection> {
    validate_object_key(&row.object_key)?;
    validate_digest(&row.metadata_digest)?;
    validate_digest(&row.container_digest)?;
    if let Some(reference) = row.verification_reference.as_deref() {
        validate_verification_reference(reference)?;
    }
    Ok(GenerationProjection {
        generation_id: GenerationId::parse(row.generation_id).map_err(identifier_error)?,
        object_key: row.object_key,
        metadata_digest: row.metadata_digest,
        container_digest: row.container_digest,
        status: parse_status(&row.status)?,
        version: AggregateVersion::new(positive_version(row.version)?)
            .map_err(|error| Error::RustError(error.to_string()))?,
        verification_reference: row.verification_reference,
    })
}

fn parse_status(value: &str) -> Result<GenerationStatus> {
    match value {
        "REGISTERED" => Ok(GenerationStatus::Registered),
        "VERIFIED" => Ok(GenerationStatus::Verified),
        "QUARANTINED" => Ok(GenerationStatus::Quarantined),
        _ => Err(Error::RustError(
            "invalid profile generation status".to_owned(),
        )),
    }
}

fn validate_object_key(value: &str) -> Result<()> {
    let valid_length = (16..=512).contains(&value.len());
    let valid_chars = value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
    });
    if !valid_length
        || value.starts_with('/')
        || value.contains("..")
        || value.contains('\\')
        || !valid_chars
    {
        return Err(Error::RustError(
            "invalid profile generation object key".to_owned(),
        ));
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(Error::RustError(
            "invalid lowercase SHA-256 generation digest".to_owned(),
        ));
    }
    Ok(())
}

fn validate_verification_reference(value: &str) -> Result<()> {
    let valid_length = (8..=256).contains(&value.len());
    let valid_chars = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'));
    if !valid_length || !valid_chars {
        return Err(Error::RustError(
            "invalid profile generation verification reference".to_owned(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn idempotency_statement(
    database: &D1Database,
    tenant_id: &str,
    actor_id: &str,
    command_name: &str,
    result_code: &str,
    result_reference: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
    expires_at: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        IDEMPOTENCY_CREATE,
        tenant_id,
        actor_id,
        envelope.idempotency_key.as_str(),
        command_name,
        envelope.request_digest,
        result_code,
        result_reference,
        now,
        expires_at
    )
}

#[allow(clippy::too_many_arguments)]
fn audit_statement(
    database: &D1Database,
    tenant_id: &str,
    correlation_id: &str,
    actor_id: &str,
    action: &str,
    resource_id: &str,
    result_code: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        AUDIT_CREATE,
        tenant_id,
        envelope.audit_event_id.as_str(),
        correlation_id,
        actor_id,
        action,
        "profile_generation",
        resource_id,
        result_code,
        now
    )
}

#[allow(clippy::too_many_arguments)]
fn outbox_statement(
    database: &D1Database,
    tenant_id: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    aggregate_version: i64,
    event_type: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
) -> Result<D1PreparedStatement> {
    query!(
        database,
        OUTBOX_CREATE,
        tenant_id,
        envelope.outbox_event_id.as_str(),
        aggregate_type,
        aggregate_id,
        aggregate_version,
        event_type,
        envelope.payload_json,
        now
    )
}

fn sqlite_version(version: AggregateVersion) -> Result<i64> {
    sqlite_integer(version.value())
}

fn next_version_value(version: AggregateVersion) -> Result<i64> {
    let next = version
        .next()
        .map_err(|_| Error::RustError("aggregate version overflow".to_owned()))?;
    sqlite_integer(next.value())
}

fn positive_version(value: i64) -> Result<u64> {
    let value = u64::try_from(value)
        .map_err(|_| Error::RustError("negative aggregate version".to_owned()))?;
    if value == 0 {
        return Err(Error::RustError("zero aggregate version".to_owned()));
    }
    Ok(value)
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationStatus, parse_status, positive_version, validate_digest, validate_object_key,
        validate_verification_reference,
    };

    #[test]
    fn storage_boundaries_fail_closed() {
        assert_eq!(
            parse_status("VERIFIED").expect("verified status"),
            GenerationStatus::Verified
        );
        assert!(parse_status("UNKNOWN").is_err());
        assert_eq!(positive_version(1).expect("positive version"), 1);
        assert!(positive_version(0).is_err());
        assert!(positive_version(-1).is_err());
        assert!(validate_object_key("profiles/v1/generation.enc").is_ok());
        assert!(validate_object_key("../generation.enc").is_err());
        assert!(validate_object_key("profiles\\generation.enc").is_err());
        assert!(validate_digest(&"a".repeat(64)).is_ok());
        assert!(validate_digest(&"A".repeat(64)).is_err());
        assert!(validate_verification_reference("review:generation_01").is_ok());
        assert!(validate_verification_reference("review generation").is_err());
    }
}
