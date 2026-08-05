#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{DeviceId, SessionId, UnixMillis};

const MIN_SECRET_LENGTH: usize = 24;
const MAX_SECRET_LENGTH: usize = 96;
const MAX_IPC_FRAME_LENGTH: usize = 512;
pub const CAMOUHOST_IPC_VERSION: u16 = 1;

#[derive(Clone, Eq, PartialEq)]
pub struct ClaimCode(String);

impl ClaimCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, ClaimCodeError> {
        let value = value.into();
        if !valid_secret(&value) {
            return Err(ClaimCodeError);
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for ClaimCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClaimCode([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimCodeError;

impl fmt::Display for ClaimCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("claim code must be a bounded opaque ASCII token")
    }
}

impl std::error::Error for ClaimCodeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimUri {
    claim_code: ClaimCode,
}

impl ClaimUri {
    pub fn parse(value: &str) -> Result<Self, ClaimUriError> {
        if value.len() > 160 || value.contains(['?', '#', '\\', '%']) {
            return Err(ClaimUriError::InvalidShape);
        }
        let claim = value
            .strip_prefix("profilebridge://claim/")
            .ok_or(ClaimUriError::InvalidShape)?;
        if claim.is_empty() || claim.contains('/') || claim.contains('.') {
            return Err(ClaimUriError::InvalidShape);
        }
        Ok(Self {
            claim_code: ClaimCode::parse(claim.to_owned())
                .map_err(|_| ClaimUriError::InvalidClaimCode)?,
        })
    }

    #[must_use]
    pub const fn claim_code(&self) -> &ClaimCode {
        &self.claim_code
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimUriError {
    InvalidShape,
    InvalidClaimCode,
}

impl fmt::Display for ClaimUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidShape => "custom URI does not match the exact claim route",
            Self::InvalidClaimCode => "custom URI contains an invalid claim token",
        })
    }
}

impl std::error::Error for ClaimUriError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrollmentClaim {
    claim_code: ClaimCode,
    expires_at: UnixMillis,
    redemption: Option<ClaimRedemption>,
}

impl EnrollmentClaim {
    pub fn issue(
        claim_code: ClaimCode,
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Self, EnrollmentClaimError> {
        if expires_at <= issued_at {
            return Err(EnrollmentClaimError::InvalidExpiry);
        }
        Ok(Self {
            claim_code,
            expires_at,
            redemption: None,
        })
    }

    pub fn redeem(
        &mut self,
        presented_code: &ClaimCode,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<ClaimRedemption, EnrollmentClaimError> {
        if let Some(redemption) = &self.redemption {
            if redemption.device_id() == device_id {
                return Err(EnrollmentClaimError::ReplayRejected);
            }
            return Err(EnrollmentClaimError::DeviceRebindRejected);
        }
        if now >= self.expires_at {
            return Err(EnrollmentClaimError::Expired);
        }
        if presented_code != &self.claim_code {
            return Err(EnrollmentClaimError::CodeMismatch);
        }

        let redemption = ClaimRedemption {
            device_id: device_id.clone(),
            redeemed_at: now,
        };
        self.redemption = Some(redemption.clone());
        Ok(redemption)
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn redemption(&self) -> Option<&ClaimRedemption> {
        self.redemption.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRedemption {
    device_id: DeviceId,
    redeemed_at: UnixMillis,
}

impl ClaimRedemption {
    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn redeemed_at(&self) -> UnixMillis {
        self.redeemed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnrollmentClaimError {
    InvalidExpiry,
    Expired,
    CodeMismatch,
    ReplayRejected,
    DeviceRebindRejected,
}

impl fmt::Display for EnrollmentClaimError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidExpiry => "claim expiry must be after issue time",
            Self::Expired => "claim has expired",
            Self::CodeMismatch => "claim code does not match",
            Self::ReplayRejected => "claim replay rejected",
            Self::DeviceRebindRejected => "claim cannot be rebound to another device",
        })
    }
}

impl std::error::Error for EnrollmentClaimError {}

#[derive(Clone, Eq, PartialEq)]
pub struct WorkspaceLockToken(String);

impl WorkspaceLockToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, WorkspaceLockTokenError> {
        let value = value.into();
        if !valid_secret(&value) {
            return Err(WorkspaceLockTokenError);
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for WorkspaceLockToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WorkspaceLockToken([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceLockTokenError;

impl fmt::Display for WorkspaceLockTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace lock token must be a bounded opaque ASCII token")
    }
}

impl std::error::Error for WorkspaceLockTokenError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceLease {
    writer_device_id: DeviceId,
    lock_token: WorkspaceLockToken,
    epoch: u64,
}

impl WorkspaceLease {
    #[must_use]
    pub const fn writer_device_id(&self) -> &DeviceId {
        &self.writer_device_id
    }

    #[must_use]
    pub const fn lock_token(&self) -> &WorkspaceLockToken {
        &self.lock_token
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceLockState {
    next_epoch: u64,
    active: Option<WorkspaceLease>,
}

impl WorkspaceLockState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_epoch: 0,
            active: None,
        }
    }

    pub fn acquire(
        &mut self,
        writer_device_id: &DeviceId,
        lock_token: &WorkspaceLockToken,
    ) -> Result<WorkspaceLease, WorkspaceLockError> {
        if let Some(active) = &self.active {
            if active.writer_device_id == *writer_device_id && active.lock_token == *lock_token {
                return Ok(active.clone());
            }
            return Err(WorkspaceLockError::WriterAlreadyActive);
        }
        let epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(WorkspaceLockError::EpochOverflow)?;
        let lease = WorkspaceLease {
            writer_device_id: writer_device_id.clone(),
            lock_token: lock_token.clone(),
            epoch,
        };
        self.next_epoch = epoch;
        self.active = Some(lease.clone());
        Ok(lease)
    }

    pub fn release(
        &mut self,
        writer_device_id: &DeviceId,
        epoch: u64,
        lock_token: &WorkspaceLockToken,
    ) -> Result<(), WorkspaceLockError> {
        let active = self
            .active
            .as_ref()
            .ok_or(WorkspaceLockError::NoActiveWriter)?;
        if active.writer_device_id != *writer_device_id
            || active.epoch != epoch
            || active.lock_token != *lock_token
        {
            return Err(WorkspaceLockError::StaleWriter);
        }
        self.active = None;
        Ok(())
    }

    #[must_use]
    pub const fn active(&self) -> Option<&WorkspaceLease> {
        self.active.as_ref()
    }
}

impl Default for WorkspaceLockState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceLockError {
    WriterAlreadyActive,
    NoActiveWriter,
    StaleWriter,
    EpochOverflow,
}

impl fmt::Display for WorkspaceLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WriterAlreadyActive => "another local writer already owns the workspace",
            Self::NoActiveWriter => "workspace has no active writer",
            Self::StaleWriter => "workspace release came from a stale writer",
            Self::EpochOverflow => "workspace writer epoch overflow",
        })
    }
}

impl std::error::Error for WorkspaceLockError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessCloseOutcome {
    Clean,
    Crash,
    ForcedTimeout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisedProcessState {
    Idle,
    Starting {
        session_id: SessionId,
        start_deadline: UnixMillis,
    },
    Ready {
        session_id: SessionId,
    },
    Closing {
        session_id: SessionId,
        close_deadline: UnixMillis,
    },
    Closed {
        session_id: SessionId,
        outcome: ProcessCloseOutcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSupervisor {
    state: SupervisedProcessState,
}

impl ProcessSupervisor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SupervisedProcessState::Idle,
        }
    }

    pub fn begin_start(
        &mut self,
        session_id: SessionId,
        now: UnixMillis,
        start_timeout_ms: u64,
    ) -> Result<(), ProcessSupervisorError> {
        if !matches!(self.state, SupervisedProcessState::Idle) || start_timeout_ms == 0 {
            return Err(ProcessSupervisorError::InvalidTransition);
        }
        let start_deadline = add_millis(now, start_timeout_ms)?;
        self.state = SupervisedProcessState::Starting {
            session_id,
            start_deadline,
        };
        Ok(())
    }

    pub fn mark_ready(
        &mut self,
        session_id: &SessionId,
        now: UnixMillis,
    ) -> Result<(), ProcessSupervisorError> {
        let SupervisedProcessState::Starting {
            session_id: active,
            start_deadline,
        } = &self.state
        else {
            return Err(ProcessSupervisorError::InvalidTransition);
        };
        if active != session_id {
            return Err(ProcessSupervisorError::SessionMismatch);
        }
        if now >= *start_deadline {
            self.state = SupervisedProcessState::Closed {
                session_id: session_id.clone(),
                outcome: ProcessCloseOutcome::ForcedTimeout,
            };
            return Err(ProcessSupervisorError::DeadlineExpired);
        }
        self.state = SupervisedProcessState::Ready {
            session_id: session_id.clone(),
        };
        Ok(())
    }

    pub fn request_close(
        &mut self,
        session_id: &SessionId,
        now: UnixMillis,
        close_timeout_ms: u64,
    ) -> Result<(), ProcessSupervisorError> {
        let SupervisedProcessState::Ready { session_id: active } = &self.state else {
            return Err(ProcessSupervisorError::InvalidTransition);
        };
        if active != session_id {
            return Err(ProcessSupervisorError::SessionMismatch);
        }
        if close_timeout_ms == 0 {
            return Err(ProcessSupervisorError::InvalidTransition);
        }
        self.state = SupervisedProcessState::Closing {
            session_id: session_id.clone(),
            close_deadline: add_millis(now, close_timeout_ms)?,
        };
        Ok(())
    }

    pub fn observe_exit(
        &mut self,
        session_id: &SessionId,
        successful_exit: bool,
    ) -> Result<ProcessCloseOutcome, ProcessSupervisorError> {
        let outcome = match &self.state {
            SupervisedProcessState::Closing { session_id: active, .. } => {
                if active != session_id {
                    return Err(ProcessSupervisorError::SessionMismatch);
                }
                if successful_exit {
                    ProcessCloseOutcome::Clean
                } else {
                    ProcessCloseOutcome::Crash
                }
            }
            SupervisedProcessState::Starting { session_id: active, .. }
            | SupervisedProcessState::Ready { session_id: active } => {
                if active != session_id {
                    return Err(ProcessSupervisorError::SessionMismatch);
                }
                ProcessCloseOutcome::Crash
            }
            SupervisedProcessState::Idle | SupervisedProcessState::Closed { .. } => {
                return Err(ProcessSupervisorError::InvalidTransition);
            }
        };
        self.state = SupervisedProcessState::Closed {
            session_id: session_id.clone(),
            outcome,
        };
        Ok(outcome)
    }

    pub fn tick(
        &mut self,
        now: UnixMillis,
    ) -> Result<Option<ProcessCloseOutcome>, ProcessSupervisorError> {
        let timed_out_session = match &self.state {
            SupervisedProcessState::Starting {
                session_id,
                start_deadline,
            } if now >= *start_deadline => Some(session_id.clone()),
            SupervisedProcessState::Closing {
                session_id,
                close_deadline,
            } if now >= *close_deadline => Some(session_id.clone()),
            _ => None,
        };
        let Some(session_id) = timed_out_session else {
            return Ok(None);
        };
        self.state = SupervisedProcessState::Closed {
            session_id,
            outcome: ProcessCloseOutcome::ForcedTimeout,
        };
        Ok(Some(ProcessCloseOutcome::ForcedTimeout))
    }

    #[must_use]
    pub const fn state(&self) -> &SupervisedProcessState {
        &self.state
    }
}

impl Default for ProcessSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessSupervisorError {
    InvalidTransition,
    SessionMismatch,
    DeadlineExpired,
    TimeOverflow,
}

impl fmt::Display for ProcessSupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTransition => "process supervisor transition is invalid",
            Self::SessionMismatch => "process supervisor session mismatch",
            Self::DeadlineExpired => "process supervisor deadline expired",
            Self::TimeOverflow => "process supervisor deadline overflow",
        })
    }
}

impl std::error::Error for ProcessSupervisorError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CamouhostMessage {
    Hello { version: u16 },
    HelloAck { version: u16 },
    Launch { session_id: SessionId },
    Ready { session_id: SessionId },
    Close { session_id: SessionId },
    Closed {
        session_id: SessionId,
        clean: bool,
    },
}

impl CamouhostMessage {
    pub fn parse(frame: &str) -> Result<Self, CamouhostProtocolError> {
        if frame.is_empty()
            || frame.len() > MAX_IPC_FRAME_LENGTH
            || frame.contains(['\n', '\r', '\0'])
        {
            return Err(CamouhostProtocolError::MalformedFrame);
        }
        let parts: Vec<&str> = frame.split('|').collect();
        match parts.as_slice() {
            ["hello", version] => Ok(Self::Hello {
                version: parse_version(version)?,
            }),
            ["hello_ack", version] => Ok(Self::HelloAck {
                version: parse_version(version)?,
            }),
            ["launch", session_id] => Ok(Self::Launch {
                session_id: SessionId::parse((*session_id).to_owned())
                    .map_err(|_| CamouhostProtocolError::MalformedFrame)?,
            }),
            ["ready", session_id] => Ok(Self::Ready {
                session_id: SessionId::parse((*session_id).to_owned())
                    .map_err(|_| CamouhostProtocolError::MalformedFrame)?,
            }),
            ["close", session_id] => Ok(Self::Close {
                session_id: SessionId::parse((*session_id).to_owned())
                    .map_err(|_| CamouhostProtocolError::MalformedFrame)?,
            }),
            ["closed", session_id, clean] => Ok(Self::Closed {
                session_id: SessionId::parse((*session_id).to_owned())
                    .map_err(|_| CamouhostProtocolError::MalformedFrame)?,
                clean: parse_bool(clean)?,
            }),
            _ => Err(CamouhostProtocolError::MalformedFrame),
        }
    }

    pub fn validate_version(&self) -> Result<(), CamouhostProtocolError> {
        match self {
            Self::Hello { version } | Self::HelloAck { version }
                if *version != CAMOUHOST_IPC_VERSION =>
            {
                Err(CamouhostProtocolError::UnsupportedVersion)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CamouhostProtocolError {
    MalformedFrame,
    UnsupportedVersion,
}

impl fmt::Display for CamouhostProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MalformedFrame => "Camouhost IPC frame is malformed",
            Self::UnsupportedVersion => "Camouhost IPC version is unsupported",
        })
    }
}

impl std::error::Error for CamouhostProtocolError {}

pub trait DeviceIdentityPort {
    fn device_id(&self) -> Result<DeviceId, BridgePortError>;
}

pub trait DeviceKeyPort {
    fn ensure_key_handle(&mut self, device_id: &DeviceId) -> Result<String, BridgePortError>;
}

pub trait CamouhostPort {
    fn exchange(&mut self, message: &CamouhostMessage)
    -> Result<CamouhostMessage, BridgePortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgePortError {
    Unavailable,
    InvalidResponse,
}

impl fmt::Display for BridgePortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Bridge port is unavailable",
            Self::InvalidResponse => "Bridge port returned an invalid response",
        })
    }
}

impl std::error::Error for BridgePortError {}

fn valid_secret(value: &str) -> bool {
    (MIN_SECRET_LENGTH..=MAX_SECRET_LENGTH).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn add_millis(value: UnixMillis, delta: u64) -> Result<UnixMillis, ProcessSupervisorError> {
    value
        .value()
        .checked_add(delta)
        .map(UnixMillis::new)
        .ok_or(ProcessSupervisorError::TimeOverflow)
}

fn parse_version(value: &str) -> Result<u16, CamouhostProtocolError> {
    value
        .parse::<u16>()
        .map_err(|_| CamouhostProtocolError::MalformedFrame)
}

fn parse_bool(value: &str) -> Result<bool, CamouhostProtocolError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(CamouhostProtocolError::MalformedFrame),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostProtocolError, ClaimCode, ClaimUri,
        EnrollmentClaim, EnrollmentClaimError, ProcessCloseOutcome, ProcessSupervisor,
        SupervisedProcessState, WorkspaceLockError, WorkspaceLockState, WorkspaceLockToken,
    };
    use profile_platform_primitives::{DeviceId, SessionId, UnixMillis};

    fn claim_code() -> Result<ClaimCode, Box<dyn std::error::Error>> {
        Ok(ClaimCode::parse("claim_01JBRIDGE_FEASIBILITY")?)
    }

    #[test]
    fn exact_claim_uri_is_accepted_without_exposing_secret_debug(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let uri = ClaimUri::parse("profilebridge://claim/claim_01JBRIDGE_FEASIBILITY")?;
        assert_eq!(format!("{:?}", uri.claim_code()), "ClaimCode([REDACTED])");
        Ok(())
    }

    #[test]
    fn malformed_claim_uris_fail_closed() {
        let invalid = [
            "PROFILEBRIDGE://claim/claim_01JBRIDGE_FEASIBILITY",
            "profilebridge://other/claim_01JBRIDGE_FEASIBILITY",
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY/extra",
            "profilebridge://claim/../../claim_01JBRIDGE_FEASIBILITY",
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY?copy=true",
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY#fragment",
            "profilebridge://claim/claim%5F01JBRIDGE_FEASIBILITY",
        ];
        for value in invalid {
            assert!(ClaimUri::parse(value).is_err(), "unexpected valid URI: {value}");
        }
    }

    #[test]
    fn claim_is_single_use_and_device_bound() -> Result<(), Box<dyn std::error::Error>> {
        let code = claim_code()?;
        let mut claim = EnrollmentClaim::issue(
            code.clone(),
            UnixMillis::new(10),
            UnixMillis::new(100),
        )?;
        let first_device = DeviceId::parse("device_01JBRIDGE")?;
        let second_device = DeviceId::parse("device_02JBRIDGE")?;
        claim.redeem(&code, &first_device, UnixMillis::new(20))?;
        assert_eq!(
            claim.redeem(&code, &first_device, UnixMillis::new(21)),
            Err(EnrollmentClaimError::ReplayRejected)
        );
        assert_eq!(
            claim.redeem(&code, &second_device, UnixMillis::new(21)),
            Err(EnrollmentClaimError::DeviceRebindRejected)
        );
        Ok(())
    }

    #[test]
    fn claim_expiry_is_strict_at_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let code = claim_code()?;
        let mut claim = EnrollmentClaim::issue(
            code.clone(),
            UnixMillis::new(10),
            UnixMillis::new(100),
        )?;
        assert_eq!(
            claim.redeem(
                &code,
                &DeviceId::parse("device_01JBRIDGE")?,
                UnixMillis::new(100),
            ),
            Err(EnrollmentClaimError::Expired)
        );
        Ok(())
    }

    #[test]
    fn second_workspace_writer_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let first_device = DeviceId::parse("device_01JBRIDGE")?;
        let second_device = DeviceId::parse("device_02JBRIDGE")?;
        let first_token = WorkspaceLockToken::parse("lock_01JBRIDGE_FEASIBILITY")?;
        let second_token = WorkspaceLockToken::parse("lock_02JBRIDGE_FEASIBILITY")?;
        let mut state = WorkspaceLockState::new();
        let lease = state.acquire(&first_device, &first_token)?;
        assert_eq!(
            state.acquire(&second_device, &second_token),
            Err(WorkspaceLockError::WriterAlreadyActive)
        );
        assert_eq!(state.acquire(&first_device, &first_token)?, lease);
        Ok(())
    }

    #[test]
    fn stale_workspace_release_cannot_unlock_new_writer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let device = DeviceId::parse("device_01JBRIDGE")?;
        let first_token = WorkspaceLockToken::parse("lock_01JBRIDGE_FEASIBILITY")?;
        let second_token = WorkspaceLockToken::parse("lock_02JBRIDGE_FEASIBILITY")?;
        let mut state = WorkspaceLockState::new();
        let first = state.acquire(&device, &first_token)?;
        state.release(&device, first.epoch(), &first_token)?;
        let second = state.acquire(&device, &second_token)?;
        assert_eq!(
            state.release(&device, first.epoch(), &first_token),
            Err(WorkspaceLockError::StaleWriter)
        );
        assert_eq!(state.active(), Some(&second));
        Ok(())
    }

    #[test]
    fn graceful_close_and_forced_timeout_are_distinct(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JBRIDGE")?;
        let mut graceful = ProcessSupervisor::new();
        graceful.begin_start(session_id.clone(), UnixMillis::new(10), 20)?;
        graceful.mark_ready(&session_id, UnixMillis::new(11))?;
        graceful.request_close(&session_id, UnixMillis::new(12), 20)?;
        assert_eq!(
            graceful.observe_exit(&session_id, true)?,
            ProcessCloseOutcome::Clean
        );

        let mut timed_out = ProcessSupervisor::new();
        timed_out.begin_start(session_id.clone(), UnixMillis::new(10), 20)?;
        timed_out.mark_ready(&session_id, UnixMillis::new(11))?;
        timed_out.request_close(&session_id, UnixMillis::new(12), 20)?;
        assert_eq!(
            timed_out.tick(UnixMillis::new(32))?,
            Some(ProcessCloseOutcome::ForcedTimeout)
        );
        assert_eq!(
            timed_out.state(),
            &SupervisedProcessState::Closed {
                session_id,
                outcome: ProcessCloseOutcome::ForcedTimeout,
            }
        );
        Ok(())
    }

    #[test]
    fn unexpected_successful_exit_is_still_a_crash(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let session_id = SessionId::parse("session_01JBRIDGE")?;
        let mut supervisor = ProcessSupervisor::new();
        supervisor.begin_start(session_id.clone(), UnixMillis::new(10), 20)?;
        supervisor.mark_ready(&session_id, UnixMillis::new(11))?;
        assert_eq!(
            supervisor.observe_exit(&session_id, true)?,
            ProcessCloseOutcome::Crash
        );
        Ok(())
    }

    #[test]
    fn versioned_camouhost_frames_parse_and_malformed_frames_fail_closed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let hello = CamouhostMessage::parse("hello|1")?;
        hello.validate_version()?;
        assert_eq!(
            hello,
            CamouhostMessage::Hello {
                version: CAMOUHOST_IPC_VERSION
            }
        );
        assert_eq!(
            CamouhostMessage::parse("hello|2")?.validate_version(),
            Err(CamouhostProtocolError::UnsupportedVersion)
        );
        assert_eq!(
            CamouhostMessage::parse("closed|session_01JBRIDGE|maybe"),
            Err(CamouhostProtocolError::MalformedFrame)
        );
        assert_eq!(
            CamouhostMessage::parse("launch|../../profile"),
            Err(CamouhostProtocolError::MalformedFrame)
        );
        Ok(())
    }
}
