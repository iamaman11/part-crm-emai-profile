use crate::d1_identity_acl::{
    ClientGrantMutation, CreateInvitationMutation, CreateProfileMutation, MembershipStatusMutation,
    MutationEnvelope, OwnerTransferMutation, ProfileGrantMutation, AssignProfileMutation,
};
use profile_platform_primitives::{ActorContext, AggregateVersion};
use worker::d1::{D1Database, D1Result};
use worker::{Error, Result, query};

const OWNER_TRANSFER_COMMAND: &str = r#"
INSERT INTO owner_transfer_commands (
    tenant_id, command_id, current_owner_actor_id, next_owner_actor_id,
    current_owner_version, next_owner_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const INVITATION_CREATE_COMMAND: &str = r#"
INSERT INTO invitation_create_commands (
    tenant_id, command_id, command_actor_id, invitation_id,
    invited_contact_hmac, expires_at_ms, expected_tenant_version, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

const MEMBERSHIP_STATUS_COMMAND: &str = r#"
INSERT INTO membership_status_commands (
    tenant_id, command_id, command_actor_id, target_actor_id,
    expected_version, next_status, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

const PROFILE_CREATE_COMMAND: &str = r#"
INSERT INTO profile_create_commands (
    tenant_id, command_id, command_actor_id, profile_id, executed_at_ms
) VALUES (?, ?, ?, ?, ?)
"#;

const PROFILE_ASSIGNMENT_COMMAND: &str = r#"
INSERT INTO profile_assignment_commands (
    tenant_id, command_id, command_actor_id, assignment_id,
    profile_id, client_id, expected_profile_version, reason, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const PROFILE_GRANT_COMMAND: &str = r#"
INSERT INTO profile_grant_commands (
    tenant_id, command_id, command_actor_id, target_actor_id,
    profile_id, operation, role, expected_profile_version, reason, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

const CLIENT_GRANT_COMMAND: &str = r#"
INSERT INTO client_grant_commands (
    tenant_id, command_id, command_actor_id, target_actor_id,
    client_id, operation, role, expected_client_version, reason, executed_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

pub struct D1GovernedCommandRepository {
    database: D1Database,
}

impl D1GovernedCommandRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn transfer_owner(
        &self,
        actor: &ActorContext,
        mutation: OwnerTransferMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let current_version = sqlite_version(mutation.current_owner_version)?;
        let next_version = sqlite_version(mutation.next_owner_version)?;
        let aggregate_version = next_version_value(mutation.next_owner_version)?;
        let resource_id = mutation.next_owner_actor_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                OWNER_TRANSFER_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                resource_id,
                current_version,
                next_version,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "membership.owner_transfer",
                "transferred",
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
                "membership.owner_transfer",
                "membership",
                resource_id,
                "transferred",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "membership",
                resource_id,
                aggregate_version,
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
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let invitation_expires_at = sqlite_integer(mutation.expires_at.value())?;
        let expected_version = sqlite_version(mutation.tenant_expected_version)?;
        let aggregate_version = next_version_value(mutation.tenant_expected_version)?;
        let resource_id = mutation.invitation_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                INVITATION_CREATE_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                resource_id,
                mutation.invited_contact_hmac,
                invitation_expires_at,
                expected_version,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "invitation.create",
                "created",
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
                "invitation.create",
                "invitation",
                resource_id,
                "created",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "tenant",
                tenant_id,
                aggregate_version,
                "invitation.created.v1",
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
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_version)?;
        let aggregate_version = next_version_value(mutation.expected_version)?;
        let action = match mutation.next_status.database_value() {
            "ACTIVE" => "membership.activate",
            "SUSPENDED" => "membership.suspend",
            "REVOKED" => "membership.revoke",
            _ => return Err(Error::RustError("unsupported membership status".to_owned())),
        };
        let event_type = match mutation.next_status.database_value() {
            "ACTIVE" => "membership.activated.v1",
            "SUSPENDED" => "membership.suspended.v1",
            "REVOKED" => "membership.revoked.v1",
            _ => return Err(Error::RustError("unsupported membership status".to_owned())),
        };
        let resource_id = mutation.target_actor_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                MEMBERSHIP_STATUS_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                resource_id,
                expected_version,
                mutation.next_status.database_value(),
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                "updated",
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
                action,
                "membership",
                resource_id,
                "updated",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "membership",
                resource_id,
                aggregate_version,
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
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let resource_id = mutation.profile_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                PROFILE_CREATE_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                resource_id,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                "profile.create",
                "created",
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
                "profile.create",
                "profile",
                resource_id,
                "created",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                resource_id,
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
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_profile_version)?;
        let aggregate_version = next_version_value(mutation.expected_profile_version)?;
        let resource_id = mutation.profile_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                PROFILE_ASSIGNMENT_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.assignment_id.as_str(),
                resource_id,
                mutation.client_id.as_str(),
                expected_version,
                mutation.reason,
                now
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
                resource_id,
                "assigned",
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                resource_id,
                aggregate_version,
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
        self.profile_grant(actor, mutation, "GRANT").await
    }

    pub async fn revoke_profile_grant(
        &self,
        actor: &ActorContext,
        mutation: ProfileGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.profile_grant(actor, mutation, "REVOKE").await
    }

    async fn profile_grant(
        &self,
        actor: &ActorContext,
        mutation: ProfileGrantMutation<'_>,
        operation: &str,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_profile_version)?;
        let aggregate_version = next_version_value(mutation.expected_profile_version)?;
        let action = if operation == "GRANT" {
            "profile.grant"
        } else {
            "profile.grant_revoke"
        };
        let result_code = if operation == "GRANT" {
            "granted"
        } else {
            "revoked"
        };
        let event_type = if operation == "GRANT" {
            "profile.access_granted.v1"
        } else {
            "profile.access_revoked.v1"
        };
        let resource_id = mutation.profile_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                PROFILE_GRANT_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.target_actor_id.as_str(),
                resource_id,
                operation,
                mutation.role.database_value(),
                expected_version,
                mutation.reason,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                result_code,
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
                action,
                "profile",
                resource_id,
                result_code,
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "profile",
                resource_id,
                aggregate_version,
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
        self.client_grant(actor, mutation, "GRANT").await
    }

    pub async fn revoke_client_grant(
        &self,
        actor: &ActorContext,
        mutation: ClientGrantMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        self.client_grant(actor, mutation, "REVOKE").await
    }

    async fn client_grant(
        &self,
        actor: &ActorContext,
        mutation: ClientGrantMutation<'_>,
        operation: &str,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let now = sqlite_integer(mutation.envelope.now.value())?;
        let expires_at = sqlite_integer(mutation.envelope.idempotency_expires_at.value())?;
        let expected_version = sqlite_version(mutation.expected_client_version)?;
        let aggregate_version = next_version_value(mutation.expected_client_version)?;
        let action = if operation == "GRANT" {
            "client.grant"
        } else {
            "client.grant_revoke"
        };
        let result_code = if operation == "GRANT" {
            "granted"
        } else {
            "revoked"
        };
        let event_type = if operation == "GRANT" {
            "client.access_granted.v1"
        } else {
            "client.access_revoked.v1"
        };
        let resource_id = mutation.client_id.as_str();
        let statements = vec![
            query!(
                &self.database,
                CLIENT_GRANT_COMMAND,
                tenant_id,
                mutation.envelope.idempotency_key.as_str(),
                actor_id,
                mutation.target_actor_id.as_str(),
                resource_id,
                operation,
                mutation.role.database_value(),
                expected_version,
                mutation.reason,
                now
            )?,
            idempotency_statement(
                &self.database,
                tenant_id,
                actor_id,
                action,
                result_code,
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
                action,
                "client",
                resource_id,
                result_code,
                &mutation.envelope,
                now,
            )?,
            outbox_statement(
                &self.database,
                tenant_id,
                "client",
                resource_id,
                aggregate_version,
                event_type,
                &mutation.envelope,
                now,
            )?,
        ];
        self.database.batch(statements).await
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

fn sqlite_version(version: AggregateVersion) -> Result<i64> {
    sqlite_integer(version.value())
}

fn next_version_value(version: AggregateVersion) -> Result<i64> {
    let next = version
        .next()
        .map_err(|_| Error::RustError("aggregate version overflow".to_owned()))?;
    sqlite_integer(next.value())
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}
