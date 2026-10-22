use super::*;
use application_ports::generation_initialization::{
    INITIALIZE_GENERATION_COMMAND, InitialGenerationApplicationPort, InitialGenerationContext,
    InitialGenerationWrite,
};

impl InitialGenerationApplicationPort for D1ProfileGenerationApplicationRepository {
    async fn initialization_context(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
    ) -> Result<Option<InitialGenerationContext>, GenerationPortError> {
        self.generations
            .initialization_context(actor, profile_id)
            .await
            .map_err(map_dependency_error)
    }

    async fn initialization_replay(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<GenerationReplayDecision, GenerationPortError> {
        self.decide_replay(actor, INITIALIZE_GENERATION_COMMAND, evidence)
            .await
    }

    async fn initialize_generation(
        &self,
        actor: &ActorContext,
        write: &InitialGenerationWrite,
    ) -> Result<(), GenerationPortError> {
        let envelope = mutation_envelope(&write.evidence, "{}");
        self.generations
            .initialize_generation(actor, write, &envelope)
            .await
            .map(|_| ())
            .map_err(map_write_error)
    }
}
