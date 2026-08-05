use crate::access_identity::VerifiedExternalIdentity;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, CorrelationId, IdempotencyKey,
    IdentityId, InvitationId, OutboxEventId, ProfileId, TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const IDEMPOTENCY_LOOKUP: &str = r#"
SELECT result_code, result_reference
FROM idempotency_records
WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
"#;

const OWNER_BOOTSTRAP_TENANT: &str = r#"
INSERT INTO tenants (
    tenant_id, display_name, status, version, created_at_ms, updated_at_ms
) VALUES (?, ?, 'ACTIVE', 1, ?, ?)
"#;

const IDENTITY_CREATE: &str = r#"
INSERT INTO identities (
    identity_id, access_subject, verified_contact_hint, created_at_ms
) VALUES (?, ?, ?, ?)
"#;

const OWNER_BOOTSTRAP_MEMBERSHIP: &str = r#"
INSERT INTO memberships (
    tenant_id, actor_id, identity_id, role, status, version,
    created_at_ms, updated_at_ms
) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, ?, ?)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedMembershipRole {
    TenantOwner,
    Member,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedActor {
    actor: ActorContext,
    role: ResolvedMembershipRole,
}

impl ResolvedActor {
    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        &self.actor
    }

    #[must_use]
    pub const fn role(&self) -> ResolvedMembershipRole {
        self.role
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBootstrapContext {
    scope: TenantScope,
    actor_id: ActorId,
    correlation_id: CorrelationId,
    access_subject: String,
    contact_hint: Option<String>,
}

impl VerifiedBootstrapContext {
    #[must_use]
    pub fn from_verified_identity(
        scope: TenantScope,
        actor_id: ActorId,
        correlation_id: CorrelationId,
        identity: &VerifiedExternalIdentity,
    ) -> Self {
        Self {
            scope,
            actor_id,
            correlation_id,
            access_subject: identity.subject().to_owned(),
            contact_hint: identity.contact_hint().map(str::to_owned),
        }
    }

    #[must_use]
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyReplay {
    result_code: String,
    result_reference: Option<String>,
}

impl IdempotencyReplay {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TenantBoundaryRow {
    pub membership_count: u64,
    pub active_owner_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipStatusValue {
    Active,
    Suspended,
    Revoked,
}

impl MembershipStatusValue {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Suspended => "SUSPENDED",
            Self::Revoked => "REVOKED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileGrantValue {
    Viewer,
    Operator,
}

impl ProfileGrantValue {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Viewer => "PROFILE_VIEWER",
            Self::Operator => "PROFILE_OPERATOR",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientGrantValue {
    Viewer,
    Editor,
}

impl ClientGrantValue {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Viewer => "CLIENT_VIEWER",
            Self::Editor => "CLIENT_EDITOR",
        }
    }
}

pub struct MutationEnvelope<'a> {
    pub idempotency_key: &'a IdempotencyKey,
    pub request_digest: &'a str,
    pub audit_event_id: &'a AuditEventId,
    pub outbox_event_id: &'a OutboxEventId,
    pub payload_json: &'a str,
    pub now: UnixMillis,
    pub idempotency_expires_at: UnixMillis,
}

pub struct BootstrapOwnerMutation<'a> {
    pub tenant_display_name: &'a str,
    pub identity_id: &'a IdentityId,
    pub envelope: MutationEnvelope<'a>,
}

pub struct OwnerTransferMutation<'a> {
    pub next_owner_actor_id: &'a ActorId,
    pub current_owner_version: AggregateVersion,
    pub next_owner_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct CreateInvitationMutation<'a> {
    pub invitation_id: &'a InvitationId,
    pub invited_contact_hmac: &'a str,
    pub expires_at: UnixMillis,
    pub tenant_expected_version: AggregateVersion,
    pub envelope: MutationEnvelope<'a>,
}

pub struct AcceptInvitationMutation<'a> {
    pub invitation_id: &'a InvitationId,
    pub identity_id: &'a IdentityId,
    pub invited_actor_id: &'a ActorId,
    pub access_subject: &'a str,
    pub contact_hint: Option<&'a str>,
    pub envelope: MutationEnvelope<'a>,
}

pub struct MembershipStatusMutation<'a> {
    pub target_actor_id: &'a ActorId,
    pub expected_version: AggregateVersion,
    pub next_status: MembershipStatusValue,
    pub envelope: MutationEnvelope<'a>,
}

pub struct CreateProfileMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub envelope: MutationEnvelope<'a>,
}

pub struct AssignProfileMutation<'a> {
    pub assignment_id: &'a profile_platform_primitives::AssignmentId,
    pub profile_id: &'a ProfileId,
    pub client_id: &'a ClientId,
    pub expected_profile_version: AggregateVersion,
    pub reason: &'a str,
    pub envelope: MutationEnvelope<'a>,
}

pub struct ProfileGrantMutation<'a> {
    pub target_actor_id: &'a ActorId,
    pub profile_id: &'a ProfileId,
    pub expected_profile_version: AggregateVersion,
    pub role: ProfileGrantValue,
    pub reason: &'a str,
    pub envelope: MutationEnvelope<'a>,
}

pub struct ClientGrantMutation<'a> {
    pub target_actor_id: &'a ActorId,
    pub client_id: &'a ClientId,
    pub expected_client_version: AggregateVersion,
    pub role: ClientGrantValue,
    pub reason: &'a str,
    pub envelope: MutationEnvelope<'a>,
}

pub struct D1IdentityAclRepository {
    database: D1Database,
}

impl D1IdentityAclRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn resolve_active_actor(
        &self,
        scope: TenantScope,
        identity: &VerifiedExternalIdentity,
        correlation_id: CorrelationId,
    ) -> Result<Option<ResolvedActor>> {
        let statement = query!(
            &self.database,
            r#"
            SELECT membership.actor_id, membership.role
            FROM identities AS identity
            JOIN memberships AS membership
              ON membership.identity_id = identity.identity_id
             AND membership.tenant_id = ?
             AND membership.status = 'ACTIVE'
            WHERE identity.access_subject = ?
            "#,
            scope.tenant_id().as_str(),
            identity.subject()
        )?;
        let row = statement.first::<MembershipRow>(None).await?;
        row.map(|row| {
            let actor_id = ActorId::parse(row.actor_id).map_err(invalid_adapter_identifier)?;
            let role = match row.role.as_str() {
                "TENANT_OWNER" => ResolvedMembershipRole::TenantOwner,
                "MEMBER" => ResolvedMembershipRole::Member,
                _ => return Err(Error::RustError("invalid membership role".to_owned())),
            };
            Ok(ResolvedActor {
                actor: ActorContext::new(scope, actor_id, correlation_id),
                role,
            })
        })
        .transpose()
    }

    pub async fn tenant_boundary(&self, scope: &TenantScope) -> Result<TenantBoundaryRow> {
        let statement = query!(
            &self.database,
            r#"
            SELECT
                COUNT(*) AS membership_count,
                COALESCE(SUM(CASE
                    WHEN role = 'TENANT_OWNER' AND status = 'ACTIVE' THEN 1
                    ELSE 0
                END), 0) AS active_owner_count
            FROM memberships
            WHERE tenant_id = ?
            "#,
            scope.tenant_id().as_str()
        )?;
        let row = statement
            .first::<BoundaryDatabaseRow>(None)
            .await?
            .ok_or_else(|| Error::RustError("tenant boundary query returned no row".to_owned()))?;
        Ok(TenantBoundaryRow {
            membership_count: non_negative_u64(row.membership_count)?,
            active_owner_count: non_negative_u64(row.active_owner_count)?,
        })
    }

    pub async fn idempotency_replay(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyReplay>> {
        query!(
            &self.database,
            IDEMPOTENCY_LOOKUP,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
            key.as_str()
        )?
        .first::<IdempotencyReplayRow>(None)
        .await
        .map(|row| {
            row.map(|row| IdempotencyReplay {
                result_code: row.result_code,
                result_reference: row.result_reference,
            })
        })
    }

    pub async fn bootstrap_owner(
        &self,
        context: &VerifiedBootstrapContext,
        mutation: BootstrapOwnerMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = context.scope.tenant_id().as_str();
        let actor_id = context.actor_id.as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let statements = vec![
            query!(
                &self.database,
                OWNER_BOOTSTRAP_TENANT,
                tenant_id,
                mutation.tenant_display_name,
                now,
                now
            )?,
            query!(
                &self.database,
                IDENTITY_CREATE,
                mutation.identity_id.as_str(),
                context.access_subject.as_str(),
                context.contact_hint.as_deref(),
                now
            )?,
            query!(
                &self.database,
                OWNER_BOOTSTRAP_MEMBERSHIP,
                tenant_id,
                actor_id,
                mutation.identity_id.as_str(),
                now,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "tenant.owner_bootstrap",
                "bootstrapped",
                tenant_id,
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                context.correlation_id.as_str(),
                actor_id,
                "tenant.owner_bootstrap",
                "tenant",
                tenant_id,
                "bootstrapped",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "tenant",
                tenant_id,
                1,
                "tenant.owner_bootstrapped.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn profile_metadata_exists(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<bool> {
        Ok(query!(
            &self.database,
            "SELECT profile_id FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<String>(Some("profile_id"))
        .await?
        .is_some())
    }

    pub async fn profile_grant_exists(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<bool> {
        Ok(query!(
            &self.database,
            r#"
            SELECT profile_id FROM profile_grants
            WHERE tenant_id = ? AND actor_id = ? AND profile_id = ?
            "#,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
            profile_id.as_str()
        )?
        .first::<String>(Some("profile_id"))
        .await?
        .is_some())
    }

    pub async fn client_grant_exists(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        client_id: &ClientId,
    ) -> Result<bool> {
        Ok(query!(
            &self.database,
            r#"
            SELECT client_id FROM client_grants
            WHERE tenant_id = ? AND actor_id = ? AND client_id = ?
            "#,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
            client_id.as_str()
        )?
        .first::<String>(Some("client_id"))
        .await?
        .is_some())
    }
}

#[derive(Deserialize)]
struct MembershipRow {
    actor_id: String,
    role: String,
}

#[derive(Deserialize)]
struct BoundaryDatabaseRow {
    membership_count: i64,
    active_owner_count: i64,
}

#[derive(Deserialize)]
struct IdempotencyReplayRow {
    result_code: String,
    result_reference: Option<String>,
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
    resource_type: &str,
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
        resource_type,
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
) -> Result<worker::D1PreparedStatement> {
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

fn invalid_adapter_identifier(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

fn non_negative_u64(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::RustError("negative D1 count".to_owned()))
}

fn sqlite_integer(value: UnixMillis) -> Result<i64> {
    sqlite_integer_value(value.value())
}

fn sqlite_integer_value(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}
