use super::{OutboundMailOperationError, execute_outbound_mail};
use application_ports::CommandExecutionEvidence;
use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortError,
};
use application_ports::outbound_mail::{
    MailAddress, MailBody, MailRecipients, OutboundMailClaimDecision, OutboundMailIntent,
    OutboundMailIntentApplicationPort, OutboundMailIntentPortError,
    OutboundMailIntentPortErrorClass, OutboundMailIntentReceipt, OutboundMailIntentState,
    OutboundMailOperation, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError, OutboundMailProviderPortErrorClass, OutboundMailReserveDecision,
    ProviderMessageReference,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AuditEventId, ClientId, CorrelationId, IdempotencyKey, MailboxBindingId,
    OutboxEventId, TenantId, TenantScope, UnixMillis,
};
use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

struct FakeAccess {
    allowed: bool,
    calls: AtomicUsize,
}

impl FakeAccess {
    const fn new(allowed: bool) -> Self {
        Self {
            allowed,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ClientMailboxAccessPort for FakeAccess {
    async fn is_mailbox_accessible(
        &self,
        _actor: &ActorContext,
        _client_id: &ClientId,
        _binding_id: &MailboxBindingId,
    ) -> Result<bool, ClientMailboxAccessPortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.allowed)
    }
}

struct StoredIntent {
    idempotency_key: String,
    request_digest: String,
    intent_id: OutboxEventId,
    state: OutboundMailIntentState,
    attempt_count: u8,
    provider_message_reference: Option<ProviderMessageReference>,
}

impl StoredIntent {
    fn receipt(&self) -> OutboundMailIntentReceipt {
        OutboundMailIntentReceipt::new(
            self.intent_id.clone(),
            self.state,
            self.attempt_count,
            self.provider_message_reference.clone(),
        )
    }
}

struct FakeStore {
    stored: Mutex<Option<StoredIntent>>,
    reserve_calls: AtomicUsize,
}

impl FakeStore {
    const fn new() -> Self {
        Self {
            stored: Mutex::new(None),
            reserve_calls: AtomicUsize::new(0),
        }
    }

    fn reserve_calls(&self) -> usize {
        self.reserve_calls.load(Ordering::SeqCst)
    }
}

impl OutboundMailIntentApplicationPort for FakeStore {
    async fn reserve_intent(
        &self,
        _actor: &ActorContext,
        _intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError> {
        self.reserve_calls.fetch_add(1, Ordering::SeqCst);
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        if let Some(existing) = stored.as_ref() {
            if existing.idempotency_key == evidence.idempotency_key().as_str()
                && existing.request_digest == evidence.request_digest()
            {
                return Ok(OutboundMailReserveDecision::Existing(existing.receipt()));
            }
            return Ok(OutboundMailReserveDecision::Conflict);
        }
        *stored = Some(StoredIntent {
            idempotency_key: evidence.idempotency_key().as_str().to_owned(),
            request_digest: evidence.request_digest().to_owned(),
            intent_id: evidence.outbox_event_id().clone(),
            state: OutboundMailIntentState::Pending,
            attempt_count: 0,
            provider_message_reference: None,
        });
        Ok(OutboundMailReserveDecision::Reserved)
    }

    async fn claim_dispatch(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
        max_attempts: u8,
    ) -> Result<OutboundMailClaimDecision, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        if !matches!(
            existing.state,
            OutboundMailIntentState::Pending | OutboundMailIntentState::Retryable
        ) {
            return Ok(OutboundMailClaimDecision::Existing(existing.receipt()));
        }
        if existing.attempt_count >= max_attempts {
            existing.state = OutboundMailIntentState::Rejected;
            return Ok(OutboundMailClaimDecision::Existing(existing.receipt()));
        }
        existing.attempt_count = existing
            .attempt_count
            .checked_add(1)
            .ok_or_else(internal_failure)?;
        existing.state = OutboundMailIntentState::Dispatching;
        Ok(OutboundMailClaimDecision::Claimed {
            attempt: existing.attempt_count,
        })
    }

    async fn complete_dispatch(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
        outcome: &OutboundMailProviderOutcome,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        match outcome {
            OutboundMailProviderOutcome::Sent {
                provider_message_reference,
            } => {
                existing.state = OutboundMailIntentState::Sent;
                existing.provider_message_reference = provider_message_reference.clone();
            }
            OutboundMailProviderOutcome::RetryableNotSent => {
                existing.state = OutboundMailIntentState::Retryable;
                existing.provider_message_reference = None;
            }
            OutboundMailProviderOutcome::Rejected => {
                existing.state = OutboundMailIntentState::Rejected;
                existing.provider_message_reference = None;
            }
            OutboundMailProviderOutcome::Ambiguous => {
                existing.state = OutboundMailIntentState::Ambiguous;
                existing.provider_message_reference = None;
            }
        }
        Ok(existing.receipt())
    }

    async fn mark_ambiguous(
        &self,
        _actor: &ActorContext,
        _evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError> {
        let mut stored = self.stored.lock().map_err(|_| internal_failure())?;
        let Some(existing) = stored.as_mut() else {
            return Err(not_found());
        };
        if existing.state == OutboundMailIntentState::Dispatching {
            existing.state = OutboundMailIntentState::Ambiguous;
        }
        Ok(existing.receipt())
    }
}

struct FakeProvider {
    outcomes: Mutex<VecDeque<Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>>>,
    calls: AtomicUsize,
}

impl FakeProvider {
    fn new(
        outcomes: impl IntoIterator<
            Item = Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>,
        >,
    ) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl OutboundMailProviderPort for FakeProvider {
    async fn send(
        &self,
        _actor: &ActorContext,
        _intent: &OutboundMailIntent,
    ) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut outcomes = self.outcomes.lock().map_err(|_| provider_failure())?;
        let Some(outcome) = outcomes.pop_front() else {
            return Err(provider_failure());
        };
        outcome
    }
}

#[test]
fn unauthorized_request_stops_before_reservation_and_provider()
-> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-unauthorized", "digest_unauthorized_01", "unauthorized")?;
        let access = FakeAccess::new(false);
        let store = FakeStore::new();
        let provider = FakeProvider::new(std::iter::empty::<
            Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>,
        >());

        let result = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await;
        assert_eq!(result, Err(OutboundMailOperationError::NotFound));
        assert_eq!(access.calls(), 1);
        assert_eq!(store.reserve_calls(), 0);
        assert_eq!(provider.calls(), 0);
        Ok(())
    })
}

#[test]
fn successful_replay_never_calls_provider_twice() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-replay", "digest_replay_0000001", "replay")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([Ok(OutboundMailProviderOutcome::Sent {
            provider_message_reference: Some(ProviderMessageReference::parse("provider-msg-1")?),
        })]);

        let first = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await?;
        let replay = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await?;
        assert_eq!(first.state(), OutboundMailIntentState::Sent);
        assert_eq!(replay.state(), OutboundMailIntentState::Sent);
        assert!(replay.replayed());
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}

#[test]
fn same_key_with_different_digest_conflicts_without_resend()
-> Result<(), Box<dyn std::error::Error>> {
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

        execute_outbound_mail(
            &actor,
            &access,
            &store,
            &provider,
            &intent,
            &first_evidence,
        )
        .await?;
        let result = execute_outbound_mail(
            &actor,
            &access,
            &store,
            &provider,
            &intent,
            &conflicting,
        )
        .await;
        assert_eq!(result, Err(OutboundMailOperationError::Conflict));
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}

#[test]
fn confirmed_not_sent_retry_is_bounded_to_three_provider_calls()
-> Result<(), Box<dyn std::error::Error>> {
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
            let outcome = execute_outbound_mail(
                &actor, &access, &store, &provider, &intent, &evidence,
            )
            .await?;
            assert_eq!(outcome.state(), OutboundMailIntentState::Retryable);
            assert_eq!(outcome.attempt_count(), expected_attempt);
        }
        let exhausted = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await?;
        assert_eq!(exhausted.state(), OutboundMailIntentState::Rejected);
        assert_eq!(exhausted.attempt_count(), 3);
        assert_eq!(provider.calls(), 3);
        Ok(())
    })
}

#[test]
fn provider_uncertainty_becomes_ambiguous_without_blind_resend()
-> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-ambiguous", "digest_ambiguous_001", "ambiguous")?;
        let access = FakeAccess::new(true);
        let store = FakeStore::new();
        let provider = FakeProvider::new([Err(provider_failure())]);

        let first = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await?;
        let replay = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await?;
        assert_eq!(first.state(), OutboundMailIntentState::Ambiguous);
        assert_eq!(replay.state(), OutboundMailIntentState::Ambiguous);
        assert!(replay.replayed());
        assert_eq!(provider.calls(), 1);
        Ok(())
    })
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_c4_outbound")?),
        ActorId::parse("actor_c4_outbound")?,
        CorrelationId::parse("correlation_c4_outbound")?,
    ))
}

fn intent() -> Result<OutboundMailIntent, Box<dyn std::error::Error>> {
    let recipients = MailRecipients::new(
        vec![MailAddress::parse("client@example.com")?],
        Vec::new(),
        Vec::new(),
    )?;
    Ok(OutboundMailIntent::new(
        ClientId::parse("client_c4_outbound")?,
        MailboxBindingId::parse("binding_c4_outbound")?,
        OutboundMailOperation::New { recipients },
        None,
        MailBody::new(Some("message".to_owned()), None)?,
    ))
}

fn evidence(
    key: &str,
    digest: &str,
    suffix: &str,
) -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse(key)?,
        digest,
        AuditEventId::parse(format!("audit-{suffix}-c4"))?,
        OutboxEventId::parse(format!("outbox-{suffix}-c4"))?,
        UnixMillis::new(1_000),
        UnixMillis::new(86_401_000),
    ))
}

const fn not_found() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::NotFound)
}

const fn internal_failure() -> OutboundMailIntentPortError {
    OutboundMailIntentPortError::new(OutboundMailIntentPortErrorClass::InternalFailure)
}

const fn provider_failure() -> OutboundMailProviderPortError {
    OutboundMailProviderPortError::new(OutboundMailProviderPortErrorClass::DependencyUnavailable)
}
