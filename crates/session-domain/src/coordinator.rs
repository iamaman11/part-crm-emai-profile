use core::fmt;
use profile_platform_primitives::{
    ActorId, AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId, ProfileId,
    SessionId, TenantId, UnixMillis,
};

const RECEIPT_LIMIT: usize = 32;
const OBJECT_NAME_PREFIX: &str = "profile-coordinator-v1:";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoordinatorConfig {
    idle_timeout_ms: u64,
    hard_timeout_ms: u64,
    drain_timeout_ms: u64,
}

impl CoordinatorConfig {
    pub const fn new(
        idle_timeout_ms: u64,
        hard_timeout_ms: u64,
        drain_timeout_ms: u64,
    ) -> Result<Self, CoordinatorError> {
        if idle_timeout_ms == 0
            || hard_timeout_ms == 0
            || drain_timeout_ms == 0
            || idle_timeout_ms > hard_timeout_ms
        {
            return Err(CoordinatorError::InvalidTimeoutConfig);
        }
        Ok(Self {
            idle_timeout_ms,
            hard_timeout_ms,
            drain_timeout_ms,
        })
    }

    #[must_use]
    pub const fn idle_timeout_ms(self) -> u64 {
        self.idle_timeout_ms
    }

    #[must_use]
    pub const fn hard_timeout_ms(self) -> u64 {
        self.hard_timeout_ms
    }

    #[must_use]
    pub const fn drain_timeout_ms(self) -> u64 {
        self.drain_timeout_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorStatus {
    Idle,
    Active,
    Draining,
    Dirty,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseDisposition {
    Clean,
    Dirty,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutKind {
    Idle,
    Hard,
    Drain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingLaunchIntent {
    launch_intent_id: LaunchIntentId,
    actor_id: ActorId,
    device_id: DeviceId,
    expires_at: UnixMillis,
}

impl PendingLaunchIntent {
    #[must_use]
    pub const fn launch_intent_id(&self) -> &LaunchIntentId {
        &self.launch_intent_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn restored(
        launch_intent_id: LaunchIntentId,
        actor_id: ActorId,
        device_id: DeviceId,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            launch_intent_id,
            actor_id,
            device_id,
            expires_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorLease {
    session_id: SessionId,
    device_id: DeviceId,
    epoch: u64,
    fencing_token: FencingToken,
    claimed_at: UnixMillis,
    last_heartbeat_at: UnixMillis,
    idle_expires_at: UnixMillis,
    hard_expires_at: UnixMillis,
}

impl CoordinatorLease {
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub const fn fencing_token(&self) -> &FencingToken {
        &self.fencing_token
    }

    #[must_use]
    pub const fn claimed_at(&self) -> UnixMillis {
        self.claimed_at
    }

    #[must_use]
    pub const fn last_heartbeat_at(&self) -> UnixMillis {
        self.last_heartbeat_at
    }

    #[must_use]
    pub const fn idle_expires_at(&self) -> UnixMillis {
        self.idle_expires_at
    }

    #[must_use]
    pub const fn hard_expires_at(&self) -> UnixMillis {
        self.hard_expires_at
    }

    /// Parsed checkpoint data cannot manufacture a valid writer state.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        session_id: SessionId,
        device_id: DeviceId,
        epoch: u64,
        fencing_token: FencingToken,
        claimed_at: UnixMillis,
        last_heartbeat_at: UnixMillis,
        idle_expires_at: UnixMillis,
        hard_expires_at: UnixMillis,
    ) -> Result<Self, CoordinatorError> {
        if epoch == 0
            || last_heartbeat_at < claimed_at
            || idle_expires_at < last_heartbeat_at
            || hard_expires_at < idle_expires_at
        {
            return Err(CoordinatorError::InvalidCheckpoint);
        }
        Ok(Self {
            session_id,
            device_id,
            epoch,
            fencing_token,
            claimed_at,
            last_heartbeat_at,
            idle_expires_at,
            hard_expires_at,
        })
    }

    #[must_use]
    pub fn accepts_writer(
        &self,
        session_id: &SessionId,
        epoch: u64,
        fencing_token: &FencingToken,
    ) -> bool {
        &self.session_id == session_id
            && self.epoch == epoch
            && &self.fencing_token == fencing_token
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinatorCommand {
    IssueLaunchIntent {
        launch_intent_id: LaunchIntentId,
        actor_id: ActorId,
        device_id: DeviceId,
        now: UnixMillis,
        expires_at: UnixMillis,
    },
    /// Atomic uncertain -> same-device launch after checked original
    /// Claim provenance in the Durable Object adapter.
    RecoverAndIssueLaunchIntent {
        launch_intent_id: LaunchIntentId,
        actor_id: ActorId,
        device_id: DeviceId,
        now: UnixMillis,
        expires_at: UnixMillis,
    },
    Claim {
        launch_intent_id: LaunchIntentId,
        actor_id: ActorId,
        device_id: DeviceId,
        session_id: SessionId,
        fencing_token: FencingToken,
        now: UnixMillis,
    },
    Heartbeat {
        session_id: SessionId,
        epoch: u64,
        fencing_token: FencingToken,
        now: UnixMillis,
    },
    /// Distinct journal command: unlike legacy Heartbeat, an authenticated
    /// renewal advances the rolling hard deadline. Old events replay exactly
    /// as written, without silently changing historical transition semantics.
    RenewLease {
        session_id: SessionId,
        epoch: u64,
        fencing_token: FencingToken,
        now: UnixMillis,
    },
    Release {
        session_id: SessionId,
        epoch: u64,
        fencing_token: FencingToken,
        disposition: ReleaseDisposition,
        now: UnixMillis,
    },
    BeginDrain {
        now: UnixMillis,
    },
    Tick {
        now: UnixMillis,
    },
    MarkRecovered {
        now: UnixMillis,
    },
}

impl CoordinatorCommand {
    #[must_use]
    pub const fn observed_at(&self) -> UnixMillis {
        match self {
            Self::IssueLaunchIntent { now, .. }
            | Self::RecoverAndIssueLaunchIntent { now, .. }
            | Self::Claim { now, .. }
            | Self::Heartbeat { now, .. }
            | Self::RenewLease { now, .. }
            | Self::Release { now, .. }
            | Self::BeginDrain { now }
            | Self::Tick { now }
            | Self::MarkRecovered { now } => *now,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorCommandEnvelope {
    idempotency_key: IdempotencyKey,
    sequence: u64,
    expected_version: AggregateVersion,
    command: CoordinatorCommand,
}

impl CoordinatorCommandEnvelope {
    pub fn new(
        idempotency_key: IdempotencyKey,
        sequence: u64,
        expected_version: AggregateVersion,
        command: CoordinatorCommand,
    ) -> Result<Self, CoordinatorError> {
        if sequence == 0 {
            return Err(CoordinatorError::InvalidSequence);
        }
        Ok(Self {
            idempotency_key,
            sequence,
            expected_version,
            command,
        })
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn command(&self) -> &CoordinatorCommand {
        &self.command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinatorOutcome {
    LaunchIntentIssued { launch_intent_id: LaunchIntentId },
    LeaseClaimed { lease: CoordinatorLease },
    HeartbeatAccepted { idle_expires_at: UnixMillis },
    Released { disposition: ReleaseDisposition },
    DrainStarted { deadline: UnixMillis },
    TimedOut { kind: TimeoutKind },
    LaunchIntentExpired,
    Recovered,
    NoChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorDecision {
    version: AggregateVersion,
    sequence: u64,
    outcome: CoordinatorOutcome,
}

impl CoordinatorDecision {
    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn outcome(&self) -> &CoordinatorOutcome {
        &self.outcome
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandReceipt {
    idempotency_key: IdempotencyKey,
    sequence: u64,
    command: CoordinatorCommand,
    decision: CoordinatorDecision,
}

/// Durable replay anchor for bounded DO journals. It is owned by the
/// original coordinator and never constitutes an independent lease authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorCheckpoint {
    pub version: AggregateVersion,
    pub sequence: u64,
    pub observed_at: UnixMillis,
    pub next_epoch: u64,
    pub status: CoordinatorStatus,
    pub pending_intent: Option<PendingLaunchIntent>,
    /// Keep the Idle projection blocked until an exact original-device
    /// recovery claim succeeds; intent expiry alone is never admission.
    pub recovery_claim_pending: bool,
    pub active_lease: Option<CoordinatorLease>,
    pub drain_deadline: Option<UnixMillis>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileCoordinatorState {
    tenant_id: TenantId,
    profile_id: ProfileId,
    config: CoordinatorConfig,
    version: AggregateVersion,
    last_sequence: u64,
    last_observed_at: UnixMillis,
    next_epoch: u64,
    status: CoordinatorStatus,
    pending_intent: Option<PendingLaunchIntent>,
    recovery_claim_pending: bool,
    active_lease: Option<CoordinatorLease>,
    drain_deadline: Option<UnixMillis>,
    receipts: Vec<CommandReceipt>,
}

impl ProfileCoordinatorState {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        config: CoordinatorConfig,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            config,
            version: AggregateVersion::INITIAL,
            last_sequence: 0,
            last_observed_at: UnixMillis::new(0),
            next_epoch: 0,
            status: CoordinatorStatus::Idle,
            pending_intent: None,
            recovery_claim_pending: false,
            active_lease: None,
            drain_deadline: None,
            receipts: Vec::new(),
        }
    }

    #[must_use]
    pub fn checkpoint(&self) -> CoordinatorCheckpoint {
        CoordinatorCheckpoint {
            version: self.version,
            sequence: self.last_sequence,
            observed_at: self.last_observed_at,
            next_epoch: self.next_epoch,
            status: self.status,
            pending_intent: self.pending_intent.clone(),
            recovery_claim_pending: self.recovery_claim_pending,
            active_lease: self.active_lease.clone(),
            drain_deadline: self.drain_deadline,
        }
    }

    /// Restore only a validated, canonical replay checkpoint. The trailing
    /// bounded command journal repopulates recent idempotency receipts.
    pub fn restore_checkpoint(
        tenant_id: TenantId,
        profile_id: ProfileId,
        config: CoordinatorConfig,
        snapshot: CoordinatorCheckpoint,
    ) -> Result<Self, CoordinatorError> {
        let expected_version = snapshot
            .sequence
            .checked_add(1)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        let has_writer = snapshot.active_lease.is_some();
        let active_status = matches!(
            snapshot.status,
            CoordinatorStatus::Active | CoordinatorStatus::Draining
        );
        if snapshot.version.value() != expected_version
            || has_writer != active_status
            || snapshot
                .active_lease
                .as_ref()
                .is_some_and(|lease| lease.epoch() > snapshot.next_epoch)
            || (snapshot.pending_intent.is_some()
                && (snapshot.status != CoordinatorStatus::Idle || has_writer))
            || (snapshot.recovery_claim_pending
                && (snapshot.status != CoordinatorStatus::Idle || has_writer))
            || (snapshot.drain_deadline.is_some()
                != (snapshot.status == CoordinatorStatus::Draining))
        {
            return Err(CoordinatorError::InvalidCheckpoint);
        }
        Ok(Self {
            tenant_id,
            profile_id,
            config,
            version: snapshot.version,
            last_sequence: snapshot.sequence,
            last_observed_at: snapshot.observed_at,
            next_epoch: snapshot.next_epoch,
            status: snapshot.status,
            pending_intent: snapshot.pending_intent,
            recovery_claim_pending: snapshot.recovery_claim_pending,
            active_lease: snapshot.active_lease,
            drain_deadline: snapshot.drain_deadline,
            receipts: Vec::new(),
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn config(&self) -> CoordinatorConfig {
        self.config
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    #[must_use]
    pub const fn last_observed_at(&self) -> UnixMillis {
        self.last_observed_at
    }

    #[must_use]
    pub const fn next_epoch(&self) -> u64 {
        self.next_epoch
    }

    #[must_use]
    pub const fn status(&self) -> CoordinatorStatus {
        self.status
    }

    /// An expired recovery intent remains exclusive until the same original
    /// actor/device obtains a new Claim. The adapter checks historical Claim
    /// provenance for every IssueLaunchIntent while this fence is set.
    #[must_use]
    pub const fn recovery_claim_pending(&self) -> bool {
        self.recovery_claim_pending
    }

    #[must_use]
    pub const fn pending_intent(&self) -> Option<&PendingLaunchIntent> {
        self.pending_intent.as_ref()
    }

    #[must_use]
    pub const fn active_lease(&self) -> Option<&CoordinatorLease> {
        self.active_lease.as_ref()
    }

    #[must_use]
    pub const fn drain_deadline(&self) -> Option<UnixMillis> {
        self.drain_deadline
    }

    pub fn apply(
        &mut self,
        envelope: CoordinatorCommandEnvelope,
    ) -> Result<CoordinatorDecision, CoordinatorError> {
        if let Some(receipt) = self
            .receipts
            .iter()
            .find(|receipt| receipt.idempotency_key == envelope.idempotency_key)
        {
            if receipt.sequence == envelope.sequence && receipt.command == envelope.command {
                return Ok(receipt.decision.clone());
            }
            return Err(CoordinatorError::IdempotencyConflict);
        }

        let expected_sequence = self
            .last_sequence
            .checked_add(1)
            .ok_or(CoordinatorError::SequenceOverflow)?;
        if envelope.sequence != expected_sequence {
            return Err(CoordinatorError::ReorderedCommand);
        }
        if envelope.expected_version != self.version {
            return Err(CoordinatorError::ExpectedVersionMismatch);
        }
        if envelope.command.observed_at() < self.last_observed_at {
            return Err(CoordinatorError::ReorderedTime);
        }

        let next_version = self
            .version
            .next()
            .map_err(|_| CoordinatorError::VersionOverflow)?;
        let outcome = self.apply_new(&envelope.command)?;
        self.version = next_version;
        self.last_sequence = envelope.sequence;
        self.last_observed_at = envelope.command.observed_at();

        let decision = CoordinatorDecision {
            version: self.version,
            sequence: envelope.sequence,
            outcome,
        };
        self.receipts.push(CommandReceipt {
            idempotency_key: envelope.idempotency_key,
            sequence: envelope.sequence,
            command: envelope.command,
            decision: decision.clone(),
        });
        if self.receipts.len() > RECEIPT_LIMIT {
            self.receipts.remove(0);
        }
        Ok(decision)
    }

    fn apply_new(
        &mut self,
        command: &CoordinatorCommand,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        match command {
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id,
                actor_id,
                device_id,
                now,
                expires_at,
            } => self.issue_launch_intent(launch_intent_id, actor_id, device_id, *now, *expires_at),
            CoordinatorCommand::RecoverAndIssueLaunchIntent {
                launch_intent_id,
                actor_id,
                device_id,
                now,
                expires_at,
            } => self.recover_and_issue_launch_intent(
                launch_intent_id,
                actor_id,
                device_id,
                *now,
                *expires_at,
            ),
            CoordinatorCommand::Claim {
                launch_intent_id,
                actor_id,
                device_id,
                session_id,
                fencing_token,
                now,
            } => self.claim(
                launch_intent_id,
                actor_id,
                device_id,
                session_id,
                fencing_token,
                *now,
            ),
            CoordinatorCommand::Heartbeat {
                session_id,
                epoch,
                fencing_token,
                now,
            } => self.heartbeat(session_id, *epoch, fencing_token, *now, false),
            CoordinatorCommand::RenewLease {
                session_id,
                epoch,
                fencing_token,
                now,
            } => self.heartbeat(session_id, *epoch, fencing_token, *now, true),
            CoordinatorCommand::Release {
                session_id,
                epoch,
                fencing_token,
                disposition,
                now,
            } => self.release(session_id, *epoch, fencing_token, *disposition, *now),
            CoordinatorCommand::BeginDrain { now } => self.begin_drain(*now),
            CoordinatorCommand::Tick { now } => self.tick(*now),
            CoordinatorCommand::MarkRecovered { now } => self.mark_recovered(*now),
        }
    }

    fn issue_launch_intent(
        &mut self,
        launch_intent_id: &LaunchIntentId,
        actor_id: &ActorId,
        device_id: &DeviceId,
        now: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        if expires_at <= now {
            return Err(CoordinatorError::InvalidIntentExpiry);
        }
        if self.active_lease.is_some() || self.status != CoordinatorStatus::Idle {
            return Err(CoordinatorError::CoordinatorUnavailable);
        }
        if self
            .pending_intent
            .as_ref()
            .is_some_and(|intent| intent.expires_at > now)
        {
            return Err(CoordinatorError::PendingIntentExists);
        }

        self.pending_intent = Some(PendingLaunchIntent {
            launch_intent_id: launch_intent_id.clone(),
            actor_id: actor_id.clone(),
            device_id: device_id.clone(),
            expires_at,
        });
        Ok(CoordinatorOutcome::LaunchIntentIssued {
            launch_intent_id: launch_intent_id.clone(),
        })
    }

    fn claim(
        &mut self,
        launch_intent_id: &LaunchIntentId,
        actor_id: &ActorId,
        device_id: &DeviceId,
        session_id: &SessionId,
        fencing_token: &FencingToken,
        now: UnixMillis,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        if self.status != CoordinatorStatus::Idle || self.active_lease.is_some() {
            return Err(CoordinatorError::CoordinatorUnavailable);
        }
        let intent = self
            .pending_intent
            .as_ref()
            .ok_or(CoordinatorError::LaunchIntentMissing)?;
        if intent.launch_intent_id != *launch_intent_id {
            return Err(CoordinatorError::LaunchIntentMismatch);
        }
        if intent.actor_id != *actor_id {
            return Err(CoordinatorError::ActorMismatch);
        }
        if intent.device_id != *device_id {
            return Err(CoordinatorError::DeviceMismatch);
        }
        if now >= intent.expires_at {
            return Err(CoordinatorError::LaunchIntentExpired);
        }

        let epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(CoordinatorError::EpochOverflow)?;
        let idle_expires_at = add_millis(now, self.config.idle_timeout_ms)?;
        let hard_expires_at = add_millis(now, self.config.hard_timeout_ms)?;
        let lease = CoordinatorLease {
            session_id: session_id.clone(),
            device_id: device_id.clone(),
            epoch,
            fencing_token: fencing_token.clone(),
            claimed_at: now,
            last_heartbeat_at: now,
            idle_expires_at,
            hard_expires_at,
        };
        self.next_epoch = epoch;
        self.status = CoordinatorStatus::Active;
        self.recovery_claim_pending = false;
        self.pending_intent = None;
        self.active_lease = Some(lease.clone());
        self.drain_deadline = None;
        Ok(CoordinatorOutcome::LeaseClaimed { lease })
    }

    fn recover_and_issue_launch_intent(
        &mut self,
        launch_intent_id: &LaunchIntentId,
        actor_id: &ActorId,
        device_id: &DeviceId,
        now: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        if !(self.status == CoordinatorStatus::Uncertain
            || (self.status == CoordinatorStatus::Idle && self.recovery_claim_pending))
            || self.active_lease.is_some()
            || self
                .pending_intent
                .as_ref()
                .is_some_and(|intent| intent.expires_at > now)
        {
            return Err(CoordinatorError::CoordinatorUnavailable);
        }
        if expires_at <= now {
            return Err(CoordinatorError::InvalidIntentExpiry);
        }
        // This single state transition is persisted atomically in the DO.
        // The adapter proves historical Claim ownership before calling apply.
        self.pending_intent = Some(PendingLaunchIntent {
            launch_intent_id: launch_intent_id.clone(),
            actor_id: actor_id.clone(),
            device_id: device_id.clone(),
            expires_at,
        });
        self.status = CoordinatorStatus::Idle;
        self.recovery_claim_pending = true;
        self.drain_deadline = None;
        Ok(CoordinatorOutcome::LaunchIntentIssued {
            launch_intent_id: launch_intent_id.clone(),
        })
    }

    fn heartbeat(
        &mut self,
        session_id: &SessionId,
        epoch: u64,
        fencing_token: &FencingToken,
        now: UnixMillis,
        renew_hard: bool,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        if !matches!(
            self.status,
            CoordinatorStatus::Active | CoordinatorStatus::Draining
        ) {
            return Err(CoordinatorError::NoActiveLease);
        }
        let lease = self
            .active_lease
            .as_ref()
            .ok_or(CoordinatorError::NoActiveLease)?;
        if !lease.accepts_writer(session_id, epoch, fencing_token) {
            return Err(CoordinatorError::StaleWriter);
        }
        if now < lease.last_heartbeat_at {
            return Err(CoordinatorError::ReorderedHeartbeat);
        }
        if let Some(outcome) = self.timeout_if_due(now) {
            return Ok(outcome);
        }

        let proposed_idle = add_millis(now, self.config.idle_timeout_ms)?;
        // Old Heartbeat journal events have an immutable hard deadline.
        // Only a NEW, exact-fenced RenewLease event advances it, allowing
        // existing DO journals to replay deterministically after upgrades.
        let proposed_hard = renew_hard
            .then(|| add_millis(now, self.config.hard_timeout_ms))
            .transpose()?;
        let lease = self
            .active_lease
            .as_mut()
            .ok_or(CoordinatorError::NoActiveLease)?;
        lease.last_heartbeat_at = now;
        if let Some(hard) = proposed_hard {
            lease.hard_expires_at = hard;
        }
        lease.idle_expires_at = proposed_idle.min(lease.hard_expires_at);
        Ok(CoordinatorOutcome::HeartbeatAccepted {
            idle_expires_at: lease.idle_expires_at,
        })
    }

    fn release(
        &mut self,
        session_id: &SessionId,
        epoch: u64,
        fencing_token: &FencingToken,
        disposition: ReleaseDisposition,
        now: UnixMillis,
    ) -> Result<CoordinatorOutcome, CoordinatorError> {
        let lease = self
            .active_lease
            .as_ref()
            .ok_or(CoordinatorError::NoActiveLease)?;
        if !lease.accepts_writer(session_id, epoch, fencing_token) {
            return Err(CoordinatorError::StaleWriter);
        }
        if let Some(outcome) = self.timeout_if_due(now) {
            return Ok(outcome);
        }

        self.active_lease = None;
        self.pending_intent = None;
        self.drain_deadline = None;
        self.status = match disposition {
            ReleaseDisposition::Clean => CoordinatorStatus::Idle,
            ReleaseDisposition::Dirty => CoordinatorStatus::Dirty,
            ReleaseDisposition::Uncertain => CoordinatorStatus::Uncertain,
        };
        Ok(CoordinatorOutcome::Released { disposition })
    }

    fn begin_drain(&mut self, now: UnixMillis) -> Result<CoordinatorOutcome, CoordinatorError> {
        if self.status != CoordinatorStatus::Active || self.active_lease.is_none() {
            return Err(CoordinatorError::NoActiveLease);
        }
        if let Some(outcome) = self.timeout_if_due(now) {
            return Ok(outcome);
        }
        let proposed_deadline = add_millis(now, self.config.drain_timeout_ms)?;
        let hard_expires_at = self
            .active_lease
            .as_ref()
            .ok_or(CoordinatorError::NoActiveLease)?
            .hard_expires_at;
        let deadline = proposed_deadline.min(hard_expires_at);
        self.status = CoordinatorStatus::Draining;
        self.drain_deadline = Some(deadline);
        Ok(CoordinatorOutcome::DrainStarted { deadline })
    }

    fn tick(&mut self, now: UnixMillis) -> Result<CoordinatorOutcome, CoordinatorError> {
        if self
            .pending_intent
            .as_ref()
            .is_some_and(|intent| now >= intent.expires_at)
        {
            self.pending_intent = None;
            return Ok(CoordinatorOutcome::LaunchIntentExpired);
        }
        Ok(self
            .timeout_if_due(now)
            .unwrap_or(CoordinatorOutcome::NoChange))
    }

    fn timeout_if_due(&mut self, now: UnixMillis) -> Option<CoordinatorOutcome> {
        let lease = self.active_lease.as_ref()?;
        let kind = if self.status == CoordinatorStatus::Draining
            && self.drain_deadline.is_some_and(|deadline| now >= deadline)
        {
            Some(TimeoutKind::Drain)
        } else if now >= lease.hard_expires_at {
            Some(TimeoutKind::Hard)
        } else if now >= lease.idle_expires_at {
            Some(TimeoutKind::Idle)
        } else {
            None
        }?;

        self.active_lease = None;
        self.pending_intent = None;
        self.drain_deadline = None;
        self.status = CoordinatorStatus::Uncertain;
        Some(CoordinatorOutcome::TimedOut { kind })
    }

    fn mark_recovered(&mut self, _now: UnixMillis) -> Result<CoordinatorOutcome, CoordinatorError> {
        if self.active_lease.is_some() {
            return Err(CoordinatorError::CoordinatorUnavailable);
        }
        if !matches!(
            self.status,
            CoordinatorStatus::Dirty | CoordinatorStatus::Uncertain
        ) {
            return Err(CoordinatorError::RecoveryNotRequired);
        }
        self.status = CoordinatorStatus::Idle;
        self.recovery_claim_pending = false;
        self.pending_intent = None;
        self.drain_deadline = None;
        Ok(CoordinatorOutcome::Recovered)
    }
}

#[must_use]
pub fn coordinator_object_name(profile_id: &ProfileId) -> String {
    let mut name = String::with_capacity(OBJECT_NAME_PREFIX.len() + profile_id.as_str().len());
    name.push_str(OBJECT_NAME_PREFIX);
    name.push_str(profile_id.as_str());
    name
}

fn add_millis(value: UnixMillis, delta: u64) -> Result<UnixMillis, CoordinatorError> {
    value
        .value()
        .checked_add(delta)
        .map(UnixMillis::new)
        .ok_or(CoordinatorError::TimeOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorError {
    InvalidTimeoutConfig,
    InvalidCheckpoint,
    InvalidSequence,
    SequenceOverflow,
    ExpectedVersionMismatch,
    ReorderedCommand,
    ReorderedTime,
    IdempotencyConflict,
    VersionOverflow,
    TimeOverflow,
    EpochOverflow,
    InvalidIntentExpiry,
    CoordinatorUnavailable,
    PendingIntentExists,
    LaunchIntentMissing,
    LaunchIntentMismatch,
    LaunchIntentExpired,
    ActorMismatch,
    DeviceMismatch,
    NoActiveLease,
    StaleWriter,
    ReorderedHeartbeat,
    RecoveryNotRequired,
}

impl fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTimeoutConfig => "coordinator timeout configuration is invalid",
            Self::InvalidCheckpoint => "coordinator replay checkpoint is invalid",
            Self::InvalidSequence => "coordinator command sequence must be positive",
            Self::SequenceOverflow => "coordinator command sequence overflow",
            Self::ExpectedVersionMismatch => "coordinator expected version mismatch",
            Self::ReorderedCommand => "coordinator command is duplicated, delayed or reordered",
            Self::ReorderedTime => "coordinator command time moved backwards",
            Self::IdempotencyConflict => "idempotency key was reused for another command",
            Self::VersionOverflow => "coordinator version overflow",
            Self::TimeOverflow => "coordinator deadline overflow",
            Self::EpochOverflow => "coordinator lease epoch overflow",
            Self::InvalidIntentExpiry => "launch intent expiry must be after issue time",
            Self::CoordinatorUnavailable => "coordinator is not available for a new launch",
            Self::PendingIntentExists => "a live launch intent already exists",
            Self::LaunchIntentMissing => "launch intent is missing",
            Self::LaunchIntentMismatch => "launch intent does not match",
            Self::LaunchIntentExpired => "launch intent has expired",
            Self::ActorMismatch => "launch intent actor does not match",
            Self::DeviceMismatch => "launch intent device does not match",
            Self::NoActiveLease => "no active coordinator lease exists",
            Self::StaleWriter => "lease epoch or fencing token is stale",
            Self::ReorderedHeartbeat => "heartbeat time is older than the accepted heartbeat",
            Self::RecoveryNotRequired => "coordinator does not require recovery",
        })
    }
}

impl std::error::Error for CoordinatorError {}

#[cfg(test)]
mod tests {
    use super::{
        CoordinatorCommand, CoordinatorCommandEnvelope, CoordinatorConfig, CoordinatorError,
        CoordinatorOutcome, CoordinatorStatus, ProfileCoordinatorState, ReleaseDisposition,
        TimeoutKind, coordinator_object_name,
    };
    use profile_platform_primitives::{
        ActorId, AggregateVersion, DeviceId, FencingToken, IdempotencyKey, LaunchIntentId,
        ProfileId, SessionId, TenantId, UnixMillis,
    };

    fn coordinator() -> Result<ProfileCoordinatorState, Box<dyn std::error::Error>> {
        coordinator_with_config(CoordinatorConfig::new(10, 100, 20)?)
    }

    fn coordinator_with_config(
        config: CoordinatorConfig,
    ) -> Result<ProfileCoordinatorState, Box<dyn std::error::Error>> {
        Ok(ProfileCoordinatorState::new(
            TenantId::parse("tenant_01JCOORDINATOR")?,
            ProfileId::parse("profile_01JCOORDINATOR")?,
            config,
        ))
    }

    fn envelope(
        key: &str,
        sequence: u64,
        version: u64,
        command: CoordinatorCommand,
    ) -> Result<CoordinatorCommandEnvelope, Box<dyn std::error::Error>> {
        Ok(CoordinatorCommandEnvelope::new(
            IdempotencyKey::parse(key)?,
            sequence,
            AggregateVersion::new(version)?,
            command,
        )?)
    }

    fn issue(
        state: &mut ProfileCoordinatorState,
        sequence: u64,
        version: u64,
        now: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        state.apply(envelope(
            "idem_issue_01JCOORDINATOR",
            sequence,
            version,
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse("intent_01JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_01JCOORDINATOR")?,
                now: UnixMillis::new(now),
                expires_at: UnixMillis::new(now + 50),
            },
        )?)?;
        Ok(())
    }

    fn claim(
        state: &mut ProfileCoordinatorState,
        sequence: u64,
        version: u64,
        now: u64,
        token: &str,
    ) -> Result<super::CoordinatorDecision, Box<dyn std::error::Error>> {
        Ok(state.apply(envelope(
            "idem_claim_01JCOORDINATOR",
            sequence,
            version,
            CoordinatorCommand::Claim {
                launch_intent_id: LaunchIntentId::parse("intent_01JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_01JCOORDINATOR")?,
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                fencing_token: FencingToken::parse(token)?,
                now: UnixMillis::new(now),
            },
        )?)?)
    }

    #[test]
    fn profile_id_maps_to_one_deterministic_object_name() -> Result<(), Box<dyn std::error::Error>>
    {
        let profile_id = ProfileId::parse("profile_01JCOORDINATOR")?;
        assert_eq!(
            coordinator_object_name(&profile_id),
            "profile-coordinator-v1:profile_01JCOORDINATOR"
        );
        Ok(())
    }

    #[test]
    fn version_overflow_does_not_partially_apply_command() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut state = coordinator()?;
        state.version = AggregateVersion::new(u64::MAX)?;
        let result = state.apply(envelope(
            "idem_overflow_01JCOORDINATOR",
            1,
            u64::MAX,
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse("intent_99JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_01JCOORDINATOR")?,
                now: UnixMillis::new(10),
                expires_at: UnixMillis::new(20),
            },
        )?);
        assert_eq!(result, Err(CoordinatorError::VersionOverflow));
        assert_eq!(state.status(), CoordinatorStatus::Idle);
        assert!(state.pending_intent().is_none());
        assert!(state.active_lease().is_none());
        assert_eq!(state.last_sequence(), 0);
        assert_eq!(state.last_observed_at(), UnixMillis::new(0));
        assert!(state.receipts.is_empty());
        Ok(())
    }

    #[test]
    fn first_claim_issues_epoch_and_duplicate_is_idempotent()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        let command = envelope(
            "idem_claim_01JCOORDINATOR",
            2,
            2,
            CoordinatorCommand::Claim {
                launch_intent_id: LaunchIntentId::parse("intent_01JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_01JCOORDINATOR")?,
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                now: UnixMillis::new(11),
            },
        )?;
        let first = state.apply(command.clone())?;
        let duplicate = state.apply(command)?;
        assert_eq!(first, duplicate);
        let CoordinatorOutcome::LeaseClaimed { lease } = first.outcome() else {
            return Err(CoordinatorError::NoActiveLease.into());
        };
        assert_eq!(lease.epoch(), 1);
        assert_eq!(state.status(), CoordinatorStatus::Active);
        Ok(())
    }

    #[test]
    fn delayed_writer_is_rejected_after_turnover() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        state.apply(envelope(
            "idem_release_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::Release {
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                epoch: 1,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                disposition: ReleaseDisposition::Clean,
                now: UnixMillis::new(12),
            },
        )?)?;
        state.apply(envelope(
            "idem_issue_02JCOORDINATOR",
            4,
            4,
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse("intent_02JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_02JCOORDINATOR")?,
                now: UnixMillis::new(13),
                expires_at: UnixMillis::new(50),
            },
        )?)?;
        state.apply(envelope(
            "idem_claim_02JCOORDINATOR",
            5,
            5,
            CoordinatorCommand::Claim {
                launch_intent_id: LaunchIntentId::parse("intent_02JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_02JCOORDINATOR")?,
                session_id: SessionId::parse("session_02JCOORDINATOR")?,
                fencing_token: FencingToken::parse("fence_02JCOORDINATOR")?,
                now: UnixMillis::new(14),
            },
        )?)?;

        let stale = state.apply(envelope(
            "idem_stale_heartbeat_01JCOORDINATOR",
            6,
            6,
            CoordinatorCommand::Heartbeat {
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                epoch: 1,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                now: UnixMillis::new(15),
            },
        )?);
        assert_eq!(stale, Err(CoordinatorError::StaleWriter));
        assert_eq!(
            state.active_lease().map(super::CoordinatorLease::epoch),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn reordered_commands_and_key_reuse_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        let gap = state.apply(envelope(
            "idem_gap_01JCOORDINATOR",
            3,
            2,
            CoordinatorCommand::Tick {
                now: UnixMillis::new(11),
            },
        )?);
        assert_eq!(gap, Err(CoordinatorError::ReorderedCommand));

        let conflict = state.apply(envelope(
            "idem_issue_01JCOORDINATOR",
            1,
            1,
            CoordinatorCommand::Tick {
                now: UnixMillis::new(10),
            },
        )?);
        assert_eq!(conflict, Err(CoordinatorError::IdempotencyConflict));
        Ok(())
    }

    #[test]
    fn idle_timeout_preserves_uncertain_state_until_recovery()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        let timeout = state.apply(envelope(
            "idem_tick_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::Tick {
                now: UnixMillis::new(21),
            },
        )?)?;
        assert_eq!(
            timeout.outcome(),
            &CoordinatorOutcome::TimedOut {
                kind: TimeoutKind::Idle
            }
        );
        assert_eq!(state.status(), CoordinatorStatus::Uncertain);

        let launch_while_uncertain = state.apply(envelope(
            "idem_issue_blocked_01JCOORDINATOR",
            4,
            4,
            CoordinatorCommand::IssueLaunchIntent {
                launch_intent_id: LaunchIntentId::parse("intent_03JCOORDINATOR")?,
                actor_id: ActorId::parse("actor_01JCOORDINATOR")?,
                device_id: DeviceId::parse("device_03JCOORDINATOR")?,
                now: UnixMillis::new(22),
                expires_at: UnixMillis::new(50),
            },
        )?);
        assert_eq!(
            launch_while_uncertain,
            Err(CoordinatorError::CoordinatorUnavailable)
        );
        state.apply(envelope(
            "idem_recover_01JCOORDINATOR",
            4,
            4,
            CoordinatorCommand::MarkRecovered {
                now: UnixMillis::new(22),
            },
        )?)?;
        assert_eq!(state.status(), CoordinatorStatus::Idle);
        Ok(())
    }

    #[test]
    fn late_clean_release_becomes_uncertain() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        let release = state.apply(envelope(
            "idem_late_release_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::Release {
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                epoch: 1,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                disposition: ReleaseDisposition::Clean,
                now: UnixMillis::new(21),
            },
        )?)?;
        assert_eq!(
            release.outcome(),
            &CoordinatorOutcome::TimedOut {
                kind: TimeoutKind::Idle
            }
        );
        assert_eq!(state.status(), CoordinatorStatus::Uncertain);
        Ok(())
    }

    #[test]
    fn drain_timeout_never_reports_a_clean_release() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator()?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        state.apply(envelope(
            "idem_drain_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::BeginDrain {
                now: UnixMillis::new(12),
            },
        )?)?;
        let timeout = state.apply(envelope(
            "idem_tick_drain_01JCOORDINATOR",
            4,
            4,
            CoordinatorCommand::Tick {
                now: UnixMillis::new(32),
            },
        )?)?;
        assert_eq!(
            timeout.outcome(),
            &CoordinatorOutcome::TimedOut {
                kind: TimeoutKind::Drain
            }
        );
        assert_eq!(state.status(), CoordinatorStatus::Uncertain);
        Ok(())
    }

    #[test]
    fn explicit_renewals_extend_hard_deadline_beyond_original_session_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator_with_config(CoordinatorConfig::new(100, 100, 20)?)?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        // The original hard expiry was 111; a timely heartbeat moves it
        // forward instead of closing a healthy browser after a fixed span.
        for (sequence, at) in [(3, 105), (4, 195), (5, 285)] {
            let heartbeat = state.apply(envelope(
                &format!("idem_heartbeat_roll_{sequence}_01JCOORDINATOR"),
                sequence,
                sequence,
                CoordinatorCommand::RenewLease {
                    session_id: SessionId::parse("session_01JCOORDINATOR")?,
                    epoch: 1,
                    fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                    now: UnixMillis::new(at),
                },
            )?)?;
            assert_eq!(
                heartbeat.outcome(),
                &CoordinatorOutcome::HeartbeatAccepted {
                    idle_expires_at: UnixMillis::new(at + 100)
                }
            );
            assert_eq!(
                state.active_lease().map(|lease| lease.hard_expires_at()),
                Some(UnixMillis::new(at + 100))
            );
        }
        assert_eq!(state.status(), CoordinatorStatus::Active);
        Ok(())
    }

    #[test]
    fn legacy_heartbeat_preserves_original_hard_deadline() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut state = coordinator_with_config(CoordinatorConfig::new(100, 100, 20)?)?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        let old = state.apply(envelope(
            "idem_old_heartbeat_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::Heartbeat {
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                epoch: 1,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                now: UnixMillis::new(105),
            },
        )?)?;
        assert_eq!(
            old.outcome(),
            &CoordinatorOutcome::HeartbeatAccepted {
                idle_expires_at: UnixMillis::new(111)
            }
        );
        assert_eq!(
            state.active_lease().map(|lease| lease.hard_expires_at()),
            Some(UnixMillis::new(111))
        );
        Ok(())
    }

    #[test]
    fn expired_heartbeat_never_renews_hard_deadline() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = coordinator_with_config(CoordinatorConfig::new(100, 100, 20)?)?;
        issue(&mut state, 1, 1, 10)?;
        claim(&mut state, 2, 2, 11, "fence_01JCOORDINATOR")?;
        let late = state.apply(envelope(
            "idem_heartbeat_too_late_01JCOORDINATOR",
            3,
            3,
            CoordinatorCommand::Heartbeat {
                session_id: SessionId::parse("session_01JCOORDINATOR")?,
                epoch: 1,
                fencing_token: FencingToken::parse("fence_01JCOORDINATOR")?,
                now: UnixMillis::new(111),
            },
        )?)?;
        assert_eq!(
            late.outcome(),
            &CoordinatorOutcome::TimedOut {
                kind: TimeoutKind::Hard
            }
        );
        assert_eq!(state.status(), CoordinatorStatus::Uncertain);
        assert!(state.active_lease().is_none());
        Ok(())
    }
}
