use super::super::{OutboundMailOperationError, execute_outbound_mail};
use super::support::{FakeAccess, FakeProvider, FakeStore, TestResult, actor, block_on, evidence, intent};
use application_ports::outbound_mail::{
    OutboundMailIntentState, OutboundMailProviderOutcome, OutboundMailProviderPortError,
    ProviderMessageReference,
};

#[test]
fn unauthorized_request_stops_before_reservation_and_provider() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence(
            "send-key-unauthorized",
            "digest_unauthorized_01",
            "unauthorized",
        )?;
        let access = FakeAccess::new(false);
        let store = FakeStore::new();
        let provider = FakeProvider::new(std::iter::empty::<
            Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>,
        >());

        let result =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await;
        assert_eq!(result, Err(OutboundMailOperationError::NotFound));
        assert_eq!(access.calls(), 1);
        assert_eq!(store.reserve_calls(), 0);
        assert_eq!(provider.calls(), 0);
        Ok(())
    })
}

#[test]
fn successful_replay_never_calls_provider_twice() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-replay", "digest_replay_0000001", "replay")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([Ok(OutboundMailProviderOutcome::Sent {
            provider_message_reference: Some(ProviderMessageReference::parse("provider-msg-1")?),
        })]);

        let first =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await?;
        let replay =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &evidence).await?;
        assert_eq!(first.state(), OutboundMailIntentState::Sent);
        assert_eq!(replay.state(), OutboundMailIntentState::Sent);
        assert!(replay.replayed());
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}

#[test]
fn same_key_with_different_digest_conflicts_without_resend() -> TestResult {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let first_evidence = evidence("send-key-conflict", "digest_conflict_0001", "conflict-a")?;
        let conflicting = evidence("send-key-conflict", "digest_conflict_0002", "conflict-b")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([Ok(OutboundMailProviderOutcome::Sent {
            provider_message_reference: None,
        })]);

        execute_outbound_mail(&actor, &access, &store, &provider, &intent, &first_evidence).await?;
        let result =
            execute_outbound_mail(&actor, &access, &store, &provider, &intent, &conflicting).await;
        assert_eq!(result, Err(OutboundMailOperationError::Conflict));
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}
