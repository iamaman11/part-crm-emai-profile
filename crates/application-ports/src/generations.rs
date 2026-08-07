use crate::commands::CommandExecutionEvidence;
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, GenerationId, ProfileId, TenantScope,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationObjectReference {
    generation_id: GenerationId,
    ciphertext_digest: String,
}

impl GenerationObjectReference {
    #[must_use]
    pub fn new(generation_id: GenerationId, ciphertext_digest: impl Into<String>) -> Self {
        Self {
            generation_id,
            ciphertext_digest: ciphertext_digest.into(),
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn ciphertext_digest(&self) -> &str {
        &self.ciphertext_digest
    }
}

pub trait GenerationObjectStorePort {
    type Error;

    fn verify_generation_object(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectReference,
    ) -> Result<bool, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationStatus {
    Registered,
    Verified,
    Quarantined,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationReadModel {
    generation_id: GenerationId,
    metadata_digest: String,
    container_digest: String,
    status: GenerationStatus,
    version: AggregateVersion,
    verification_reference: Option<String>,
}

impl GenerationReadModel {
    #[must_use]
    pub fn new(
        generation_id: GenerationId,
        metadata_digest: impl Into<String>,
        container_digest: impl Into<String>,
        status: GenerationStatus,
        version: AggregateVersion,
        verification_reference: Option<String>,
    ) -> Self {
        Self {
            generation_id,
            metadata_digest: metadata_digest.into(),
            container_digest: container_digest.into(),
            status,
            version,
            verification_reference,
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn status(&self) -> GenerationStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub fn verification_reference(&self) -> Option<&str> {
        self.verification_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl GenerationReplayReceipt {
    #[must_use]
    pub fn new(result_code: impl Into<String>, result_reference: Option<String>) -> Self {
        Self {
            result_code: result_code.into(),
            result_reference,
        }
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationReplayDecision {
    Miss,
    Replay(GenerationReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationPortError {
    class: GenerationPortErrorClass,
}

impl GenerationPortError {
    #[must_use]
    pub const fn new(class: GenerationPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> GenerationPortErrorClass {
        self.class
    }
}

impl fmt::Display for GenerationPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            GenerationPortErrorClass::NotFound => "generation not found",
            GenerationPortErrorClass::VersionConflict => "generation version conflict",
            GenerationPortErrorClass::InvalidState => "generation invalid state",
            GenerationPortErrorClass::Conflict => "generation conflict",
            GenerationPortErrorClass::IntegrityFailure => "generation integrity failure",
            GenerationPortErrorClass::InternalFailure => "generation internal failure",
            GenerationPortErrorClass::DependencyUnavailable => "generation dependency unavailable",
        })
    }
}

impl std::error::Error for GenerationPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterGenerationWrite {
    profile_id: ProfileId,
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl RegisterGenerationWrite {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        generation_id: GenerationId,
        object_key: impl Into<String>,
        metadata_digest: impl Into<String>,
        container_digest: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            object_key: object_key.into(),
            metadata_digest: metadata_digest.into(),
            container_digest: container_digest.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyGenerationWrite {
    profile_id: ProfileId,
    generation_id: GenerationId,
    expected_generation_version: AggregateVersion,
    verification_reference: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl VerifyGenerationWrite {
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        generation_id: GenerationId,
        expected_generation_version: AggregateVersion,
        verification_reference: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            expected_generation_version,
            verification_reference: verification_reference.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn expected_generation_version(&self) -> AggregateVersion {
        self.expected_generation_version
    }

    #[must_use]
    pub fn verification_reference(&self) -> &str {
        &self.verification_reference
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationProfileVersionWrite {
    profile_id: ProfileId,
    generation_id: GenerationId,
    expected_profile_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl GenerationProfileVersionWrite {
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        generation_id: GenerationId,
        expected_profile_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            expected_profile_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn expected_profile_version(&self) -> AggregateVersion {
        self.expected_profile_version
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantineGenerationWrite {
    profile_id: ProfileId,
    generation_id: GenerationId,
    expected_generation_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl QuarantineGenerationWrite {
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        generation_id: GenerationId,
        expected_generation_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            profile_id,
            generation_id,
            expected_generation_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn expected_generation_version(&self) -> AggregateVersion {
        self.expected_generation_version
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[allow(async_fn_in_trait)]
pub trait GenerationApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<GenerationReplayDecision, GenerationPortError>;

    async fn register_generation(
        &self,
        actor: &ActorContext,
        write: &RegisterGenerationWrite,
    ) -> Result<(), GenerationPortError>;

    async fn verify_generation(
        &self,
        actor: &ActorContext,
        write: &VerifyGenerationWrite,
    ) -> Result<(), GenerationPortError>;

    async fn activate_generation(
        &self,
        actor: &ActorContext,
        write: &GenerationProfileVersionWrite,
    ) -> Result<(), GenerationPortError>;

    async fn deactivate_generation(
        &self,
        actor: &ActorContext,
        write: &GenerationProfileVersionWrite,
    ) -> Result<(), GenerationPortError>;

    async fn quarantine_generation(
        &self,
        actor: &ActorContext,
        write: &QuarantineGenerationWrite,
    ) -> Result<(), GenerationPortError>;

    async fn find_visible_generation(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
    ) -> Result<Option<GenerationReadModel>, GenerationPortError>;
}
