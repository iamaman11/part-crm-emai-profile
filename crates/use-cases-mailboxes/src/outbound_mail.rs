use application_ports::CommandExecutionEvidence;
use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortError, ClientMailboxAccessPortErrorClass,
};
use application_ports::outbound_mail::{
    OutboundMailClaimDecision, OutboundMailInputError, OutboundMailIntent,
    OutboundMailIntentApplicationPort, OutboundMailIntentPortError, OutboundMailIntentPortErrorClass,
    OutboundMailIntentReceipt, OutboundMailIntentState, OutboundMailProviderOutcome,
    OutboundMailProviderPort, OutboundMailReserveDecision,
};
use core::fmt;
use profile_platform_primitives::{ActorContext, OutboxEventId};

pub const MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundMailOutcome {
    intent_id: OutboxEventId,
    state: OutboundMailIntentState,
    attempt_count: u8,
    replayed: bool,
}

impl OutboundMailOutcome {
    #[must_use]
    pub const fn intent_id(&self) -> &OutboxEventId {
        &self.intent_id
    }

    #[must_use]
    pub const fn state(&self) -> OutboundMailIntentState {
        self.state
    }

    #[must_use]
    pub const fn attempt_count(&self) -> u8 {
        self.attempt_count
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailOperationError {
    NotFound,
    InvalidInput,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for OutboundMailOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "outbound mailbox target not found",
            Self::InvalidInput => "outbound mail input is invalid",
            Self::Conflict => "outbound mail command conflict",
            Self::IntegrityFailure => "outbound mail integrity failure",
            Self::InternalFailure => "outbound mail internal failure",
            Self::DependencyUnavailable => "outbound mail dependency unavailable",
        })
    }
}

impl std::error::Error for OutboundMailOperationError {}

pub async fn execute_outbound_mail<A, S, P>(
    actor: &ActorContext,
    access: &A,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    A: ClientMailboxAccessPort,
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    let accessible = access
        .is_mailbox_accessible(actor, intent.client_id(), intent.binding_id())
        .await
        .map_err(map_access_error)?;
    if !accessible {
        return Err(OutboundMailOperationError::NotFound);
    }

    intent
        .validate_source_binding()
        .map_err(map_input_error)?;

    let reserve = store
        .reserve_intent(actor, intent, evidence)
        .await
        .map_err(map_store_error)?;

    match reserve {
        OutboundMailReserveDecision::Conflict => Err(OutboundMailOperationError::Conflict),
        OutboundMailReserveDecision::Reserved => {
            dispatch(actor, store, provider, intent, evidence, false).await
        }
        OutboundMailReserveDecision::Existing(receipt) => {
            resume_or_replay(actor, store, provider, intent, evidence, receipt).await
        }
    }
}

async fn resume_or_replay<S, P>(
    actor: &ActorContext,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
    receipt: OutboundMailIntentReceipt,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    match receipt.state() {
        OutboundMailIntentState::Pending | OutboundMailIntentState::Retryable => {
            dispatch(actor, store, provider, intent, evidence, true).await
        }
        OutboundMailIntentState::Dispatching => {
            let receipt = store
                .mark_ambiguous(actor, evidence)
                .await
                .unwrap_or(receipt);
            Ok(outcome_from_receipt(receipt, true))
        }
        OutboundMailIntentState::Sent
        | OutboundMailIntentState::Ambiguous
        | OutboundMailIntentState::Rejected => Ok(outcome_from_receipt(receipt, true)),
    }
}

async fn dispatch<S, P>(
    actor: &ActorContext,
    store: &S,
    provider: &P,
    intent: &OutboundMailIntent,
    evidence: &CommandExecutionEvidence,
    replayed: bool,
) -> Result<OutboundMailOutcome, OutboundMailOperationError>
where
    S: OutboundMailIntentApplicationPort,
    P: OutboundMailProviderPort,
{
    let claim = store
        .claim_dispatch(actor, evidence, MAX_OUTBOUND_MAIL_DISPATCH_ATTEMPTS)
        .await
        .map_err(map_store_error)?;

    let attempt = match claim {
        OutboundMailClaimDecision::Claimed { attempt } => attempt,
        OutboundMailClaimDecision::Existing(receipt) => {
            if receipt.state() == OutboundMailIntentState::Dispatching {
                let receipt = store
                    .mark_ambiguous(actor, evidence)
                    .await
                    .unwrap_or(receipt);
                return Ok(outcome_from_receipt(receipt, true));
            }
            return Ok(outcome_from_receipt(receipt, true));
        }
    };

    let provider_outcome = match provider.send(actor, intent).await {
        Ok(outcome) => outcome,
        Err(_) => OutboundMailProviderOutcome::Ambiguous,
    };

    match store
        .complete_dispatch(actor, evidence, &provider_outcome)
        .await
    {
        Ok(receipt) => Ok(outcome_from_receipt(receipt, replayed)),
        Err(_) => {
            let fallback = store.mark_ambiguous(actor, evidence).await.ok();
            Ok(fallback.map_or_else(
                || OutboundMailOutcome {
                    intent_id: evidence.outbox_event_id().clone(),
                    state: OutboundMailIntentState::Ambiguous,
                    attempt_count: attempt,
                    replayed,
                },
                |receipt| outcome_from_receipt(receipt, replayed),
            ))
        }
    }
}

fn outcome_from_receipt(receipt: OutboundMailIntentReceipt, replayed: bool) -> OutboundMailOutcome {
    OutboundMailOutcome {
        intent_id: receipt.intent_id().clone(),
        state: receipt.state(),
        attempt_count: receipt.attempt_count(),
        replayed,
    }
}

const fn map_input_error(_error: OutboundMailInputError) -> OutboundMailOperationError {
    OutboundMailOperationError::InvalidInput
}

const fn map_access_error(error: ClientMailboxAccessPortError) -> OutboundMailOperationError {
    match error.class() {
        ClientMailboxAccessPortErrorClass::IntegrityFailure => {
            OutboundMailOperationError::IntegrityFailure
        }
        ClientMailboxAccessPortErrorClass::DependencyUnavailable => {
            OutboundMailOperationError::DependencyUnavailable
        }
    }
}

const fn map_store_error(error: OutboundMailIntentPortError) -> OutboundMailOperationError {
    match error.class() {
        OutboundMailIntentPortErrorClass::NotFound => OutboundMailOperationError::NotFound,
        OutboundMailIntentPortErrorClass::Conflict => OutboundMailOperationError::Conflict,
        OutboundMailIntentPortErrorClass::IntegrityFailure => {
            OutboundMailOperationError::IntegrityFailure
        }
        OutboundMailIntentPortErrorClass::InternalFailure => {
            OutboundMailOperationError::InternalFailure
        }
        OutboundMailIntentPortErrorClass::DependencyUnavailable => {
            OutboundMailOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
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
        OutboundMailProviderPortError, OutboundMailProviderPortErrorClass,
        OutboundMailReserveDecision, ProviderMessageReference,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, AuditEventId, ClientId, CorrelationId, IdempotencyKey,
        MailboxBindingId, OutboxEventId, TenantId, TenantScope, UnixMillis,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[tokio::test]
    async fn unauthorized_request_stops_before_reservation_and_provider(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let intent = intent()?;
        let evidence = evidence("send-key-unauthorized", "digest_unauthorized_01", "unauthorized")?;
        let access = FakeAccess::new(false);
        let store = FakeStore::new();
        let provider = FakeProvider::new([]);

        let result = execute_outbound_mail(
            &actor, &access, &store, &provider, &intent, &evidence,
        )
        .await;
        assert_eq!(result, Err(OutboundMailOperationError::NotFound));
        assert_eq!(access.calls(), 1);
        assert_eq!(store.reserve_calls(), 0);
        assert_eq!(provider.calls(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn successful_replay_never_calls_provider_twice() -> Result<(), Box<dyn std::error::Error>> {
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
    }

    #[tokio::test]
    async fn same_key_with_different_digest_conflicts_without_resend(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    }

    #[tokio::test]
    async fn confirmed_not_sent_retry_is_bounded_to_three_provider_calls(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    }

    #[tokio::test]
    async fn provider_uncertainty_becomes_ambiguous_without_blind_resend(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
}
