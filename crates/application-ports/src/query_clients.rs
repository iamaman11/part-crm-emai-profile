use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use client_domain::{ClientKind, ClientStatus};
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientReadProjection {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
    status: ClientStatus,
    version: AggregateVersion,
}

impl ClientReadProjection {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
        status: ClientStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            client_id,
            kind,
            display_name: display_name.into(),
            status,
            version,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId { &self.client_id }
    #[must_use]
    pub const fn kind(&self) -> ClientKind { self.kind }
    #[must_use]
    pub fn display_name(&self) -> &str { &self.display_name }
    #[must_use]
    pub const fn status(&self) -> ClientStatus { self.status }
    #[must_use]
    pub const fn version(&self) -> AggregateVersion { self.version }
}

pub trait ClientReadModelPort {
    async fn list_clients(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<ClientReadProjection>, QueryPortError>;
}
