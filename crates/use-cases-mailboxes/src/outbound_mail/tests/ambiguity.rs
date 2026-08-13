use super::super::execute_outbound_mail;
use super::support::{
    FakeAccess, FakeProvider, FakeStore, TestResult, actor, block_on, evidence, intent,
    provider_failure,
};
use application_ports::outbound_mail::OutboundMailIntentState;

#[test]
fn provider_uncertainty_becomes_ambiguous_without_blind_resend() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-ambiguous", "digest_ambiguous_001", "ambiguous")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([Err(provider_failure())]);

        let first =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await?;
        let replay =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await?;
        assert_eq!(first.state(), OutboundMailIntentState::Ambiguous);
        assert_eq!(replay.state(), OutboundMailIntentState::Ambiguous);
        assert!(replay.replayed());
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}
