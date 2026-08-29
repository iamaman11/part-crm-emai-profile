use application_ports::profile_launch::{
    ProfileLaunchContext, ProfileLaunchContextPort, ProfileLaunchPortError,
    ProfileLaunchPortErrorClass,
};
use identity_access_domain::{ProfileGrant, ProfileGrantRole};
use profile_domain::{BrowserProfile, GenerationVerification, ProfileGeneration};
use profile_platform_primitives::{ActorId, GenerationId, ProfileId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const LOAD_PROFILE_LAUNCH_CONTEXT: &str = r#"
SELECT
    profile.status,
    profile.active_generation_id,
    grant.role AS grant_role
FROM browser_profiles AS profile
LEFT JOIN profile_grants AS grant
  ON grant.tenant_id = profile.tenant_id
 AND grant.profile_id = profile.profile_id
 AND grant.actor_id = ?
WHERE profile.tenant_id = ?
  AND profile.profile_id = ?
"#;

#[derive(Deserialize)]
struct ProfileLaunchContextRow {
    status: String,
    active_generation_id: Option<String>,
    grant_role: Option<String>,
}

pub struct D1ProfileLaunchContext {
    database: D1Database,
}

impl D1ProfileLaunchContext {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl ProfileLaunchContextPort for D1ProfileLaunchContext {
    async fn load_profile_launch_context(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileLaunchContext>, ProfileLaunchPortError> {
        let row = query!(
            &self.database,
            LOAD_PROFILE_LAUNCH_CONTEXT,
            actor_id.as_str(),
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )
        .map_err(map_worker_error)?
        .first::<ProfileLaunchContextRow>(None)
        .await
        .map_err(map_worker_error)?;

        row.map(|row| map_context(scope, actor_id, profile_id, row))
            .transpose()
    }
}

fn map_context(
    scope: &TenantScope,
    actor_id: &ActorId,
    profile_id: &ProfileId,
    row: ProfileLaunchContextRow,
) -> Result<ProfileLaunchContext, ProfileLaunchPortError> {
    let mut profile = BrowserProfile::create(scope.tenant_id().clone(), profile_id.clone());
    match row.status.as_str() {
        "READY" => {
            let generation_id = row
                .active_generation_id
                .ok_or_else(integrity_failure)
                .and_then(|value| GenerationId::parse(value).map_err(|_| integrity_failure()))?;
            let generation = ProfileGeneration::new(
                scope.tenant_id().clone(),
                profile_id.clone(),
                generation_id,
                GenerationVerification::Verified,
            );
            profile
                .activate_generation(&generation)
                .map_err(|_| integrity_failure())?;
        }
        "DRAFT" | "QUARANTINED" | "IN_USE" | "DIRTY_LOCAL" | "SYNCING" | "SUSPENDED"
        | "DELETING" | "DELETED" => {
            // `decide_open_profile` distinguishes launchable Ready+active-generation from every
            // other lifecycle state. Non-Ready rows are therefore projected to a non-Ready
            // BrowserProfile without creating a second lifecycle decision in this adapter.
        }
        _ => return Err(integrity_failure()),
    }

    let grant = match row.grant_role.as_deref() {
        None => None,
        Some("PROFILE_VIEWER") => Some(ProfileGrant::new(
            scope.tenant_id().clone(),
            actor_id.clone(),
            profile_id.clone(),
            ProfileGrantRole::Viewer,
        )),
        Some("PROFILE_OPERATOR") => Some(ProfileGrant::new(
            scope.tenant_id().clone(),
            actor_id.clone(),
            profile_id.clone(),
            ProfileGrantRole::Operator,
        )),
        Some(_) => return Err(integrity_failure()),
    };

    Ok(ProfileLaunchContext::new(profile, grant))
}

fn integrity_failure() -> ProfileLaunchPortError {
    ProfileLaunchPortError::new(ProfileLaunchPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> ProfileLaunchPortError {
    ProfileLaunchPortError::new(ProfileLaunchPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::LOAD_PROFILE_LAUNCH_CONTEXT;

    #[test]
    fn launch_context_query_loads_evidence_without_deciding_authorization() {
        for required in [
            "profile.tenant_id = ?",
            "profile.profile_id = ?",
            "profile.active_generation_id",
            "grant.actor_id = ?",
            "grant.role AS grant_role",
        ] {
            assert!(LOAD_PROFILE_LAUNCH_CONTEXT.contains(required));
        }
        assert!(!LOAD_PROFILE_LAUNCH_CONTEXT.contains("PROFILE_OPERATOR'"));
        assert!(!LOAD_PROFILE_LAUNCH_CONTEXT.contains("device_authorizations"));
        assert!(!LOAD_PROFILE_LAUNCH_CONTEXT.contains("profile_client_assignments"));
    }
}
