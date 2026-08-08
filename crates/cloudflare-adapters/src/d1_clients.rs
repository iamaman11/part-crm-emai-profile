use crate::d1_catalog::{CatalogClientKind, CreateClientMutation, D1CatalogRepository};
use crate::d1_governed_commands::D1GovernedCommandRepository;
use crate::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use crate::d1_identity_acl::{
    ClientGrantMutation, ClientGrantValue, MutationEnvelope, ResolvedMembershipRole,
};
use crate::d1_identity_queries::{ClientProjection, D1IdentityQueryRepository};
use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ClientApplicationPort, ClientCreateWrite, ClientGrantApplicationPort, ClientGrantPortError,
    ClientGrantPortErrorClass, ClientGrantRole, ClientGrantWrite, ClientPortError,
    ClientPortErrorClass, ClientReadModel, ClientReplayDecision, ClientReplayReceipt,
};
use client_domain::{ClientKind, ClientStatus};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ActorId, AggregateVersion, ClientId, TenantScope};
use worker::Error;
use worker::d1::D1Database;

pub struct D1ClientApplicationRepository {
    catalog: D1CatalogRepository,
    governed: D1GovernedCommandRepository,
    idempotency: D1IdempotencyRepository,
    queries: D1IdentityQueryRepository,
}

impl D1ClientApplicationRepository {
    #[must_use]
    pub const fn new(
        catalog_database: D1Database,
        governed_database: D1Database,
        idempotency_database: D1Database,
        query_database: D1Database,
    ) -> Self {
        Self {
            catalog: D1CatalogRepository::new(catalog_database),
            governed: D1GovernedCommandRepository::new(governed_database),
            idempotency: D1IdempotencyRepository::new(idempotency_database),
            queries: D1IdentityQueryRepository::new(query_database),
        }
    }
}

impl ClientApplicationPort for D1ClientApplicationRepository {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError> {
        self.idempotency
            .decide(
                actor.tenant_scope(),
                actor.actor_id(),
                evidence.idempotency_key(),
                command_name,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map(map_replay_decision)
            .map_err(map_dependency_error)
    }

    async fn create_client(
        &self,
        actor: &ActorContext,
        write: &ClientCreateWrite,
    ) -> Result<(), ClientPortError> {
        let evidence = write.evidence();
        let mutation = CreateClientMutation {
            client_id: write.client().client_id(),
            kind: catalog_kind(write.client().kind()),
            display_name: write.requested_display_name(),
            idempotency_key: evidence.idempotency_key(),
            request_digest: evidence.request_digest(),
            audit_event_id: evidence.audit_event_id(),
            outbox_event_id: evidence.outbox_event_id(),
            event_payload_json: write.event_payload_json(),
            now: evidence.now(),
            idempotency_expires_at: evidence.idempotency_expires_at(),
        };
        self.catalog
            .create_client(actor, mutation)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }

    async fn find_visible_client(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        client_id: &ClientId,
    ) -> Result<Option<ClientReadModel>, ClientPortError> {
        self.queries
            .find_visible_client(scope, actor_id, resolved_role(role), client_id)
            .await
            .map_err(map_dependency_error)?
            .map(client_read_model)
            .transpose()
    }
}

impl ClientGrantApplicationPort for D1ClientApplicationRepository {
    async fn decide_client_grant_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientGrantPortError> {
        self.idempotency
            .decide(
                actor.tenant_scope(),
                actor.actor_id(),
                evidence.idempotency_key(),
                command_name,
                evidence.request_digest(),
                evidence.now(),
            )
            .await
            .map(map_replay_decision)
            .map_err(map_client_grant_dependency_error)
    }

    async fn grant_client(
        &self,
        actor: &ActorContext,
        write: &ClientGrantWrite,
    ) -> Result<(), ClientGrantPortError> {
        self.write_client_grant(actor, write, false).await
    }

    async fn revoke_client_grant(
        &self,
        actor: &ActorContext,
        write: &ClientGrantWrite,
    ) -> Result<(), ClientGrantPortError> {
        self.write_client_grant(actor, write, true).await
    }
}

impl D1ClientApplicationRepository {
    async fn write_client_grant(
        &self,
        actor: &ActorContext,
        write: &ClientGrantWrite,
        revoke: bool,
    ) -> Result<(), ClientGrantPortError> {
        let mutation = ClientGrantMutation {
            target_actor_id: write.target_actor_id(),
            client_id: write.client_id(),
            expected_client_version: write.expected_client_version(),
            role: map_client_grant_role(write.role()),
            reason: write.reason(),
            envelope: mutation_envelope(write.evidence(), write.event_payload_json()),
        };
        let result = if revoke {
            self.governed.revoke_client_grant(actor, mutation).await
        } else {
            self.governed.grant_client(actor, mutation).await
        };
        result.map(|_| ()).map_err(map_client_grant_write_error)
    }
}

fn mutation_envelope<'a>(
    evidence: &'a CommandExecutionEvidence,
    payload_json: &'a str,
) -> MutationEnvelope<'a> {
    MutationEnvelope {
        idempotency_key: evidence.idempotency_key(),
        request_digest: evidence.request_digest(),
        audit_event_id: evidence.audit_event_id(),
        outbox_event_id: evidence.outbox_event_id(),
        payload_json,
        now: evidence.now(),
        idempotency_expires_at: evidence.idempotency_expires_at(),
    }
}

fn map_replay_decision(decision: IdempotencyDecision) -> ClientReplayDecision {
    match decision {
        IdempotencyDecision::Miss => ClientReplayDecision::Miss,
        IdempotencyDecision::Replay(receipt) => {
            ClientReplayDecision::Replay(ClientReplayReceipt::new(
                receipt.result_code(),
                receipt.result_reference().map(str::to_owned),
            ))
        }
        IdempotencyDecision::Conflict => ClientReplayDecision::Conflict,
    }
}

const fn catalog_kind(kind: ClientKind) -> CatalogClientKind {
    match kind {
        ClientKind::Person => CatalogClientKind::Person,
        ClientKind::Organization => CatalogClientKind::Organization,
    }
}

const fn resolved_role(role: MembershipRole) -> ResolvedMembershipRole {
    match role {
        MembershipRole::TenantOwner => ResolvedMembershipRole::TenantOwner,
        MembershipRole::Member => ResolvedMembershipRole::Member,
    }
}

const fn map_client_grant_role(role: ClientGrantRole) -> ClientGrantValue {
    match role {
        ClientGrantRole::Viewer => ClientGrantValue::Viewer,
        ClientGrantRole::Editor => ClientGrantValue::Editor,
    }
}

fn client_read_model(projection: ClientProjection) -> Result<ClientReadModel, ClientPortError> {
    let kind = match projection.kind() {
        "PERSON" => ClientKind::Person,
        "ORGANIZATION" => ClientKind::Organization,
        _ => return Err(integrity_failure()),
    };
    let status = match projection.status() {
        "ACTIVE" => ClientStatus::Active,
        "ARCHIVED" => ClientStatus::Archived,
        "MERGED" => ClientStatus::Merged,
        _ => return Err(integrity_failure()),
    };
    let version = AggregateVersion::new(projection.version()).map_err(|_| integrity_failure())?;
    Ok(ClientReadModel::new(
        projection.client_id().clone(),
        kind,
        projection.display_name(),
        status,
        version,
    ))
}

fn map_write_error(error: Error) -> ClientPortError {
    ClientPortError::new(classify_write_failure(&error.to_string()))
}

fn map_dependency_error(_error: Error) -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::IntegrityFailure)
}

fn classify_write_failure(message: &str) -> ClientPortErrorClass {
    if message.contains("UNIQUE constraint failed") {
        return ClientPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
    {
        return ClientPortErrorClass::IntegrityFailure;
    }
    if message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return ClientPortErrorClass::InternalFailure;
    }
    ClientPortErrorClass::DependencyUnavailable
}

fn map_client_grant_write_error(error: Error) -> ClientGrantPortError {
    ClientGrantPortError::new(classify_client_grant_write_failure(&error.to_string()))
}

fn map_client_grant_dependency_error(_error: Error) -> ClientGrantPortError {
    ClientGrantPortError::new(ClientGrantPortErrorClass::DependencyUnavailable)
}

fn classify_client_grant_write_failure(message: &str) -> ClientGrantPortErrorClass {
    if message.contains("owner_required")
        || message.contains("client_missing")
        || message.contains("target_missing")
        || message.contains("target_not_active_member")
    {
        return ClientGrantPortErrorClass::NotFound;
    }
    if message.contains("state_mismatch")
        || message.contains("version_mismatch")
        || message.contains("tenant_version_mismatch")
    {
        return ClientGrantPortErrorClass::VersionConflict;
    }
    if message.contains("time_regression")
        || message.contains("invalid_transition")
        || message.contains("grant_missing")
    {
        return ClientGrantPortErrorClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return ClientGrantPortErrorClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
    {
        return ClientGrantPortErrorClass::IntegrityFailure;
    }
    if message.contains("aggregate version overflow")
        || message.contains("value exceeds SQLite INTEGER")
        || message.contains("idempotency expiry overflow")
    {
        return ClientGrantPortErrorClass::InternalFailure;
    }
    ClientGrantPortErrorClass::DependencyUnavailable
}

#[cfg(test)]
mod tests {
    use super::{classify_client_grant_write_failure, classify_write_failure};
    use application_ports::clients::{ClientGrantPortErrorClass, ClientPortErrorClass};

    #[test]
    fn client_write_failures_keep_public_classes_stable() {
        assert_eq!(
            classify_write_failure(
                "UNIQUE constraint failed: clients.tenant_id, clients.client_id"
            ),
            ClientPortErrorClass::Conflict
        );
        assert_eq!(
            classify_write_failure("CHECK constraint failed: clients"),
            ClientPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_write_failure("value exceeds SQLite INTEGER"),
            ClientPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_write_failure("network request failed"),
            ClientPortErrorClass::DependencyUnavailable
        );
    }

    #[test]
    fn client_grant_write_failures_keep_legacy_public_classes() {
        assert_eq!(
            classify_client_grant_write_failure("client_grant_client_missing"),
            ClientGrantPortErrorClass::NotFound
        );
        assert_eq!(
            classify_client_grant_write_failure("client_grant_target_not_active_member"),
            ClientGrantPortErrorClass::NotFound
        );
        assert_eq!(
            classify_client_grant_write_failure("client_grant_client_version_mismatch"),
            ClientGrantPortErrorClass::VersionConflict
        );
        assert_eq!(
            classify_client_grant_write_failure("client_grant_missing"),
            ClientGrantPortErrorClass::InvalidState
        );
        assert_eq!(
            classify_client_grant_write_failure(
                "UNIQUE constraint failed: client_grant_commands.tenant_id"
            ),
            ClientGrantPortErrorClass::Conflict
        );
        assert_eq!(
            classify_client_grant_write_failure("client_grant_not_governed"),
            ClientGrantPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            classify_client_grant_write_failure("aggregate version overflow"),
            ClientGrantPortErrorClass::InternalFailure
        );
        assert_eq!(
            classify_client_grant_write_failure("network request failed"),
            ClientGrantPortErrorClass::DependencyUnavailable
        );
    }
}
