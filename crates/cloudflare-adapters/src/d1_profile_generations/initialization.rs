use super::*;
use application_ports::generation_initialization::{
    InitialGenerationContext, InitialGenerationWrite,
};
use identity_access_domain::{ProfileGrant, ProfileGrantRole};
use profile_domain::ProfileStatus;

const LOAD_INITIAL_CONTEXT: &str = r#"
SELECT profile.status, profile.version, profile.active_generation_id, grant.role AS grant_role
FROM browser_profiles AS profile
LEFT JOIN profile_grants AS grant ON grant.tenant_id = profile.tenant_id
 AND grant.profile_id = profile.profile_id AND grant.actor_id = ?3
WHERE profile.tenant_id = ?1 AND profile.profile_id = ?2
"#;

const INITIAL_REGISTER_COMMAND: &str = r#"
INSERT INTO profile_generation_register_commands (
    tenant_id, command_id, command_actor_id, profile_id, generation_id,
    object_key, metadata_digest, container_digest, executed_at_ms,
    initial_device_id, initial_expected_profile_version
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

// A missing/stale owner/device/DRAFT observation becomes a NOT NULL failure inside the
// same transaction, rolling back register/verify/admission/activation and their evidence.
// No pre-existing admission, including a revoked one, can be overwritten.
const INITIAL_DEVICE_ADMISSION: &str = r#"
INSERT INTO device_authorizations (
    tenant_id, device_id, profile_id, generation_id, version, status,
    evidence_reference, authorized_by_actor_id, updated_by_actor_id,
    authorized_at_ms, updated_at_ms, revoked_at_ms
) VALUES (
    ?1,
    (SELECT binding.device_id FROM device_actor_bindings AS binding
     JOIN memberships AS member
       ON member.tenant_id = binding.tenant_id AND member.actor_id = binding.actor_id
     JOIN browser_profiles AS profile ON profile.tenant_id = binding.tenant_id
     WHERE binding.tenant_id = ?1 AND binding.actor_id = ?2 AND binding.device_id = ?3
       AND binding.status = 'ACTIVE' AND member.status = 'ACTIVE'
       AND (member.role = 'TENANT_OWNER' OR (member.role = 'MEMBER' AND EXISTS (
           SELECT 1 FROM profile_grants AS grant WHERE grant.tenant_id = profile.tenant_id
            AND grant.profile_id = profile.profile_id AND grant.actor_id = member.actor_id
            AND grant.role = 'PROFILE_OPERATOR')))
       AND profile.profile_id = ?4 AND profile.status = 'DRAFT'
       AND profile.active_generation_id IS NULL AND profile.version = ?8),
    ?4, ?5, 1, 'ACTIVE', ?6, ?2, ?2, ?7, ?7, NULL
)
"#;

#[derive(Deserialize)]
struct InitialContextRow {
    status: String,
    version: i64,
    active_generation_id: Option<String>,
    grant_role: Option<String>,
}

impl D1ProfileGenerationRepository {
    pub async fn initialization_context(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
    ) -> Result<Option<InitialGenerationContext>> {
        let row = query!(
            &self.database,
            LOAD_INITIAL_CONTEXT,
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            actor.actor_id().as_str()
        )?
        .first::<InitialContextRow>(None)
        .await?;
        row.map(|row| {
            let status = match row.status.as_str() {
                "DRAFT" => ProfileStatus::Draft,
                "READY" => ProfileStatus::Ready,
                "IN_USE" => ProfileStatus::InUse,
                "DIRTY_LOCAL" => ProfileStatus::DirtyLocal,
                "SYNCING" => ProfileStatus::Syncing,
                "QUARANTINED" => ProfileStatus::Quarantined,
                "SUSPENDED" => ProfileStatus::Suspended,
                "DELETING" => ProfileStatus::Deleting,
                "DELETED" => ProfileStatus::Deleted,
                _ => {
                    return Err(Error::RustError(
                        "initial generation profile state integrity failure".into(),
                    ));
                }
            };
            let grant = match row.grant_role.as_deref() {
                None => None,
                Some("PROFILE_OPERATOR") => Some(ProfileGrant::new(
                    actor.tenant_scope().tenant_id().clone(),
                    actor.actor_id().clone(),
                    profile_id.clone(),
                    ProfileGrantRole::Operator,
                )),
                Some("PROFILE_VIEWER") => Some(ProfileGrant::new(
                    actor.tenant_scope().tenant_id().clone(),
                    actor.actor_id().clone(),
                    profile_id.clone(),
                    ProfileGrantRole::Viewer,
                )),
                _ => {
                    return Err(Error::RustError(
                        "initial generation grant integrity failure".into(),
                    ));
                }
            };
            Ok(InitialGenerationContext {
                tenant_id: actor.tenant_scope().tenant_id().clone(),
                profile_id: profile_id.clone(),
                grant,
                status,
                version: AggregateVersion::new(positive_version(row.version)?).map_err(|_| {
                    Error::RustError("initial generation profile version integrity failure".into())
                })?,
                active_generation_id: row
                    .active_generation_id
                    .map(GenerationId::parse)
                    .transpose()
                    .map_err(identifier_error)?,
            })
        })
        .transpose()
    }

    pub async fn initialize_generation(
        &self,
        actor: &ActorContext,
        write: &InitialGenerationWrite,
        envelope: &MutationEnvelope<'_>,
    ) -> Result<Vec<D1Result>> {
        let object = &write.object;
        validate_object_key(object.object_key())?;
        validate_digest(object.metadata_digest())?;
        validate_digest(object.container_digest())?;
        let tenant = actor.tenant_scope().tenant_id().as_str();
        let actor_id = actor.actor_id().as_str();
        let profile = object.profile_id().as_str();
        let generation = object.generation_id().as_str();
        let command_id = command_journal_id(
            actor.tenant_scope().tenant_id(),
            actor.actor_id(),
            envelope.idempotency_key,
        )?;
        let now = sqlite_integer(envelope.now.value())?;
        let expected = sqlite_version(write.expected_profile_version)?;
        let verification = format!("r2sha256:{}", object.container_digest());
        let commands = vec![
            query!(
                &self.database,
                INITIAL_REGISTER_COMMAND,
                tenant,
                command_id.as_str(),
                actor_id,
                profile,
                generation,
                object.object_key(),
                object.metadata_digest(),
                object.container_digest(),
                now,
                write.device_id.as_str(),
                expected
            )?,
            query!(
                &self.database,
                VERIFY_COMMAND,
                tenant,
                command_id.as_str(),
                actor_id,
                profile,
                generation,
                1,
                verification.as_str(),
                now
            )?,
            query!(
                &self.database,
                INITIAL_DEVICE_ADMISSION,
                tenant,
                actor_id,
                write.device_id.as_str(),
                profile,
                generation,
                verification.as_str(),
                now,
                expected
            )?,
            query!(
                &self.database,
                ACTIVATE_COMMAND,
                tenant,
                command_id.as_str(),
                actor_id,
                profile,
                generation,
                expected,
                now
            )?,
        ];
        self.execute_commands(
            actor,
            envelope,
            commands,
            CommandEvidence {
                command_name:
                    application_ports::generation_initialization::INITIALIZE_GENERATION_COMMAND,
                result_code: "initialized",
                resource_id: generation,
                aggregate_type: "profile",
                aggregate_id: profile,
                aggregate_version: next_version_value(write.expected_profile_version)?,
                event_type: "profile.initialized.v1",
            },
        )
        .await
    }
}
