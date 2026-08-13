use super::super::{OutboundMailOperationError, execute_outbound_mail};
use super::support::{
    FakeAccess, FakeProvider, FakeStore, TestResult, actor, block_on, evidence, intent,
    provider_failure,
};
use application_ports::CommandExecutionEvidence;
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailIntent, OutboundMailIntentApplicationPort,
    OutboundMailIntentPortError, OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt,
    OutboundMailProviderOutcome, OutboundMailReserveDecision,
};
use profile_platform_primitives::ActorContext;

struct FailingReconciliationStore(FakeStore);

impl FailingReconciliationStore {
    const fn new() -> Self {
        Self(FakeStore::new())
    }
}

impl OutboundMailIntentApplicationPort for FailingReconciliationStore {
    async fn reserve_intent(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        self.0.reserve_intent(actor, intent, evidence).await
    }

    async fn claim_dispatch(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        max_attempts: u8,
    ) -> Result<OutboundMailClaimDecision, OutboundMailIntentPortError> {
        self.0.claim_dispatch(actor, evidence, max_attempts).await
    }

    async fn complete_dispatch(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
        _outcome: &OutboundMailProviderOutcome,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        Err(reconciliation_unavailable())
    }

    async fn mark_ambiguous(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        Err(reconciliation_unavailable())
    }
}

#[test]
fn persistence_failure_is_fail_closed_without_resend() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence(
            "send-key-reconcile-failure",
            "digest_reconcile_failure_01",
            "reconcile-failure",
        )?;
        let access = FakeAccess::new(true);
        let store = FailingReconciliationStore::new();
        let provider = FakeProvider::new([Err(provider_failure())]);

        for _ in 0..2 {
            let result =
                execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await;
            assert_eq!(
                result,
                Err(OutboundMailOperationError::DependencyUnavailable)
            );
        }
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}

const fn reconciliation_unavailable() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::DependencyUnavailable)
}
