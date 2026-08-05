use crate::access_identity::VerifiedExternalIdentity;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, CorrelationId, IdentityId,
    IdempotencyKey, InvitationId, OutboxEventId, ProfileId, TenantScope, UnixMillis,
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

const OWNER_TRANSFER_DEMOTE: &str = r#"
UPDATE memberships
SET role = 'MEMBER', version = version + 1, updated_at_ms = ?
WHERE tenant_id = ? AND actor_id = ? AND role = 'TENANT_OWNER'
  AND status = 'ACTIVE' AND version = ?
"#;

const OWNER_TRANSFER_PROMOTE: &str = r#"
UPDATE memberships
SET role = 'TENANT_OWNER', version = version + 1, updated_at_ms = ?
WHERE tenant_id = ? AND actor_id = ? AND role = 'MEMBER'
  AND status = 'ACTIVE' AND version = ?
"#;

const INVITATION_CREATE: &str = r#"
INSERT INTO invitations (
    tenant_id, invitation_id, invited_contact_hmac, intended_role,
    status, expires_at_ms, created_by_actor_id, created_at_ms
) VALUES (?, ?, ?, 'MEMBER', 'PENDING', ?, ?, ?)
"#;

const INVITATION_ACCEPT_MEMBERSHIP: &str = r#"
INSERT INTO memberships (
    tenant_id, actor_id, identity_id, role, status, version,
    created_at_ms, updated_at_ms
) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?)
"#;

const INVITATION_ACCEPT_RECORD: &str = r#"
INSERT INTO invitation_acceptances (
    tenant_id, invitation_id, identity_id, actor_id, accepted_at_ms
) VALUES (?, ?, ?, ?, ?)
"#;

const MEMBERSHIP_STATUS_UPDATE: &str = r#"
UPDATE memberships
SET status = ?, version = version + 1, updated_at_ms = ?
WHERE tenant_id = ? AND actor_id = ? AND version = ?
"#;

const PROFILE_CREATE: &str = r#"
INSERT INTO browser_profiles (
    tenant_id, profile_id, status, active_generation_id, version,
    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, ?, ?)
"#;

const PROFILE_ASSIGNMENT_CLOSE: &str = r#"
UPDATE profile_client_assignments
SET closed_at_ms = ?
WHERE tenant_id = ? AND profile_id = ? AND closed_at_ms IS NULL
"#;

const PROFILE_ASSIGNMENT_CREATE: &str = r#"
INSERT INTO profile_client_assignments (
    tenant_id, assignment_id, profile_id, client_id,
    assigned_by_actor_id, assigned_at_ms, reason
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const PROFILE_VERSION_CAS: &str = r#"
UPDATE browser_profiles
SET version = version + 1, updated_by_actor_id = ?, updated_at_ms = ?
WHERE tenant_id = ? AND profile_id = ? AND version = ?
"#;

const PROFILE_GRANT_UPSERT: &str = r#"
INSERT INTO profile_grants (
    tenant_id, actor_id, profile_id, role, granted_by_actor_id, reason, created_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, actor_id, profile_id) DO UPDATE SET
    role = excluded.role,
    granted_by_actor_id = excluded.granted_by_actor_id,
    reason = excluded.reason,
    created_at_ms = excluded.created_at_ms
"#;

const PROFILE_GRANT_REVOKE: &str = r#"
DELETE FROM profile_grants
WHERE tenant_id = ? AND actor_id = ? AND profile_id = ?
"#;

const CLIENT_GRANT_UPSERT: &str = r#"
INSERT INTO client_grants (
    tenant_id, actor_id, client_id, role, granted_by_actor_id, reason, created_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (tenant_id, actor_id, client_id) DO UPDATE SET
    role = excluded.role,
    granted_by_actor_id = excluded.granted_by_actor_id,
    reason = excluded.reason,
    created_at_ms = excluded.created_at_ms
"#;

const CLIENT_GRANT_REVOKE: &str = r#"
DELETE FROM client_grants
WHERE tenant_id = ? AND actor_id = ? AND client_id = ?
"#;

const CLIENT_VERSION_CAS: &str = r#"
UPDATE clients
SET version = version + 1, updated_by_actor_id = ?, updated_at_ms = ?
WHERE tenant_id = ? AND client_id = ? AND version = ?
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

    pub async fn transfer_owner(
        &self,
        actor: &ActorContext,
        mutation: OwnerTransferMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let current_version = sqlite_integer_value(mutation.current_owner_version.value())?;
        let next_version = sqlite_integer_value(mutation.next_owner_version.value())?;
        let next_aggregate_version = sqlite_integer_value(
            mutation
                .next_owner_version
                .next()
                .map_err(|_| Error::RustError("membership version overflow".to_owned()))?
                .value(),
        )?;
        let statements = vec![
            query!(
                &self.database,
                OWNER_TRANSFER_DEMOTE,
                now,
                tenant_id,
                actor_id,
                current_version
            )?,
            query!(
                &self.database,
                OWNER_TRANSFER_PROMOTE,
                now,
                tenant_id,
                mutation.next_owner_actor_id.as_str(),
                next_version
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "membership.owner_transfer",
                "transferred",
                mutation.next_owner_actor_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "membership.owner_transfer",
                "membership",
                mutation.next_owner_actor_id.as_str(),
                "transferred",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "membership",
                mutation.next_owner_actor_id.as_str(),
                next_aggregate_version,
                "membership.owner_transferred.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn create_invitation(
        &self,
        actor: &ActorContext,
        mutation: CreateInvitationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let invitation_expires_at = sqlite_integer(mutation.expires_at)?;
        let expected_tenant_version = sqlite_integer_value(mutation.tenant_expected_version.value())?;
        let next_tenant_version = sqlite_integer_value(
            mutation
                .tenant_expected_version
                .next()
                .map_err(|_| Error::RustError("tenant version overflow".to_owned()))?
                .value(),
        )?;
        let statements = vec![
            query!(
                &self.database,
                r#"
                UPDATE tenants
                SET version = version + 1, updated_at_ms = ?
                WHERE tenant_id = ? AND version = ?
                "#,
                now,
                tenant_id,
                expected_tenant_version
            )?,
            query!(
                &self.database,
                INVITATION_CREATE,
                tenant_id,
                mutation.invitation_id.as_str(),
                mutation.invited_contact_hmac,
                invitation_expires_at,
                actor_id,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "invitation.create",
                "created",
                mutation.invitation_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "invitation.create",
                "invitation",
                mutation.invitation_id.as_str(),
                "created",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "tenant",
                tenant_id,
                next_tenant_version,
                "invitation.created.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn accept_invitation(
        &self,
        actor: &ActorContext,
        mutation: AcceptInvitationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let statements = vec![
            query!(
                &self.database,
                IDENTITY_CREATE,
                mutation.identity_id.as_str(),
                mutation.access_subject,
                mutation.contact_hint,
                now
            )?,
            query!(
                &self.database,
                INVITATION_ACCEPT_MEMBERSHIP,
                tenant_id,
                mutation.invited_actor_id.as_str(),
                mutation.identity_id.as_str(),
                now,
                now
            )?,
            query!(
                &self.database,
                INVITATION_ACCEPT_RECORD,
                tenant_id,
                mutation.invitation_id.as_str(),
                mutation.identity_id.as_str(),
                mutation.invited_actor_id.as_str(),
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "invitation.accept",
                "accepted",
                mutation.invited_actor_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "invitation.accept",
                "invitation",
                mutation.invitation_id.as_str(),
                "accepted",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "membership",
                mutation.invited_actor_id.as_str(),
                1,
                "membership.activated.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn update_membership_status(
        &self,
        actor: &ActorContext,
        mutation: MembershipStatusMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let expected_version = sqlite_integer_value(mutation.expected_version.value())?;
        let next_version = sqlite_integer_value(
            mutation
                .expected_version
                .next()
                .map_err(|_| Error::RustError("membership version overflow".to_owned()))?
                .value(),
        )?;
        let action = match mutation.next_status {
            MembershipStatusValue::Active => "membership.activate",
            MembershipStatusValue::Suspended => "membership.suspend",
            MembershipStatusValue::Revoked => "membership.revoke",
        };
        let event_type = match mutation.next_status {
            MembershipStatusValue::Active => "membership.activated.v1",
            MembershipStatusValue::Suspended => "membership.suspended.v1",
            MembershipStatusValue::Revoked => "membership.revoked.v1",
        };
        let statements = vec![
            query!(
                &self.database,
                MEMBERSHIP_STATUS_UPDATE,
                mutation.next_status.database_value(),
                now,
                tenant_id,
                mutation.target_actor_id.as_str(),
                expected_version
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                "updated",
                mutation.target_actor_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                action,
                "membership",
                mutation.target_actor_id.as_str(),
                "updated",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "membership",
                mutation.target_actor_id.as_str(),
                next_version,
                event_type,
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn create_profile(
        &self,
        actor: &ActorContext,
        mutation: CreateProfileMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let statements = vec![
            query!(
                &self.database,
                PROFILE_CREATE,
                tenant_id,
                mutation.profile_id.as_str(),
                actor_id,
                actor_id,
                now,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile.create",
                "created",
                mutation.profile_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile.create",
                "profile",
                mutation.profile_id.as_str(),
                "created",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                mutation.profile_id.as_str(),
                1,
                "profile.created.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn assign_profile(
        &self,
        actor: &ActorContext,
        mutation: AssignProfileMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let expected_version = sqlite_integer_value(mutation.expected_profile_version.value())?;
        let next_version = sqlite_integer_value(
            mutation
                .expected_profile_version
                .next()
                .map_err(|_| Error::RustError("profile version overflow".to_owned()))?
                .value(),
        )?;
        let statements = vec![
            query!(
                &self.database,
                PROFILE_ASSIGNMENT_CLOSE,
                now,
                tenant_id,
                mutation.profile_id.as_str()
            )?,
            query!(
                &self.database,
                PROFILE_ASSIGNMENT_CREATE,
                tenant_id,
                mutation.assignment_id.as_str(),
                mutation.profile_id.as_str(),
                mutation.client_id.as_str(),
                actor_id,
                now,
                mutation.reason
            )?,
            query!(
                &self.database,
                PROFILE_VERSION_CAS,
                actor_id,
                now,
                tenant_id,
                mutation.profile_id.as_str(),
                expected_version
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile.assign_client",
                "assigned",
                mutation.assignment_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                "profile.assign_client",
                "profile",
                mutation.profile_id.as_str(),
                "assigned",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                mutation.profile_id.as_str(),
                next_version,
                "profile.client_assigned.v1",
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn grant_profile(
        &self,
        actor: &ActorContext,
        mutation: ProfileGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.profile_grant_batch(actor, mutation, false).await
    }

    pub async fn revoke_profile_grant(
        &self,
        actor: &ActorContext,
        mutation: ProfileGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.profile_grant_batch(actor, mutation, true).await
    }

    async fn profile_grant_batch(
        &self,
        actor: &ActorContext,
        mutation: ProfileGrantMutation<'_>,
        revoke: bool,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let expected_version = sqlite_integer_value(mutation.expected_profile_version.value())?;
        let next_version = sqlite_integer_value(
            mutation
                .expected_profile_version
                .next()
                .map_err(|_| Error::RustError("profile version overflow".to_owned()))?
                .value(),
        )?;
        let grant_statement = if revoke {
            query!(
                &self.database,
                PROFILE_GRANT_REVOKE,
                tenant_id,
                mutation.target_actor_id.as_str(),
                mutation.profile_id.as_str()
            )?
        } else {
            query!(
                &self.database,
                PROFILE_GRANT_UPSERT,
                tenant_id,
                mutation.target_actor_id.as_str(),
                mutation.profile_id.as_str(),
                mutation.role.database_value(),
                actor_id,
                mutation.reason,
                now
            )?
        };
        let action = if revoke {
            "profile.grant_revoke"
        } else {
            "profile.grant"
        };
        let result_code = if revoke { "revoked" } else { "granted" };
        let event_type = if revoke {
            "profile.access_revoked.v1"
        } else {
            "profile.access_granted.v1"
        };
        let statements = vec![
            grant_statement,
            query!(
                &self.database,
                PROFILE_VERSION_CAS,
                actor_id,
                now,
                tenant_id,
                mutation.profile_id.as_str(),
                expected_version
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                result_code,
                mutation.profile_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                action,
                "profile",
                mutation.profile_id.as_str(),
                result_code,
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                mutation.profile_id.as_str(),
                next_version,
                event_type,
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
    }

    pub async fn grant_client(
        &self,
        actor: &ActorContext,
        mutation: ClientGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.client_grant_batch(actor, mutation, false).await
    }

    pub async fn revoke_client_grant(
        &self,
        actor: &ActorContext,
        mutation: ClientGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.client_grant_batch(actor, mutation, true).await
    }

    async fn client_grant_batch(
        &self,
        actor: &ActorContext,
        mutation: ClientGrantMutation<'_>,
        revoke: bool,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now)?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at)?;
        let expected_version = sqlite_integer_value(mutation.expected_client_version.value())?;
        let next_version = sqlite_integer_value(
            mutation
                .expected_client_version
                .next()
                .map_err(|_| Error::RustError("client version overflow".to_owned()))?
                .value(),
        )?;
        let grant_statement = if revoke {
            query!(
                &self.database,
                CLIENT_GRANT_REVOKE,
                tenant_id,
                mutation.target_actor_id.as_str(),
                mutation.client_id.as_str()
            )?
        } else {
            query!(
                &self.database,
                CLIENT_GRANT_UPSERT,
                tenant_id,
                mutation.target_actor_id.as_str(),
                mutation.client_id.as_str(),
                mutation.role.database_value(),
                actor_id,
                mutation.reason,
                now
            )?
        };
        let action = if revoke {
            "client.grant_revoke"
        } else {
            "client.grant"
        };
        let result_code = if revoke { "revoked" } else { "granted" };
        let event_type = if revoke {
            "client.access_revoked.v1"
        } else {
            "client.access_granted.v1"
        };
        let statements = vec![
            grant_statement,
            query!(
                &self.database,
                CLIENT_VERSION_CAS,
                actor_id,
                now,
                tenant_id,
                mutation.client_id.as_str(),
                expected_version
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                result_code,
                mutation.client_id.as_str(),
                &mutation.envelope,
                now,
                expires_at,
            )?,
            audit_statement(
                &self.database,
                tenant_id,
                actor.correlation_id().as_str(),
                actor_id,
                action,
                "client",
                mutation.client_id.as_str(),
                result_code,
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "client",
                mutation.client_id.as_str(),
                next_version,
                event_type,
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
