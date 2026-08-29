use application_ports::generation_objects::GenerationObjectDescriptor;
use application_ports::profile_generation_successor::{
    ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitError,
    ProfileGenerationSuccessorCommitErrorClass, ProfileGenerationSuccessorCommitRequest,
    ProfileGenerationWriterAuthorityRequest,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken, GenerationId,
    ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_GENERATION_SUCCESSOR_COMMIT_PATH: &str = "/profile-generation-successor";
pub const PROFILE_GENERATION_WRITER_AUTHORITY_PATH: &str = "/profile-generation-writer-authority";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGenerationSuccessorInternalRequest {
    tenant_id: String,
    actor_id: String,
    correlation_id: String,
    device_id: String,
    profile_id: String,
    base_generation_id: String,
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    expected_profile_version: u64,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
    coordinator_version: u64,
    coordinator_sequence: u64,
}

impl ProfileGenerationSuccessorInternalRequest {
    #[must_use]
    pub fn from_domain(
        actor: &ActorContext,
        request: &ProfileGenerationSuccessorCommitRequest,
    ) -> Self {
        let object = request.object();
        Self {
            tenant_id: actor.tenant_scope().tenant_id().as_str().to_owned(),
            actor_id: actor.actor_id().as_str().to_owned(),
            correlation_id: actor.correlation_id().as_str().to_owned(),
            device_id: request.device_id().as_str().to_owned(),
            profile_id: request.profile_id().as_str().to_owned(),
            base_generation_id: request.base_generation_id().as_str().to_owned(),
            generation_id: object.generation_id().as_str().to_owned(),
            object_key: object.object_key().to_owned(),
            metadata_digest: object.metadata_digest().to_owned(),
            container_digest: object.container_digest().to_owned(),
            container_bytes: object.container_bytes(),
            expected_profile_version: request.expected_profile_version().value(),
            coordinator_session_id: request.coordinator().session_id().as_str().to_owned(),
            coordinator_fencing_token: request.coordinator().fencing_token().as_str().to_owned(),
            coordinator_epoch: request.coordinator().epoch(),
            coordinator_version: request.coordinator().coordinator_version(),
            coordinator_sequence: request.coordinator().coordinator_sequence(),
        }
    }

    pub fn into_domain(
        self,
        observed_at: UnixMillis,
    ) -> Result<
        (ActorContext, ProfileGenerationSuccessorCommitRequest),
        ProfileGenerationSuccessorCommitError,
    > {
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id),
            ActorId::parse(self.actor_id).map_err(|_| integrity_failure())?,
            CorrelationId::parse(self.correlation_id).map_err(|_| integrity_failure())?,
        );
        let profile_id = ProfileId::parse(self.profile_id).map_err(|_| integrity_failure())?;
        let request = ProfileGenerationSuccessorCommitRequest::new(
            DeviceId::parse(self.device_id).map_err(|_| integrity_failure())?,
            profile_id.clone(),
            GenerationId::parse(self.base_generation_id).map_err(|_| integrity_failure())?,
            GenerationObjectDescriptor::new(
                profile_id,
                GenerationId::parse(self.generation_id).map_err(|_| integrity_failure())?,
                self.object_key,
                self.metadata_digest,
                self.container_digest,
                self.container_bytes,
            ),
            AggregateVersion::new(self.expected_profile_version)
                .map_err(|_| integrity_failure())?,
            ProfileGenerationCommitWitness::new(
                SessionId::parse(self.coordinator_session_id).map_err(|_| integrity_failure())?,
                FencingToken::parse(self.coordinator_fencing_token)
                    .map_err(|_| integrity_failure())?,
                self.coordinator_epoch,
                self.coordinator_version,
                self.coordinator_sequence,
            ),
            observed_at,
        );
        Ok((actor, request))
    }

    #[must_use]
    pub fn authority_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"profile-generation-successor-authority-v1\n");
        for value in [
            self.tenant_id.as_bytes(),
            self.actor_id.as_bytes(),
            self.device_id.as_bytes(),
            self.profile_id.as_bytes(),
            self.base_generation_id.as_bytes(),
            self.generation_id.as_bytes(),
            self.object_key.as_bytes(),
            self.metadata_digest.as_bytes(),
            self.container_digest.as_bytes(),
            self.coordinator_session_id.as_bytes(),
            self.coordinator_fencing_token.as_bytes(),
        ] {
            hash_field(&mut hasher, value);
        }
        for value in [
            self.container_bytes,
            self.expected_profile_version,
            self.coordinator_epoch,
            self.coordinator_version,
            self.coordinator_sequence,
        ] {
            hash_field(&mut hasher, &value.to_be_bytes());
        }
        hex_digest(hasher.finalize().into())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGenerationWriterAuthorityInternalRequest {
    tenant_id: String,
    actor_id: String,
    correlation_id: String,
    device_id: String,
    profile_id: String,
    coordinator_session_id: String,
    coordinator_fencing_token: String,
    coordinator_epoch: u64,
}

impl ProfileGenerationWriterAuthorityInternalRequest {
    #[must_use]
    pub fn from_domain(actor: &ActorContext, request: &ProfileGenerationWriterAuthorityRequest) -> Self {
        Self {
            tenant_id: actor.tenant_scope().tenant_id().as_str().to_owned(),
            actor_id: actor.actor_id().as_str().to_owned(),
            correlation_id: actor.correlation_id().as_str().to_owned(),
            device_id: request.device_id().as_str().to_owned(),
            profile_id: request.profile_id().as_str().to_owned(),
            coordinator_session_id: request.session_id().as_str().to_owned(),
            coordinator_fencing_token: request.fencing_token().as_str().to_owned(),
            coordinator_epoch: request.epoch(),
        }
    }

    pub fn into_domain(
        self,
    ) -> Result<
        (ActorContext, ProfileGenerationWriterAuthorityRequest),
        ProfileGenerationSuccessorCommitError,
    > {
        let tenant_id = TenantId::parse(self.tenant_id).map_err(|_| integrity_failure())?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id),
            ActorId::parse(self.actor_id).map_err(|_| integrity_failure())?,
            CorrelationId::parse(self.correlation_id).map_err(|_| integrity_failure())?,
        );
        let request = ProfileGenerationWriterAuthorityRequest::new(
            DeviceId::parse(self.device_id).map_err(|_| integrity_failure())?,
            ProfileId::parse(self.profile_id).map_err(|_| integrity_failure())?,
            SessionId::parse(self.coordinator_session_id).map_err(|_| integrity_failure())?,
            FencingToken::parse(self.coordinator_fencing_token).map_err(|_| integrity_failure())?,
            self.coordinator_epoch,
        );
        if request.epoch() == 0 {
            return Err(integrity_failure());
        }
        Ok((actor, request))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileGenerationSuccessorInternalOutcome {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileGenerationSuccessorInternalErrorClass {
    StaleAuthority,
    VersionConflict,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGenerationSuccessorInternalResponse {
    pub outcome: ProfileGenerationSuccessorInternalOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGenerationWriterAuthorityInternalResponse {
    pub coordinator_version: u64,
    pub coordinator_sequence: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGenerationSuccessorInternalErrorResponse {
    pub class: ProfileGenerationSuccessorInternalErrorClass,
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn hex_digest(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn integrity_failure() -> ProfileGenerationSuccessorCommitError {
    ProfileGenerationSuccessorCommitError::new(
        ProfileGenerationSuccessorCommitErrorClass::IntegrityFailure,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileGenerationSuccessorInternalRequest,
        ProfileGenerationWriterAuthorityInternalRequest,
    };
    use application_ports::generation_objects::GenerationObjectDescriptor;
    use application_ports::profile_generation_successor::{
        ProfileGenerationCommitWitness, ProfileGenerationSuccessorCommitRequest,
        ProfileGenerationWriterAuthorityRequest,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, CorrelationId, DeviceId, FencingToken,
        GenerationId, ProfileId, SessionId, TenantId, TenantScope, UnixMillis,
    };

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_successor_runtime_01")?),
            ActorId::parse("actor_successor_runtime_01")?,
            CorrelationId::parse("corr_successor_runtime_01")?,
        ))
    }

    #[test]
    fn internal_shape_round_trips_without_client_clock_authority()
    -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let tenant_id = actor.tenant_scope().tenant_id().clone();
        let profile_id = ProfileId::parse("profile_successor_runtime_01")?;
        let domain = ProfileGenerationSuccessorCommitRequest::new(
            DeviceId::parse("device_successor_runtime_01")?,
            profile_id.clone(),
            GenerationId::parse("generation_successor_runtime_base_01")?,
            GenerationObjectDescriptor::new(
                profile_id.clone(),
                GenerationId::parse("generation_successor_runtime_next_01")?,
                format!(
                    "tenants/{}/profiles/{}/generations/generation_successor_runtime_next_01.bpgc",
                    tenant_id.as_str(),
                    profile_id.as_str(),
                ),
                "a".repeat(64),
                "b".repeat(64),
                4096,
            ),
            AggregateVersion::new(7)?,
            ProfileGenerationCommitWitness::new(
                SessionId::parse("session_successor_runtime_01")?,
                FencingToken::parse("fence_successor_runtime_01")?,
                3,
                11,
                10,
            ),
            UnixMillis::new(100),
        );
        let internal = ProfileGenerationSuccessorInternalRequest::from_domain(&actor, &domain);
        let digest = internal.authority_digest();
        let serialized = serde_json::to_string(&internal)?;
        assert!(!serialized.contains("observed_at"));
        let parsed: ProfileGenerationSuccessorInternalRequest = serde_json::from_str(&serialized)?;
        assert_eq!(parsed.authority_digest(), digest);
        let (round_actor, round_domain) = parsed.into_domain(UnixMillis::new(200))?;
        assert_eq!(round_actor, actor);
        assert_eq!(round_domain.observed_at(), UnixMillis::new(200));
        assert_eq!(round_domain.object(), domain.object());
        assert_eq!(round_domain.coordinator(), domain.coordinator());
        Ok(())
    }

    #[test]
    fn writer_authority_shape_contains_raw_witness_but_no_client_clock_or_version()
    -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let domain = ProfileGenerationWriterAuthorityRequest::new(
            DeviceId::parse("device_successor_runtime_01")?,
            ProfileId::parse("profile_successor_runtime_01")?,
            SessionId::parse("session_successor_runtime_01")?,
            FencingToken::parse("fence_successor_runtime_01")?,
            3,
        );
        let internal = ProfileGenerationWriterAuthorityInternalRequest::from_domain(&actor, &domain);
        let serialized = serde_json::to_string(&internal)?;
        for forbidden in [
            "observed_at",
            "client_clock",
            "coordinator_version",
            "coordinator_sequence",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        let (round_actor, round_domain) = internal.into_domain()?;
        assert_eq!(round_actor, actor);
        assert_eq!(round_domain, domain);
        Ok(())
    }
}
