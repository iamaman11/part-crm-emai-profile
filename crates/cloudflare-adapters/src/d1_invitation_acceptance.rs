use crate::access_identity::VerifiedExternalIdentity;
use crate::d1_identity_acl::{MutationEnvelope, VerifiedBootstrapContext};
use profile_platform_primitives::{CorrelationId, IdentityId, InvitationId};
use worker::d1::{D1Database, D1Result};
use worker::{Result, query};

const IDENTITY_CREATE: &str = r#"
INSERT INTO identities (
    identity_id, access_subject, verified_contact_hint, created_at_ms
) VALUES (?, ?, ?, ?)
"#;

const MEMBERSHIP_CREATE: &str = r#"
INSERT INTO memberships (
    tenant_id, actor_id, identity_id, role, status, version,
    created_at_ms, updated_at_ms
) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?)
"#;

const ACCEPTANCE_CREATE: &str = r#"
INSERT INTO invitation_acceptances (
    tenant_id, invitation_id, identity_id, actor_id, accepted_at_ms
) VALUES (?, ?, ?, ?, ?)
"#;

const IDEMPOTENCY_CREATE: &str = r#"
INSERT INTO idempotency_records (
    tenant_id, actor_id, idempotency_key, command_name, request_digest,
    result_code, result_reference, created_at_ms, expires_at_ms
) VALUES (?, ?, ?, 'invitation.accept', ?, 'accepted', ?, ?, ?)
"#;

const AUDIT_CREATE: &str = r#"
INSERT INTO audit_events (
    tenant_id, audit_event_id, correlation_id, actor_id, action,
    resource_type, resource_id, result_code, occurred_at_ms
) VALUES (?, ?, ?, ?, 'invitation.accept', 'invitation', ?, 'accepted', ?)
"#;

const OUTBOX_CREATE: &str = r#"
INSERT INTO outbox_events (
    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
    aggregate_version, event_type, payload_json, created_at_ms
) VALUES (?, ?, 'membership', ?, 1, 'membership.activated.v1', ?, ?)
"#;

pub struct AcceptInvitationMutation<'a> {
    pub invitation_id: &'a InvitationId,
    pub identity_id: &'a IdentityId,
    pub envelope: MutationEnvelope<'a>,
}

pub struct D1InvitationAcceptanceRepository {
    database: D1Database,
}

impl D1InvitationAcceptanceRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn accept(
        &self,
        context: &VerifiedBootstrapContext,
        identity: &VerifiedExternalIdentity,
        correlation_id: &CorrelationId,
        mutation: AcceptInvitationMutation<'_>,
    ) -> Result<Vec<D1Result>> {
        let tenant_id = context.scope().tenant_id().as_str();
        let actor_id = context.actor_id().as_str();
        let now = i64::try_from(mutation.envelope.now.value())?;
        let expires_at = i64::try_from(mutation.envelope.idempotency_expires_at.value())?;
        let statements = vec![
            query!(
                &self.database,
                IDENTITY_CREATE,
                mutation.identity_id.as_str(),
                identity.subject(),
                identity.contact_hint(),
                now
            )?,
            query!(
                &self.database,
                MEMBERSHIP_CREATE,
                tenant_id,
                actor_id,
                mutation.identity_id.as_str(),
                now,
                now
            )?,
            query!(
                &self.database,
                ACCEPTANCE_CREATE,
                tenant_id,
                mutation.invitation_id.as_str(),
                mutation.identity_id.as_str(),
                actor_id,
                now
            )?,
            query!(
                &self.database,
                IDEMPOTENCY_CREATE,
                tenant_id,
                actor_id,
                mutation.envelope.idempotency_key.as_str(),
                mutation.envelope.request_digest,
                actor_id,
                now,
                expires_at
            )?,
            query!(
                &self.database,
                AUDIT_CREATE,
                tenant_id,
                mutation.envelope.audit_event_id.as_str(),
                correlation_id.as_str(),
                actor_id,
                mutation.invitation_id.as_str(),
                now
            )?,
            query!(
                &self.database,
                OUTBOX_CREATE,
                tenant_id,
                mutation.envelope.outbox_event_id.as_str(),
                actor_id,
                mutation.envelope.payload_json,
                now
            )?,
        ];
        self.database.batch(statements).await
    }
}
