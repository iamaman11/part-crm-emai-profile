use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use core::future::Future;
use profile_domain::ProfileStatus;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, GenerationId, ProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileReadProjection {
    profile_id: ProfileId,
    status: ProfileStatus,
    version: AggregateVersion,
    linked_client_id: Option<ClientId>,
    active_generation_id: Option<GenerationId>,
}

impl ProfileReadProjection {
    #[must_use]
    pub const fn new(
        profile_id: ProfileId,
        status: ProfileStatus,
        version: AggregateVersion,
        linked_client_id: Option<ClientId>,
        active_generation_id: Option<GenerationId>,
    ) -> Self {
        Self {
            profile_id,
            status,
            version,
            linked_client_id,
            active_generation_id,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn status(&self) -> ProfileStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn linked_client_id(&self) -> Option<&ClientId> {
        self.linked_client_id.as_ref()
    }

    #[must_use]
    pub const fn active_generation_id(&self) -> Option<&GenerationId> {
        self.active_generation_id.as_ref()
    }
}

pub trait ProfileReadModelPort {
    fn list_profiles(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> impl Future<Output = Result<QueryPage<ProfileReadProjection>, QueryPortError>>;
}
