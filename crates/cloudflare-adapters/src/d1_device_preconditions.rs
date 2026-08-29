use application_ports::device_generation_commit::{
    DeviceGenerationCommitError, DeviceGenerationCommitErrorClass,
    DeviceGenerationProfileVersionPort,
};
use application_ports::generation_objects::{
    ActiveGenerationObjectReferencePort, GenerationObjectCatalogReference,
};
use application_ports::generations::{GenerationPortError, GenerationPortErrorClass};
use application_ports::profile_generation_successor::{
    ProfileGenerationSuccessorCommitError, ProfileGenerationSuccessorCommitErrorClass,
    ProfileGenerationSuccessorVersionPort,
};
use application_ports::{
    DeviceExecutionBlocker, DeviceExecutionPreconditionPort, DeviceExecutionReadiness,
    DeviceJobPortError, DeviceJobPortErrorClass,
};
use device_domain::DeviceJobTarget;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, GenerationId, ProfileId, TenantScope,
};
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

const LOAD_SUCCESSOR_PROFILE_VERSION: &str = r#"
SELECT
    profile.version,
    profile.active_generation_id
FROM browser_profiles AS profile
WHERE profile.tenant_id = ?
  AND profile.profile_id = ?
  AND profile.status = 'READY'
"#;

const LOAD_ACTIVE_GENERATION_OBJECT_REFERENCE: &str = r#"
SELECT
    generation.generation_id,
    generation.object_key,
    generation.metadata_digest,
    generation.container_digest
FROM browser_profiles AS profile
JOIN profile_generations AS generation
  ON generation.tenant_id = profile.tenant_id
 AND generation.profile_id = profile.profile_id
 AND generation.generation_id = profile.active_generation_id
WHERE profile.tenant_id = ?
  AND profile.profile_id = ?
  AND profile.status = 'READY'
  AND generation.status = 'VERIFIED'
  AND generation.verification_reference IS NOT NULL
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

#[derive(Deserialize)]
struct SuccessorProfileVersionRow {
    version: i64,
    active_generation_id: String,
}

#[derive(Deserialize)]
struct ActiveGenerationObjectRow {
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
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

impl ActiveGenerationObjectReferencePort for D1DeviceExecutionPreconditions {
    async fn load_active_verified_generation_object(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<GenerationObjectCatalogReference>, GenerationPortError> {
        let row = query!(
            &self.database,
            LOAD_ACTIVE_GENERATION_OBJECT_REFERENCE,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )
        .map_err(|_| generation_dependency_failure())?
        .first::<ActiveGenerationObjectRow>(None)
        .await
        .map_err(|_| generation_dependency_failure())?;

        row.map(|row| {
            let generation_id =
                GenerationId::parse(row.generation_id).map_err(|_| generation_integrity_failure())?;
            let canonical_key = format!(
                "tenants/{}/profiles/{}/generations/{}.bpgc",
                scope.tenant_id().as_str(),
                profile_id.as_str(),
                generation_id.as_str()
            );
            if row.object_key != canonical_key
                || !is_lower_sha256(&row.metadata_digest)
                || !is_lower_sha256(&row.container_digest)
            {
                return Err(generation_integrity_failure());
            }
            Ok(GenerationObjectCatalogReference::new(
                profile_id.clone(),
                generation_id,
                row.object_key,
                row.metadata_digest,
                row.container_digest,
            ))
        })
        .transpose()
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

impl ProfileGenerationSuccessorVersionPort for D1DeviceExecutionPreconditions {
    async fn load_successor_expected_profile_version(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        candidate_generation_id: &GenerationId,
    ) -> Result<Option<AggregateVersion>, ProfileGenerationSuccessorCommitError> {
        let row = query!(
            &self.database,
            LOAD_SUCCESSOR_PROFILE_VERSION,
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str()
        )
        .map_err(|_| profile_successor_dependency_failure())?
        .first::<SuccessorProfileVersionRow>(None)
        .await
        .map_err(|_| profile_successor_dependency_failure())?;

        let Some(row) = row else {
            return Ok(None);
        };
        let current =
            u64::try_from(row.version).map_err(|_| profile_successor_integrity_failure())?;
        let expected = if row.active_generation_id == base_generation_id.as_str() {
            current
        } else if row.active_generation_id == candidate_generation_id.as_str() {
            current
                .checked_sub(1)
                .ok_or_else(profile_successor_integrity_failure)?
        } else {
            return Ok(None);
        };
        AggregateVersion::new(expected)
            .map(Some)
            .map_err(|_| profile_successor_integrity_failure())
    }
}

fn bounded_boolean(value: i64) -> Result<bool, DeviceJobPortError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(integrity_failure()),
    }
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn integrity_failure() -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::IntegrityFailure)
}

fn map_worker_error(_error: worker::Error) -> DeviceJobPortError {
    DeviceJobPortError::new(DeviceJobPortErrorClass::DependencyUnavailable)
}

fn generation_integrity_failure() -> GenerationPortError {
    GenerationPortError::new(GenerationPortErrorClass::IntegrityFailure)
}

fn generation_dependency_failure() -> GenerationPortError {
    GenerationPortError::new(GenerationPortErrorClass::DependencyUnavailable)
}

fn generation_commit_integrity_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

fn generation_commit_dependency_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::DependencyUnavailable)
}

const fn profile_successor_integrity_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure,
    )
}

const fn profile_successor_dependency_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::DependencyUnavailable,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        EVALUATE_EXECUTION_PRECONDITIONS, LOAD_ACTIVE_GENERATION_OBJECT_REFERENCE,
        LOAD_ACTIVE_PROFILE_VERSION, LOAD_SUCCESSOR_PROFILE_VERSION,
    };

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
    fn active_generation_object_query_is_server_selected_verified_and_profile_scoped() {
        for required in [
            "generation.generation_id = profile.active_generation_id",
            "profile.status = 'READY'",
            "generation.status = 'VERIFIED'",
            "generation.verification_reference IS NOT NULL",
            "generation.object_key",
            "generation.metadata_digest",
            "generation.container_digest",
        ] {
            assert!(LOAD_ACTIVE_GENERATION_OBJECT_REFERENCE.contains(required));
        }
        assert!(!LOAD_ACTIVE_GENERATION_OBJECT_REFERENCE.contains("device_jobs"));
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

    #[test]
    fn successor_version_query_reads_only_server_owned_profile_state() {
        for required in [
            "profile.version",
            "profile.active_generation_id",
            "profile.tenant_id = ?",
            "profile.profile_id = ?",
            "profile.status = 'READY'",
        ] {
            assert!(LOAD_SUCCESSOR_PROFILE_VERSION.contains(required));
        }
        assert!(!LOAD_SUCCESSOR_PROFILE_VERSION.contains("device_jobs"));
        assert!(!LOAD_SUCCESSOR_PROFILE_VERSION.contains("profile_assignments"));
    }
}
