use crate::d1_identity_acl::{MutationEnvelope, ResolvedMembershipRole};
use profile_domain::registry::{
    GenerationDigest, GenerationObjectKey, GenerationRegistryStatus, VerificationReference,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, GenerationId, ProfileId, TenantScope,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1Result};
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
    pub object_key: &'a GenerationObjectKey,
    pub metadata_digest: &'a GenerationDigest,
    pub container_digest: &'a GenerationDigest,
    pub envelope: MutationEnvelope<'a>,
}

pub struct VerifyGenerationMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub generation_id: &'a GenerationId,
    pub expected_generation_version: AggregateVersion,
    pub verification_reference: &'a VerificationReference,
    pub envelope: MutationEnvelope<'a>,
}

pub struct ActivateGenerationMutation<'a> {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationProjection {
    generation_id: GenerationId,
    object_key: GenerationObjectKey,
    metadata_digest: GenerationDigest,
    container_digest: GenerationDigest,
    status: GenerationRegistryStatus,
    version: AggregateVersion,
    verification_reference: Option<VerificationReference>,
}

impl GenerationProjection {
    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn object_key(&self) -> &GenerationObjectKey {
        &self.object_key
    }

    #[must_use]
    pub const fn metadata_digest(&self) -> &GenerationDigest {
        &self.metadata_digest
    }

    #[must_use]
    pub const fn container_digest(&self) -> &GenerationDigest {
        &self.container_digest
    }

    #[must_use]
    pub const fn status(&self) -> GenerationRegistryStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn verification_reference(&self) -> Option<&VerificationReference> {
        self.verification_reference.as_ref()
    }
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
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let resource_id = mutation.generation_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                REGISTER_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.profile_id.as_str(),
                resource_id,
                mutation.object_key.as_str(),
                mutation.metadata_digest.as_str(),
                mutation.container_digest.as_str(),
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile_generation.register",
                "registered",
                resource_id,
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile_generation.register",
                resource_id,
                "registered",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                mutation.profile_id.as_str(),
                1,
                "profile_generation.registered.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn verify(
        &self,
        actor: &ActorContext,
        mutation: VerifyGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_generation_version)?;
        let aggregate_version = next_version_value(mutation.expected_generation_version)?;
        let resource_id = mutation.generation_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                VERIFY_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.profile_id.as_str(),
                resource_id,
                expected_version,
                mutation.verification_reference.as_str(),
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile_generation.verify",
                "verified",
                resource_id,
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile_generation.verify",
                resource_id,
                "verified",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                mutation.profile_id.as_str(),
                aggregate_version,
                "profile_generation.verified.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn activate(
        &self,
        actor: &ActorContext,
        mutation: ActivateGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_profile_version)?;
        let aggregate_version = next_version_value(mutation.expected_profile_version)?;
        let resource_id = mutation.generation_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                ACTIVATE_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.profile_id.as_str(),
                resource_id,
                expected_version,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile_generation.activate",
                "activated",
                resource_id,
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile_generation.activate",
                resource_id,
                "activated",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                mutation.profile_id.as_str(),
                aggregate_version,
                "profile.generation_activated.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn quarantine(
        &self,
        actor: &ActorContext,
        mutation: QuarantineGenerationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_generation_version)?;
        let aggregate_version = next_version_value(mutation.expected_generation_version)?;
        let resource_id = mutation.generation_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                QUARANTINE_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.profile_id.as_str(),
                resource_id,
                expected_version,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile_generation.quarantine",
                "quarantined",
                resource_id,
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile_generation.quarantine",
                resource_id,
                "quarantined",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                mutation.profile_id.as_str(),
                aggregate_version,
                "profile_generation.quarantined.v1",
                &mutation.envelope,
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
    Ok(GenerationProjection {
        generation_id: GenerationId::parse(row.generation_id).map_err(identifier_error)?,
        object_key: GenerationObjectKey::parse(row.object_key).map_err(registry_error)?,
        metadata_digest: GenerationDigest::parse(row.metadata_digest).map_err(registry_error)?,
        container_digest: GenerationDigest::parse(row.container_digest).map_err(registry_error)?,
        status: parse_status(&row.status)?,
        version: AggregateVersion::new(positive_version(row.version)?)
            .map_err(|error| Error::RustError(error.to_string()))?,
        verification_reference: row
            .verification_reference
            .map(VerificationReference::parse)
            .transpose()
            .map_err(registry_error)?,
    })
}

fn parse_status(value: &str) -> Result<GenerationRegistryStatus> {
    match value {
        "REGISTERED" => Ok(GenerationRegistryStatus::Registered),
        "VERIFIED" => Ok(GenerationRegistryStatus::Verified),
        "QUARANTINED" => Ok(GenerationRegistryStatus::Quarantined),
        _ => Err(Error::RustError(
            "invalid profile generation status".to_owned(),
        )),
    }
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
) -> Result<worker::D1PreparedStatement> {
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
) -> Result<worker::D1PreparedStatement> {
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
    profile_id: &str,
    aggregate_version: i64,
    event_type: &str,
    envelope: &MutationEnvelope<'_>,
    now: i64,
) -> Result<worker::D1PreparedStatement> {
    query!(
        database,
        OUTBOX_CREATE,
        tenant_id,
        envelope.outbox_event_id.as_str(),
        "profile",
        profile_id,
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

fn registry_error(error: profile_domain::registry::GenerationRegistryError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_status, positive_version};
    use profile_domain::registry::GenerationRegistryStatus;

    #[test]
    fn storage_status_and_versions_fail_closed() {
        assert_eq!(
            parse_status("VERIFIED").expect("verified status"),
            GenerationRegistryStatus::Verified
        );
        assert!(parse_status("UNKNOWN").is_err());
        assert_eq!(positive_version(1).expect("positive version"), 1);
        assert!(positive_version(0).is_err());
        assert!(positive_version(-1).is_err());
    }
}
