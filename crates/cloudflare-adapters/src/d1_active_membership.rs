use application_ports::identity::{
    ActiveMembershipPort, ActiveMembershipPortError, ActiveMembershipPortErrorClass,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_ACTIVE_MEMBERSHIP: &str = r#"
SELECT role
FROM memberships
WHERE tenant_id = ?
  AND actor_id = ?
  AND status = 'ACTIVE'
"#;

#[derive(Deserialize)]
struct ActiveMembershipRow {
    role: String,
}

pub struct D1ActiveMembership {
    database: D1Database,
}

impl D1ActiveMembership {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl ActiveMembershipPort for D1ActiveMembership {
    async fn active_membership_role(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<MembershipRole>, ActiveMembershipPortError> {
        let row = query!(
            &self.database,
            LOAD_ACTIVE_MEMBERSHIP,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
        )
        .map_err(map_worker_error)?
        .first::<ActiveMembershipRow>(None)
        .await
        .map_err(map_worker_error)?;

        row.map(|row| match row.role.as_str() {
            "TENANT_OWNER" => Ok(MembershipRole::TenantOwner),
            "MEMBER" => Ok(MembershipRole::Member),
            _ => Err(integrity_failure()),
        })
        .transpose()
    }
}

fn integrity_failure() -> ActiveMembershipPortError {
    ActiveMembershipPortError::new(ActiveMembershipPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> ActiveMembershipPortError {
    ActiveMembershipPortError::new(ActiveMembershipPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::LOAD_ACTIVE_MEMBERSHIP;

    #[test]
    fn membership_revalidation_is_identity_owned_and_active_only() {
        for required in ["FROM memberships", "tenant_id = ?", "actor_id = ?", "status = 'ACTIVE'"] {
            assert!(LOAD_ACTIVE_MEMBERSHIP.contains(required));
        }
        assert!(!LOAD_ACTIVE_MEMBERSHIP.contains("profile_launch_claims"));
        assert!(!LOAD_ACTIVE_MEMBERSHIP.contains("profile_grants"));
    }
}
