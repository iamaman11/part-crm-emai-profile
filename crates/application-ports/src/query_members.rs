use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use identity_access_domain::{MembershipRole, MembershipStatus};
use profile_platform_primitives::{ActorContext, ActorId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberReadProjection {
    actor_id: ActorId,
    role: MembershipRole,
    status: MembershipStatus,
}

impl MemberReadProjection {
    #[must_use]
    pub const fn new(actor_id: ActorId, role: MembershipRole, status: MembershipStatus) -> Self {
        Self { actor_id, role, status }
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId { &self.actor_id }
    #[must_use]
    pub const fn role(&self) -> MembershipRole { self.role }
    #[must_use]
    pub const fn status(&self) -> MembershipStatus { self.status }
}

pub trait MemberReadModelPort {
    async fn list_members(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<MemberReadProjection>, QueryPortError>;
}
