#![forbid(unsafe_code)]

use profile_platform_primitives::{
    DeviceId, GenerationId, LaunchIntentId, ProfileId, SessionId, TenantId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgePortError {
    Unavailable,
    InvalidResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeStatus {
    Ready,
    ClaimRedeemed,
    Launching,
    Open,
    Closing,
    Saved,
    Cancelled,
    Denied,
    Failed,
}

impl BridgeStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::ClaimRedeemed => "CLAIM_REDEEMED",
            Self::Launching => "LAUNCHING",
            Self::Open => "OPEN",
            Self::Closing => "CLOSING",
            Self::Saved => "SAVED",
            Self::Cancelled => "CANCELLED",
            Self::Denied => "DENIED",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchDenialReason {
    Unauthorized,
    ClaimExpired,
    ClaimReplay,
    DeviceUntrusted,
    ProfileBusy,
    GenerationUnavailable,
    RuntimeUnavailable,
    RecoveryRequired,
}

impl LaunchDenialReason {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::ClaimExpired => "CLAIM_EXPIRED",
            Self::ClaimReplay => "CLAIM_REPLAY",
            Self::DeviceUntrusted => "DEVICE_UNTRUSTED",
            Self::ProfileBusy => "PROFILE_BUSY",
            Self::GenerationUnavailable => "GENERATION_UNAVAILABLE",
            Self::RuntimeUnavailable => "RUNTIME_UNAVAILABLE",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchClaim {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    launch_intent_id: LaunchIntentId,
    device_id: DeviceId,
    expires_at_unix_seconds: u64,
}

impl LaunchClaim {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        launch_intent_id: LaunchIntentId,
        device_id: DeviceId,
        expires_at_unix_seconds: u64,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            launch_intent_id,
            device_id,
            expires_at_unix_seconds,
        }
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
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn launch_intent_id(&self) -> &LaunchIntentId {
        &self.launch_intent_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }

    #[must_use]
    pub const fn is_expired_at(&self, unix_seconds: u64) -> bool {
        unix_seconds >= self.expires_at_unix_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchAuthorization {
    claim: LaunchClaim,
}

impl LaunchAuthorization {
    #[must_use]
    pub const fn new(claim: LaunchClaim) -> Self {
        Self { claim }
    }

    #[must_use]
    pub const fn claim(&self) -> &LaunchClaim {
        &self.claim
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimRedemptionDecision {
    Redeemed,
    Denied(LaunchDenialReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionCloseDecision {
    Save,
    Cancel,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeSession {
    session_id: SessionId,
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    launch_intent_id: LaunchIntentId,
    device_id: DeviceId,
    status: BridgeStatus,
}

impl BridgeSession {
    #[must_use]
    pub const fn new(session_id: SessionId, authorization: &LaunchAuthorization) -> Self {
        let claim = authorization.claim();
        Self {
            session_id,
            tenant_id: claim.tenant_id().clone(),
            profile_id: claim.profile_id().clone(),
            generation_id: claim.generation_id().clone(),
            launch_intent_id: claim.launch_intent_id().clone(),
            device_id: claim.device_id().clone(),
            status: BridgeStatus::ClaimRedeemed,
        }
    }

    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
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
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn launch_intent_id(&self) -> &LaunchIntentId {
        &self.launch_intent_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn status(&self) -> &BridgeStatus {
        &self.status
    }

    pub fn transition_to_launching(&mut self) -> Result<(), BridgeStateError> {
        if self.status != BridgeStatus::ClaimRedeemed {
            return Err(BridgeStateError::InvalidTransition);
        }
        self.status = BridgeStatus::Launching;
        Ok(())
    }

    pub fn transition_to_open(&mut self) -> Result<(), BridgeStateError> {
        if self.status != BridgeStatus::Launching {
            return Err(BridgeStateError::InvalidTransition);
        }
        self.status = BridgeStatus::Open;
        Ok(())
    }

    pub fn transition_to_closing(&mut self) -> Result<(), BridgeStateError> {
        if self.status != BridgeStatus::Open {
            return Err(BridgeStateError::InvalidTransition);
        }
        self.status = BridgeStatus::Closing;
        Ok(())
    }

    pub fn complete_close(
        &mut self,
        decision: SessionCloseDecision,
    ) -> Result<(), BridgeStateError> {
        if self.status != BridgeStatus::Closing {
            return Err(BridgeStateError::InvalidTransition);
        }
        self.status = match decision {
            SessionCloseDecision::Save => BridgeStatus::Saved,
            SessionCloseDecision::Cancel => BridgeStatus::Cancelled,
            SessionCloseDecision::Fail => BridgeStatus::Failed,
        };
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeStateError {
    InvalidTransition,
}

impl core::fmt::Display for BridgeStateError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTransition => "bridge session transition is invalid",
        })
    }
}

impl std::error::Error for BridgeStateError {}

pub trait ClaimRedemptionPort {
    fn redeem(
        &mut self,
        raw_claim: &str,
        device_id: &DeviceId,
    ) -> Result<LaunchAuthorization, LaunchDenialReason>;
}

pub trait AuditPort {
    fn record_status(&mut self, session: &BridgeSession) -> Result<(), BridgePortError>;
}

pub const CAMOUHOST_IPC_VERSION: u16 = 3;
const MAX_IPC_FRAME_LENGTH: usize = 1_100_000;
const MAX_BROWSER_VISIBLE_PAYLOAD_HEX_LENGTH: usize = 512 * 1024 * 2;
const MAX_NAVIGATION_TARGET_HEX_LENGTH: usize = 2048 * 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CamouhostErrorKind {
    Protocol,
    Identity,
    Runtime,
}

impl CamouhostErrorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Identity => "identity",
            Self::Runtime => "runtime",
        }
    }

    fn parse(value: &str) -> Result<Self, CamouhostProtocolError> {
        match value {
            "protocol" => Ok(Self::Protocol),
            "identity" => Ok(Self::Identity),
            "runtime" => Ok(Self::Runtime),
            _ => Err(CamouhostProtocolError::MalformedFrame),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CamouhostMessage {
    Hello {
        version: u16,
    },
    HelloAck {
        version: u16,
    },
    Launch {
        session_id: SessionId,
    },
    Ready {
        session_id: SessionId,
    },
    ObserveBrowserVisible {
        session_id: SessionId,
    },
    BrowserVisible {
        session_id: SessionId,
        payload_hex: String,
    },
    AdmitNavigation {
        session_id: SessionId,
        target_hex: String,
    },
    NavigationAdmitted {
        session_id: SessionId,
    },
    ObserveClose {
        session_id: SessionId,
    },
    CloseObserved {
        session_id: SessionId,
        controlled: bool,
    },
    Close {
        session_id: SessionId,
    },
    Closed {
        session_id: SessionId,
        clean: bool,
    },
    Error {
        kind: CamouhostErrorKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CamouhostProtocolError {
    MalformedFrame,
    UnsupportedVersion,
}

impl CamouhostMessage {
    pub fn parse(value: &str) -> Result<Self, CamouhostProtocolError> {
        if value.is_empty()
            || value.len() > MAX_IPC_FRAME_LENGTH
            || value.contains(['\r', '\n', '\0'])
        {
            return Err(CamouhostProtocolError::MalformedFrame);
        }
        let parts: Vec<_> = value.split('|').collect();
        match parts.as_slice() {
            ["hello", version] => Ok(Self::Hello {
                version: parse_version(version)?,
            }),
            ["hello_ack", version] => Ok(Self::HelloAck {
                version: parse_version(version)?,
            }),
            ["launch", session] => Ok(Self::Launch {
                session_id: parse_session(session)?,
            }),
            ["ready", session] => Ok(Self::Ready {
                session_id: parse_session(session)?,
            }),
            ["observe_browser_visible", session] => Ok(Self::ObserveBrowserVisible {
                session_id: parse_session(session)?,
            }),
            ["browser_visible", session, payload_hex]
                if valid_lower_hex(payload_hex, false, MAX_BROWSER_VISIBLE_PAYLOAD_HEX_LENGTH) =>
            {
                Ok(Self::BrowserVisible {
                    session_id: parse_session(session)?,
                    payload_hex: (*payload_hex).to_owned(),
                })
            }
            ["admit_navigation", session, target_hex]
                if valid_lower_hex(target_hex, true, MAX_NAVIGATION_TARGET_HEX_LENGTH) =>
            {
                Ok(Self::AdmitNavigation {
                    session_id: parse_session(session)?,
                    target_hex: (*target_hex).to_owned(),
                })
            }
            ["navigation_admitted", session] => Ok(Self::NavigationAdmitted {
                session_id: parse_session(session)?,
            }),
            ["observe_close", session] => Ok(Self::ObserveClose {
                session_id: parse_session(session)?,
            }),
            ["close_observed", session, controlled] => Ok(Self::CloseObserved {
                session_id: parse_session(session)?,
                controlled: parse_bool(controlled)?,
            }),
            ["close", session] => Ok(Self::Close {
                session_id: parse_session(session)?,
            }),
            ["closed", session, clean] => Ok(Self::Closed {
                session_id: parse_session(session)?,
                clean: parse_bool(clean)?,
            }),
            ["error", kind] => Ok(Self::Error {
                kind: CamouhostErrorKind::parse(kind)?,
            }),
            _ => Err(CamouhostProtocolError::MalformedFrame),
        }
    }

    pub fn to_frame(&self) -> Result<String, CamouhostProtocolError> {
        self.validate_version()?;
        let frame = match self {
            Self::Hello { version } => format!("hello|{version}"),
            Self::HelloAck { version } => format!("hello_ack|{version}"),
            Self::Launch { session_id } => format!("launch|{}", session_id.as_str()),
            Self::Ready { session_id } => format!("ready|{}", session_id.as_str()),
            Self::ObserveBrowserVisible { session_id } => {
                format!("observe_browser_visible|{}", session_id.as_str())
            }
            Self::BrowserVisible {
                session_id,
                payload_hex,
            } => {
                if !valid_lower_hex(payload_hex, false, MAX_BROWSER_VISIBLE_PAYLOAD_HEX_LENGTH) {
                    return Err(CamouhostProtocolError::MalformedFrame);
                }
                format!("browser_visible|{}|{payload_hex}", session_id.as_str())
            }
            Self::AdmitNavigation {
                session_id,
                target_hex,
            } => {
                if !valid_lower_hex(target_hex, true, MAX_NAVIGATION_TARGET_HEX_LENGTH) {
                    return Err(CamouhostProtocolError::MalformedFrame);
                }
                format!("admit_navigation|{}|{target_hex}", session_id.as_str())
            }
            Self::NavigationAdmitted { session_id } => {
                format!("navigation_admitted|{}", session_id.as_str())
            }
            Self::ObserveClose { session_id } => format!("observe_close|{}", session_id.as_str()),
            Self::CloseObserved {
                session_id,
                controlled,
            } => format!("close_observed|{}|{controlled}", session_id.as_str()),
            Self::Close { session_id } => format!("close|{}", session_id.as_str()),
            Self::Closed { session_id, clean } => {
                format!("closed|{}|{clean}", session_id.as_str())
            }
            Self::Error { kind } => format!("error|{}", kind.as_str()),
        };
        if frame.len() > MAX_IPC_FRAME_LENGTH {
            return Err(CamouhostProtocolError::MalformedFrame);
        }
        Ok(frame)
    }

    pub const fn validate_version(&self) -> Result<(), CamouhostProtocolError> {
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

fn valid_lower_hex(value: &str, allow_empty: bool, maximum: usize) -> bool {
    if value.len() > maximum || value.len() % 2 != 0 || (!allow_empty && value.is_empty()) {
        return false;
    }
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn parse_version(value: &str) -> Result<u16, CamouhostProtocolError> {
    value
        .parse::<u16>()
        .map_err(|_| CamouhostProtocolError::MalformedFrame)
}

fn parse_session(value: &str) -> Result<SessionId, CamouhostProtocolError> {
    SessionId::parse(value.to_owned()).map_err(|_| CamouhostProtocolError::MalformedFrame)
}

fn parse_bool(value: &str) -> Result<bool, CamouhostProtocolError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(CamouhostProtocolError::MalformedFrame),
    }
}

pub trait CamouhostPort {
    fn exchange(
        &mut self,
        message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError>;
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeSession, BridgeStateError, BridgeStatus, CAMOUHOST_IPC_VERSION, CamouhostErrorKind,
        CamouhostMessage, CamouhostProtocolError, LaunchAuthorization, LaunchClaim,
        SessionCloseDecision,
    };
    use profile_platform_primitives::{
        DeviceId, GenerationId, LaunchIntentId, ProfileId, SessionId, TenantId,
    };

    fn authorization() -> Result<LaunchAuthorization, Box<dyn std::error::Error>> {
        Ok(LaunchAuthorization::new(LaunchClaim::new(
            TenantId::parse("tenant_01JATEST")?,
            ProfileId::parse("profile_01JATEST")?,
            GenerationId::parse("generation_01JATEST")?,
            LaunchIntentId::parse("intent_01JATEST")?,
            DeviceId::parse("device_01JATEST")?,
            1_900_000_000,
        )))
    }

    #[test]
    fn session_state_machine_rejects_out_of_order_transitions()
    -> Result<(), Box<dyn std::error::Error>> {
        let authorization = authorization()?;
        let mut session = BridgeSession::new(SessionId::parse("session_01JATEST")?, &authorization);
        assert_eq!(session.status(), &BridgeStatus::ClaimRedeemed);
        assert_eq!(
            session.transition_to_open(),
            Err(BridgeStateError::InvalidTransition)
        );
        session.transition_to_launching()?;
        session.transition_to_open()?;
        session.transition_to_closing()?;
        session.complete_close(SessionCloseDecision::Save)?;
        assert_eq!(session.status(), &BridgeStatus::Saved);
        assert_eq!(
            session.transition_to_closing(),
            Err(BridgeStateError::InvalidTransition)
        );
        Ok(())
    }

    #[test]
    fn camouhost_protocol_is_strict_and_versioned() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            CamouhostMessage::parse(&format!("hello|{CAMOUHOST_IPC_VERSION}"))?,
            CamouhostMessage::Hello {
                version: CAMOUHOST_IPC_VERSION
            }
        );
        assert_eq!(
            CamouhostMessage::parse("hello|999")?.validate_version(),
            Err(CamouhostProtocolError::UnsupportedVersion)
        );
        assert!(CamouhostMessage::parse("launch|").is_err());
        assert!(CamouhostMessage::parse("ready|bad id").is_err());
        assert!(CamouhostMessage::parse("closed|session_01JATEST|maybe").is_err());
        assert!(CamouhostMessage::parse("ready|session_01JATEST|extra").is_err());
        assert!(CamouhostMessage::parse("unknown|session_01JATEST").is_err());
        Ok(())
    }

    #[test]
    fn camouhost_v3_owns_observation_admission_and_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let session = SessionId::parse("session_01JAWIREV3")?;
        let messages = [
            CamouhostMessage::ObserveBrowserVisible {
                session_id: session.clone(),
            },
            CamouhostMessage::BrowserVisible {
                session_id: session.clone(),
                payload_hex: "7b7d0a".to_owned(),
            },
            CamouhostMessage::AdmitNavigation {
                session_id: session.clone(),
                target_hex: String::new(),
            },
            CamouhostMessage::NavigationAdmitted {
                session_id: session.clone(),
            },
            CamouhostMessage::Error {
                kind: CamouhostErrorKind::Runtime,
            },
        ];
        for message in messages {
            let frame = message.to_frame()?;
            assert_eq!(CamouhostMessage::parse(&frame)?, message);
        }
        assert!(CamouhostMessage::parse("browser_visible|session_01JAWIREV3|").is_err());
        assert!(CamouhostMessage::parse("browser_visible|session_01JAWIREV3|GG").is_err());
        assert!(CamouhostMessage::parse("admit_navigation|session_01JAWIREV3|0").is_err());
        assert!(CamouhostMessage::parse("navigated|session_01JAWIREV3").is_err());
        assert!(CamouhostMessage::parse("error|unknown").is_err());
        Ok(())
    }
}
