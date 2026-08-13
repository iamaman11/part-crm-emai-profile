use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError, OutboundMailProviderPortErrorClass,
};
use profile_platform_primitives::ActorContext;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct FakeProvider {
    outcomes: Mutex<VecDeque<Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>>>,
    calls: AtomicUsize,
}

impl FakeProvider {
    pub(super) fn new(
        outcomes: impl IntoIterator<
            Item = Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>,
        >,
    ) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            calls: AtomicUsize::new(0),
        }
    }

    pub(super) fn calls(&self) -> usize {
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

pub(super) const fn provider_failure() -> OutboundMailProviderPortError {
    OutboundMailProviderPortError::new(OutboundMailProviderPortErrorClass::DependencyUnavailable)
}
