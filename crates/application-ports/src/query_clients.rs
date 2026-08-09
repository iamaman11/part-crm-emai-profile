use crate::query::{QueryPage, QueryPageRequest, QueryPageSize, QueryPortError};
use client_domain::{
    ClientKind, ClientStatus, ContactKind, ContactNormalizationVersion, ExactLookupToken,
};
use core::future::Future;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, ContactPointId};

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
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn kind(&self) -> ClientKind {
        self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn status(&self) -> ClientStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientContactExactMatchProjection {
    client_id: ClientId,
    contact_point_id: ContactPointId,
}

impl ClientContactExactMatchProjection {
    #[must_use]
    pub const fn new(client_id: ClientId, contact_point_id: ContactPointId) -> Self {
        Self {
            client_id,
            contact_point_id,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        &self.contact_point_id
    }
}

pub trait ClientReadModelPort {
    fn list_clients(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> impl Future<Output = Result<QueryPage<ClientReadProjection>, QueryPortError>>;
}

pub trait ClientExactContactQueryPort {
    fn find_visible_clients_by_exact_contact(
        &self,
        actor: &ActorContext,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        token: &ExactLookupToken,
        limit: QueryPageSize,
    ) -> impl Future<Output = Result<Vec<ClientContactExactMatchProjection>, QueryPortError>>;
}
