use crate::error::ApplicationError;
use client_domain::{ClientError, ClientKind, ClientRecord};
use contracts::ProblemCode;
use profile_platform_primitives::{ActorContext, ClientId, TenantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateClientCommand {
    tenant_id: TenantId,
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
}

impl CreateClientCommand {
    #[must_use]
    pub fn new(
        tenant_id: TenantId,
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            client_id,
            kind,
            display_name: display_name.into(),
        }
    }
}

pub fn decide_create_client(
    actor: &ActorContext,
    command: CreateClientCommand,
) -> Result<ClientRecord, ApplicationError> {
    if actor.tenant_scope().tenant_id() != &command.tenant_id {
        return Err(ApplicationError::new(ProblemCode::Forbidden));
    }

    ClientRecord::create(
        command.tenant_id,
        command.client_id,
        command.kind,
        command.display_name,
    )
    .map_err(map_client_error)
}

fn map_client_error(error: ClientError) -> ApplicationError {
    match error {
        ClientError::InvalidDisplayName => ApplicationError::new(ProblemCode::InvalidRequest),
        ClientError::InvalidStatusTransition => ApplicationError::new(ProblemCode::InvalidState),
        ClientError::VersionOverflow => ApplicationError::new(ProblemCode::InternalFailure),
    }
}
