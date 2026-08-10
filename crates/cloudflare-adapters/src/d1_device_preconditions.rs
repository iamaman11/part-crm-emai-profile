use application_ports::device_generation_commit::{
    DeviceGenerationCommitError, DeviceGenerationCommitErrorClass,
    DeviceGenerationProfileVersionPort,
};
use application_ports::{
    DeviceExecutionBlocker, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
    DeviceJobPortError, DeviceJobPortErrorClass,
};
use device_domain::DeviceJobTarget;
use profile_platform_primitives::{ActorContext, AggregateVersion, GenerationId, ProfileId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

const EVALUATE_EXECUTION_PRECONDITIONS: &str = r#"
SELECT
    EXISTS(
        SELECT 1
        FROM browser_profiles AS profile
        WHERE profile.tenant_id = ?
          AND profile.profile_id = ?
          AND profile.status = 'READY'
          AND profile.active_generation_id = ?
    ) AS generation_active,
    EXISTS(
        SELECT 1
        FROM profile_generations AS generation
        WHERE generation.tenant_id = ?
          AND generation.profile_id = ?
          AND generation.generation_id = ?
          AND generation.status = 'VERIFIED'
          AND generation.verification_reference IS NOT NULL
    ) AS generation_verified,
    EXISTS(
        SELECT 1
        FROM device_authorizations AS authorization
        WHERE authorization.tenant_id = ?
          AND authorization.device_id = ?
          AND authorization.profile_id = ?
          AND authorization.generation_id = ?
          AND authorization.status = 'ACTIVE'
          AND authorization.version >= 1
    ) AS device_authorized
"#;

const LOAD_ACTIVE_PROFILE_VERSION: &str = r#"
SELECT profile.version
FROM browser_profiles AS profile
WHERE profile.tenant_id = ?
  AND profile.profile_id = ?
  AND profile.status = 'READY'
  AND profile.active_generation_id = ?
"#;

#[derive(Deserialize)]
struct ExecutionPreconditionRow {
    generation_active: i64,
    generation_verified: i64,
    device_authorized: i64,
}

#[derive(Deserialize)]
struct ActiveProfileVersionRow {
    version: i64,
}

pub struct D1DeviceExecutionPreconditions {
    database: D1Database,
}

impl D1DeviceExecutionPreconditions {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl DeviceExecutionPreconditionPort for D1DeviceExecutionPreconditions {
    async fn evaluate_device_execution(
        &self,
        actor: &ActorContext,
        target: &DeviceJobTarget,
    ) -> Result<DeviceExecutionReadiness, DeviceJobPortError> {
        if actor.tenant_scope().tenant_id() != target.tenant_id() {
            return Ok(DeviceExecutionReadiness::Blocked(
                DeviceExecutionBlocker::DeviceUnauthorized,
            ));
        }

        let row = query!(
            &self.database,
            EVALUATE_EXECUTION_PRECONDITIONS,
            target.tenant_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str(),
            target.tenant_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str(),
            target.tenant_id().as_str(),
            target.device_id().as_str(),
            target.profile_id().as_str(),
            target.generation_id().as_str()
        )
        .map_err(map_worker_error)?
        .first::<ExecutionPreconditionRow>(None)
        .await
        .map_err(map_worker_error)?
        .ok_or_else(integrity_failure)?;

        let generation_active = bounded_boolean(row.generation_active)?;
        let generation_verified = bounded_boolean(row.generation_verified)?;
        let device_authorized = bounded_boolean(row.device_authorized)?;

        if !device_authorized {
            return Ok(DeviceExecutionReadiness::Blocked(
                DeviceExecutionBlocker::DeviceUnauthorized,
            ));
        }
        if !generation_active {
            return Ok(DeviceExecutionReadiness::Blocked(
                DeviceExecutionBlocker::GenerationInactive,
            ));
        }
        if !generation_verified {
            return Ok(DeviceExecutionReadiness::Blocked(
                DeviceExecutionBlocker::CertificationIncomplete,
            ));
        }
        Ok(DeviceExecutionReadiness::Ready)
    }
}

impl DeviceGenerationProfileVersionPort for D1DeviceExecutionPreconditions {
    async fn load_active_profile_version(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
    ) -> Result<Option<AggregateVersion>, DeviceGenerationCommitError> {
        let row = query!(
            &self.database,
            LOAD_ACTIVE_PROFILE_VERSION,
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            base_generation_id.as_str()
        )
        .map_err(|_| generation_commit_dependency_failure())?
        .first::<ActiveProfileVersionRow>(None)
        .await
        .map_err(|_| generation_commit_dependency_failure())?;

        row.map(|row| {
            let version =
                u64::try_from(row.version).map_err(|_| generation_commit_integrity_failure())?;
            AggregateVersion::new(version).map_err(|_| generation_commit_integrity_failure())
        })
        .transpose()
    }
}

fn bounded_boolean(value: i64) -> Result<bool, DeviceJobPortError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(integrity_failure()),
    }
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

fn generation_commit_integrity_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

fn generation_commit_dependency_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{EVALUATE_EXECUTION_PRECONDITIONS, LOAD_ACTIVE_PROFILE_VERSION};

    #[test]
    fn precondition_query_is_exact_target_and_freshness_scoped() {
        for required in [
            "profile.status = 'READY'",
            "profile.active_generation_id = ?",
            "generation.status = 'VERIFIED'",
            "generation.verification_reference IS NOT NULL",
            "authorization.status = 'ACTIVE'",
            "authorization.device_id = ?",
            "authorization.generation_id = ?",
        ] {
            assert!(EVALUATE_EXECUTION_PRECONDITIONS.contains(required));
        }
        assert!(!EVALUATE_EXECUTION_PRECONDITIONS.contains("profile_assignments"));
    }

    #[test]
    fn active_profile_version_query_is_exact_base_generation_scoped() {
        for required in [
            "profile.tenant_id = ?",
            "profile.profile_id = ?",
            "profile.status = 'READY'",
            "profile.active_generation_id = ?",
        ] {
            assert!(LOAD_ACTIVE_PROFILE_VERSION.contains(required));
        }
        assert!(!LOAD_ACTIVE_PROFILE_VERSION.contains("profile_assignments"));
    }
}
