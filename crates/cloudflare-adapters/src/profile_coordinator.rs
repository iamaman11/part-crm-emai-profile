use profile_platform_primitives::{
    ActorId, AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId, ProfileId,
    SessionId, TenantId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{
    CoordinatorCommand, CoordinatorCommandEnvelope, CoordinatorConfig, CoordinatorDecision,
    CoordinatorError, CoordinatorOutcome, CoordinatorStatus, ProfileCoordinatorState,
    ReleaseDisposition,
};
use std::fmt;

const MAX_JOURNAL_ENTRIES: usize = 4_096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredCoordinatorDocument {
    tenant_id: String,
    profile_id: String,
    idle_timeout_ms: u64,
    hard_timeout_ms: u64,
    drain_timeout_ms: u64,
    journal: Vec<StoredCoordinatorEnvelope>,
}

impl StoredCoordinatorDocument {
    pub fn new(
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        config: CoordinatorConfig,
    ) -> Self {
        Self {
            tenant_id: tenant_id.as_str().to_owned(),
            profile_id: profile_id.as_str().to_owned(),
            idle_timeout_ms: config.idle_timeout_ms(),
            hard_timeout_ms: config.hard_timeout_ms(),
            drain_timeout_ms: config.drain_timeout_ms(),
            journal: Vec::new(),
        }
    }

    pub fn ensure_identity(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<(), CoordinatorAdapterError> {
        if self.tenant_id != tenant_id.as_str() {
            return Err(CoordinatorAdapterError::TenantMismatch);
        }
        if self.profile_id != profile_id.as_str() {
            return Err(CoordinatorAdapterError::ProfileMismatch);
        }
        Ok(())
    }

    pub fn apply(
        &mut self,
        envelope: StoredCoordinatorEnvelope,
    ) -> Result<CoordinatorApplied, CoordinatorAdapterError> {
        let mut state = self.replay()?;
        let previous_sequence = state.last_sequence();
        let decision = state.apply(envelope.to_domain()?)?;
        let appended = decision.sequence() > previous_sequence;
        if appended {
            if self.journal.len() >= MAX_JOURNAL_ENTRIES {
                return Err(CoordinatorAdapterError::JournalCapacityExceeded);
            }
            self.journal.push(envelope);
        }
        Ok(CoordinatorApplied {
            projection: CoordinatorProjection::from_state(&state),
            decision,
            appended,
            next_alarm_at: next_alarm_at(&state),
        })
    }

    pub fn replay(&self) -> Result<ProfileCoordinatorState, CoordinatorAdapterError> {
        let tenant_id = TenantId::parse(self.tenant_id.clone())?;
        let profile_id = ProfileId::parse(self.profile_id.clone())?;
        let config = CoordinatorConfig::new(
            self.idle_timeout_ms,
            self.hard_timeout_ms,
            self.drain_timeout_ms,
        )?;
        let mut state = ProfileCoordinatorState::new(tenant_id, profile_id, config);
        for envelope in &self.journal {
            state.apply(envelope.to_domain()?)?;
        }
        Ok(state)
    }

    pub fn projection(&self) -> Result<CoordinatorProjection, CoordinatorAdapterError> {
        Ok(CoordinatorProjection::from_state(&self.replay()?))
    }

    #[must_use]
    pub fn journal_len(&self) -> usize {
        self.journal.len()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredCoordinatorEnvelope {
    idempotency_key: String,
    sequence: u64,
    expected_version: u64,
    command: StoredCoordinatorCommand,
}

impl StoredCoordinatorEnvelope {
    #[must_use]
    pub fn new(
        idempotency_key: impl Into<String>,
        sequence: u64,
        expected_version: u64,
        command: StoredCoordinatorCommand,
    ) -> Self {
        Self {
            idempotency_key: idempotency_key.into(),
            sequence,
            expected_version,
            command,
        }
    }

    pub fn to_domain(&self) -> Result<CoordinatorCommandEnvelope, CoordinatorAdapterError> {
        Ok(CoordinatorCommandEnvelope::new(
            IdempotencyKey::parse(self.idempotency_key.clone())?,
            self.sequence,
            AggregateVersion::new(self.expected_version)?,
            self.command.to_domain()?,
        )?)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredReleaseDisposition {
    Clean,
    Dirty,
    Uncertain,
}

impl From<StoredReleaseDisposition> for ReleaseDisposition {
    fn from(value: StoredReleaseDisposition) -> Self {
        match value {
            StoredReleaseDisposition::Clean => Self::Clean,
            StoredReleaseDisposition::Dirty => Self::Dirty,
            StoredReleaseDisposition::Uncertain => Self::Uncertain,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoredCoordinatorCommand {
    IssueLaunchIntent {
        launch_intent_id: String,
        actor_id: String,
        device_id: String,
        now_ms: u64,
        expires_at_ms: u64,
    },
    Claim {
        launch_intent_id: String,
        actor_id: String,
        device_id: String,
        session_id: String,
        fencing_token: String,
        now_ms: u64,
    },
    Heartbeat {
        session_id: String,
        epoch: u64,
        fencing_token: String,
        now_ms: u64,
    },
    Release {
        session_id: String,
        epoch: u64,
        fencing_token: String,
        disposition: StoredReleaseDisposition,
        now_ms: u64,
    },
    BeginDrain {
        now_ms: u64,
    },
    Tick {
        now_ms: u64,
    },
    MarkRecovered {
        now_ms: u64,
    },
}

impl StoredCoordinatorCommand {
    fn to_domain(&self) -> Result<CoordinatorCommand, CoordinatorAdapterError> {
        Ok(match self {
            Self::IssueLaunchIntent {
                launch_intent_id,
                actor_id,
                device_id,
                now_ms,
                expires_at_ms,
            } => CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse(launch_intent_id.clone())?,
                actor_id: ActorId::parse(actor_id.clone())?,
                device_id: DeviceId::parse(device_id.clone())?,
                now: UnixMillis::new(*now_ms),
                expires_at: UnixMillis::new(*expires_at_ms),
            },
            Self::Claim {
                launch_intent_id,
                actor_id,
                device_id,
                session_id,
                fencing_token,
                now_ms,
            } => CoordinatorCommand::Claim {
                launch_intent_id: LaunchIntentId::parse(launch_intent_id.clone())?,
                actor_id: ActorId::parse(actor_id.clone())?,
                device_id: DeviceId::parse(device_id.clone())?,
                session_id: SessionId::parse(session_id.clone())?,
                fencing_token: FencingToken::parse(fencing_token.clone())?,
                now: UnixMillis::new(*now_ms),
            },
            Self::Heartbeat {
                session_id,
                epoch,
                fencing_token,
                now_ms,
            } => CoordinatorCommand::Heartbeat {
                session_id: SessionId::parse(session_id.clone())?,
                epoch: *epoch,
                fencing_token: FencingToken::parse(fencing_token.clone())?,
                now: UnixMillis::new(*now_ms),
            },
            Self::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
                now_ms,
            } => CoordinatorCommand::Release {
                session_id: SessionId::parse(session_id.clone())?,
                epoch: *epoch,
                fencing_token: FencingToken::parse(fencing_token.clone())?,
                disposition: (*disposition).into(),
                now: UnixMillis::new(*now_ms),
            },
            Self::BeginDrain { now_ms } => CoordinatorCommand::BeginDrain {
                now: UnixMillis::new(*now_ms),
            },
            Self::Tick { now_ms } => CoordinatorCommand::Tick {
                now: UnixMillis::new(*now_ms),
            },
            Self::MarkRecovered { now_ms } => CoordinatorCommand::MarkRecovered {
                now: UnixMillis::new(*now_ms),
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorApplied {
    projection: CoordinatorProjection,
    decision: CoordinatorDecision,
    appended: bool,
    next_alarm_at: Option<UnixMillis>,
}

impl CoordinatorApplied {
    #[must_use]
    pub const fn projection(&self) -> &CoordinatorProjection {
        &self.projection
    }

    #[must_use]
    pub const fn decision(&self) -> &CoordinatorDecision {
        &self.decision
    }

    #[must_use]
    pub const fn appended(&self) -> bool {
        self.appended
    }

    #[must_use]
    pub const fn next_alarm_at(&self) -> Option<UnixMillis> {
        self.next_alarm_at
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoordinatorProjection {
    pub tenant_id: String,
    pub profile_id: String,
    pub status: String,
    pub version: u64,
    pub sequence: u64,
    pub next_epoch: u64,
    pub active_session_id: Option<String>,
    pub active_device_id: Option<String>,
    pub active_epoch: Option<u64>,
    pub idle_expires_at_ms: Option<u64>,
    pub hard_expires_at_ms: Option<u64>,
    pub drain_deadline_ms: Option<u64>,
    pub pending_launch_intent_id: Option<String>,
    pub pending_intent_expires_at_ms: Option<u64>,
}

impl CoordinatorProjection {
    fn from_state(state: &ProfileCoordinatorState) -> Self {
        let lease = state.active_lease();
        let intent = state.pending_intent();
        Self {
            tenant_id: state.tenant_id().as_str().to_owned(),
            profile_id: state.profile_id().as_str().to_owned(),
            status: status_name(state.status()).to_owned(),
            version: state.version().value(),
            sequence: state.last_sequence(),
            next_epoch: state.next_epoch(),
            active_session_id: lease.map(|value| value.session_id().as_str().to_owned()),
            active_device_id: lease.map(|value| value.device_id().as_str().to_owned()),
            active_epoch: lease.map(session_domain::coordinator::CoordinatorLease::epoch),
            idle_expires_at_ms: lease.map(|value| value.idle_expires_at().value()),
            hard_expires_at_ms: lease.map(|value| value.hard_expires_at().value()),
            drain_deadline_ms: state.drain_deadline().map(UnixMillis::value),
            pending_launch_intent_id: intent
                .map(|value| value.launch_intent_id().as_str().to_owned()),
            pending_intent_expires_at_ms: intent.map(|value| value.expires_at().value()),
        }
    }
}

#[must_use]
pub fn outcome_name(outcome: &CoordinatorOutcome) -> &'static str {
    match outcome {
        CoordinatorOutcome::LaunchIntentIssued { .. } => "launch_intent_issued",
        CoordinatorOutcome::LeaseClaimed { .. } => "lease_claimed",
        CoordinatorOutcome::HeartbeatAccepted { .. } => "heartbeat_accepted",
        CoordinatorOutcome::Released { .. } => "released",
        CoordinatorOutcome::DrainStarted { .. } => "drain_started",
        CoordinatorOutcome::TimedOut { .. } => "timed_out",
        CoordinatorOutcome::LaunchIntentExpired => "launch_intent_expired",
        CoordinatorOutcome::Recovered => "recovered",
        CoordinatorOutcome::NoChange => "no_change",
    }
}

fn status_name(status: CoordinatorStatus) -> &'static str {
    match status {
        CoordinatorStatus::Idle => "idle",
        CoordinatorStatus::Active => "active",
        CoordinatorStatus::Draining => "draining",
        CoordinatorStatus::Dirty => "dirty",
        CoordinatorStatus::Uncertain => "uncertain",
    }
}

fn next_alarm_at(state: &ProfileCoordinatorState) -> Option<UnixMillis> {
    let intent_deadline = state.pending_intent().map(|intent| intent.expires_at());
    let lease_deadline = state.active_lease().map(|lease| {
        let mut deadline = lease.idle_expires_at();
        if lease.hard_expires_at() < deadline {
            deadline = lease.hard_expires_at();
        }
        if let Some(drain_deadline) = state.drain_deadline()
            && drain_deadline < deadline
        {
            deadline = drain_deadline;
        }
        deadline
    });
    match (intent_deadline, lease_deadline) {
        (Some(left), Some(right)) => Some(if left < right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[derive(Debug)]
pub enum CoordinatorAdapterError {
    Identifier(profile_platform_primitives::ParseOpaqueIdError),
    ZeroVersion(profile_platform_primitives::ZeroAggregateVersion),
    Domain(CoordinatorError),
    TenantMismatch,
    ProfileMismatch,
    JournalCapacityExceeded,
}

impl fmt::Display for CoordinatorAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(error) => error.fmt(formatter),
            Self::ZeroVersion(error) => error.fmt(formatter),
            Self::Domain(error) => error.fmt(formatter),
            Self::TenantMismatch => formatter.write_str("coordinator tenant mismatch"),
            Self::ProfileMismatch => formatter.write_str("coordinator profile mismatch"),
            Self::JournalCapacityExceeded => {
                formatter.write_str("coordinator journal capacity exceeded")
            }
        }
    }
}

impl std::error::Error for CoordinatorAdapterError {}

impl From<profile_platform_primitives::ParseOpaqueIdError> for CoordinatorAdapterError {
    fn from(value: profile_platform_primitives::ParseOpaqueIdError) -> Self {
        Self::Identifier(value)
    }
}

impl From<profile_platform_primitives::ZeroAggregateVersion> for CoordinatorAdapterError {
    fn from(value: profile_platform_primitives::ZeroAggregateVersion) -> Self {
        Self::ZeroVersion(value)
    }
}

impl From<CoordinatorError> for CoordinatorAdapterError {
    fn from(value: CoordinatorError) -> Self {
        Self::Domain(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinatorAdapterError, StoredCoordinatorCommand, StoredCoordinatorDocument,
        StoredCoordinatorEnvelope, StoredReleaseDisposition,
    };
    use profile_platform_primitives::{ProfileId, TenantId};
    use session_domain::coordinator::CoordinatorConfig;

    fn document() -> Result<StoredCoordinatorDocument, Box<dyn std::error::Error>> {
        Ok(StoredCoordinatorDocument::new(
            &TenantId::parse("tenant_01JADAPTER")?,
            &ProfileId::parse("profile_01JADAPTER")?,
            CoordinatorConfig::new(10, 100, 20)?,
        ))
    }

    #[test]
    fn journal_replays_and_preserves_idempotent_decision(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut document = document()?;
        let issue = StoredCoordinatorEnvelope::new(
            "idem_issue_01JADAPTER",
            1,
            1,
            StoredCoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: "intent_01JADAPTER".to_owned(),
                actor_id: "actor_01JADAPTER".to_owned(),
                device_id: "device_01JADAPTER".to_owned(),
                now_ms: 10,
                expires_at_ms: 50,
            },
        );
        let first = document.apply(issue.clone())?;
        let duplicate = document.apply(issue)?;
        assert!(first.appended());
        assert!(!duplicate.appended());
        assert_eq!(first.decision(), duplicate.decision());
        assert_eq!(document.journal_len(), 1);
        assert_eq!(document.projection()?.sequence, 1);
        Ok(())
    }

    #[test]
    fn persisted_turnover_fences_old_writer() -> Result<(), Box<dyn std::error::Error>> {
        let mut document = document()?;
        document.apply(StoredCoordinatorEnvelope::new(
            "idem_issue_01JADAPTER",
            1,
            1,
            StoredCoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: "intent_01JADAPTER".to_owned(),
                actor_id: "actor_01JADAPTER".to_owned(),
                device_id: "device_01JADAPTER".to_owned(),
                now_ms: 10,
                expires_at_ms: 50,
            },
        ))?;
        document.apply(StoredCoordinatorEnvelope::new(
            "idem_claim_01JADAPTER",
            2,
            2,
            StoredCoordinatorCommand::Claim {
                launch_intent_id: "intent_01JADAPTER".to_owned(),
                actor_id: "actor_01JADAPTER".to_owned(),
                device_id: "device_01JADAPTER".to_owned(),
                session_id: "session_01JADAPTER".to_owned(),
                fencing_token: "fence_01JADAPTER".to_owned(),
                now_ms: 11,
            },
        ))?;
        document.apply(StoredCoordinatorEnvelope::new(
            "idem_release_01JADAPTER",
            3,
            3,
            StoredCoordinatorCommand::Release {
                session_id: "session_01JADAPTER".to_owned(),
                epoch: 1,
                fencing_token: "fence_01JADAPTER".to_owned(),
                disposition: StoredReleaseDisposition::Clean,
                now_ms: 12,
            },
        ))?;

        let stale = document.apply(StoredCoordinatorEnvelope::new(
            "idem_stale_01JADAPTER",
            4,
            4,
            StoredCoordinatorCommand::Heartbeat {
                session_id: "session_01JADAPTER".to_owned(),
                epoch: 1,
                fencing_token: "fence_01JADAPTER".to_owned(),
                now_ms: 13,
            },
        ));
        assert!(matches!(stale, Err(CoordinatorAdapterError::Domain(_))));
        assert_eq!(document.journal_len(), 3);
        Ok(())
    }

    #[test]
    fn identity_mismatch_is_never_rebound() -> Result<(), Box<dyn std::error::Error>> {
        let document = document()?;
        assert_eq!(
            document.ensure_identity(
                &TenantId::parse("tenant_02JADAPTER")?,
                &ProfileId::parse("profile_01JADAPTER")?,
            ),
            Err(CoordinatorAdapterError::TenantMismatch)
        );
        Ok(())
    }
}
