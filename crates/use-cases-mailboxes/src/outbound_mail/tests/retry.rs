use super::super::execute_outbound_mail;
use super::support::{
    FakeAccess, FakeProvider, FakeStore, TestResult, actor, block_on, evidence, intent,
};
use application_ports::outbound_mail::{OutboundMailIntentState, OutboundMailProviderOutcome};

#[test]
fn confirmed_not_sent_retry_is_bounded_to_three_provider_calls() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-retry", "digest_retry_0000001", "retry")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([
            Ok(OutboundMailProviderOutcome::RetryableNotSent),
            Ok(OutboundMailProviderOutcome::RetryableNotSent),
            Ok(OutboundMailProviderOutcome::RetryableNotSent),
        ]);

        for expected_attempt in 1..=3 {
            let outcome =
                execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence)
                    .await?;
            assert_eq!(outcome.state(), OutboundMailIntentState::Retryable);
            assert_eq!(outcome.attempt_count(), expected_attempt);
        }
        let exhausted =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await?;
        assert_eq!(exhausted.state(), OutboundMailIntentState::Rejected);
        assert_eq!(exhausted.attempt_count(), 3);
        assert_eq!(provider.calls(), 3);
        Ok(())
    })
}
