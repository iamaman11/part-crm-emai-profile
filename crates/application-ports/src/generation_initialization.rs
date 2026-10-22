//! First-generation publication over the existing generation/device persistence owners.

use crate::CommandExecutionEvidence;
use crate::generation_objects::GenerationObjectDescriptor;
use crate::generations::{GenerationPortError, GenerationReplayDecision};
use identity_access_domain::ProfileGrant;
use profile_domain::ProfileStatus;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, DeviceId, GenerationId, ProfileId, TenantId,
};

pub const INITIALIZE_GENERATION_COMMAND: &str = "profile_generation.initialize";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialGenerationContext {
    pub tenant_id: TenantId,
    pub profile_id: ProfileId,
    pub grant: Option<ProfileGrant>,
    pub status: ProfileStatus,
    pub version: AggregateVersion,
    pub active_generation_id: Option<GenerationId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialGenerationWrite {
    pub device_id: DeviceId,
    pub object: GenerationObjectDescriptor,
    pub expected_profile_version: AggregateVersion,
    pub evidence: CommandExecutionEvidence,
}

#[allow(async_fn_in_trait)]
pub trait InitialGenerationApplicationPort {
    async fn initialization_context(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
    ) -> Result<Option<InitialGenerationContext>, GenerationPortError>;

    async fn initialization_replay(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<GenerationReplayDecision, GenerationPortError>;

    /// Atomically register, verify, admit only the current device and activate with CAS,
    /// together with idempotency/audit/outbox. Recheck Profile Operate/device/DRAFT at the write boundary.
    async fn initialize_generation(
        &self,
        actor: &ActorContext,
        write: &InitialGenerationWrite,
    ) -> Result<(), GenerationPortError>;
}
