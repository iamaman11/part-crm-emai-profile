use core::fmt;
use profile_platform_primitives::UnixMillis;

const MAX_CONFIGURED_ATTEMPTS: u16 = 64;
const MAX_RETRYABLE_ATTEMPTS: u16 = MAX_CONFIGURED_ATTEMPTS - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryAttemptCount(u16);

impl DeliveryAttemptCount {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }

    fn increment(self) -> Result<Self, DeliveryTransitionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(DeliveryTransitionError::AttemptOverflow)
    }

    fn restore(value: u16, minimum: u16, maximum: u16) -> Result<Self, DeliveryRestoreError> {
        if value < minimum || value > maximum {
            return Err(DeliveryRestoreError::InvalidAttemptCount);
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptLimit(u16);

impl AttemptLimit {
    pub fn new(value: u16) -> Result<Self, InvalidAttemptLimit> {
        if value == 0 || value > MAX_CONFIGURED_ATTEMPTS {
            return Err(InvalidAttemptLimit);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidAttemptLimit;

impl fmt::Display for InvalidAttemptLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("notification delivery attempt limit is outside accepted bounds")
    }
}

impl std::error::Error for InvalidAttemptLimit {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryFailureClass {
    DependencyUnavailable,
    Rejected,
    IntegrityFailure,
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    Ready {
        attempts: DeliveryAttemptCount,
    },
    RetryScheduled {
        attempts: DeliveryAttemptCount,
        last_attempt_at: UnixMillis,
        next_attempt_at: UnixMillis,
        failure_class: DeliveryFailureClass,
    },
    Delivered {
        attempts: DeliveryAttemptCount,
        delivered_at: UnixMillis,
    },
    DeadLetter {
        attempts: DeliveryAttemptCount,
        terminal_at: UnixMillis,
        failure_class: DeliveryFailureClass,
    },
}

impl DeliveryState {
    #[must_use]
    pub const fn new() -> Self {
        Self::Ready {
            attempts: DeliveryAttemptCount::ZERO,
        }
    }

    pub fn restore_ready(attempt_count: u16) -> Result<Self, DeliveryRestoreError> {
        Ok(Self::Ready {
            attempts: DeliveryAttemptCount::restore(attempt_count, 0, 0)?,
        })
    }

    pub fn restore_retry_scheduled(
        attempt_count: u16,
        last_attempt_at: UnixMillis,
        next_attempt_at: UnixMillis,
        failure_class: DeliveryFailureClass,
    ) -> Result<Self, DeliveryRestoreError> {
        if next_attempt_at.value() <= last_attempt_at.value() {
            return Err(DeliveryRestoreError::InvalidRetrySchedule);
        }
        Ok(Self::RetryScheduled {
            attempts: DeliveryAttemptCount::restore(
                attempt_count,
                1,
                MAX_RETRYABLE_ATTEMPTS,
            )?,
            last_attempt_at,
            next_attempt_at,
            failure_class,
        })
    }

    pub fn restore_delivered(
        attempt_count: u16,
        delivered_at: UnixMillis,
    ) -> Result<Self, DeliveryRestoreError> {
        Ok(Self::Delivered {
            attempts: DeliveryAttemptCount::restore(attempt_count, 1, MAX_CONFIGURED_ATTEMPTS)?,
            delivered_at,
        })
    }

    pub fn restore_dead_letter(
        attempt_count: u16,
        terminal_at: UnixMillis,
        failure_class: DeliveryFailureClass,
    ) -> Result<Self, DeliveryRestoreError> {
        Ok(Self::DeadLetter {
            attempts: DeliveryAttemptCount::restore(attempt_count, 1, MAX_CONFIGURED_ATTEMPTS)?,
            terminal_at,
            failure_class,
        })
    }

    #[must_use]
    pub const fn attempts(self) -> DeliveryAttemptCount {
        match self {
            Self::Ready { attempts }
            | Self::RetryScheduled { attempts, .. }
            | Self::Delivered { attempts, .. }
            | Self::DeadLetter { attempts, .. } => attempts,
        }
    }

    #[must_use]
    pub const fn last_attempt_at(self) -> Option<UnixMillis> {
        match self {
            Self::Ready { .. } => None,
            Self::RetryScheduled {
                last_attempt_at, ..
            } => Some(last_attempt_at),
            Self::Delivered { delivered_at, .. } => Some(delivered_at),
            Self::DeadLetter { terminal_at, .. } => Some(terminal_at),
        }
    }

    #[must_use]
    pub const fn next_attempt_at(self) -> Option<UnixMillis> {
        match self {
            Self::RetryScheduled {
                next_attempt_at, ..
            } => Some(next_attempt_at),
            Self::Ready { .. } | Self::Delivered { .. } | Self::DeadLetter { .. } => None,
        }
    }

    #[must_use]
    pub const fn failure_class(self) -> Option<DeliveryFailureClass> {
        match self {
            Self::RetryScheduled { failure_class, .. }
            | Self::DeadLetter { failure_class, .. } => Some(failure_class),
            Self::Ready { .. } | Self::Delivered { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered { .. } | Self::DeadLetter { .. })
    }

    #[must_use]
    pub const fn is_due(self, now: UnixMillis) -> bool {
        match self {
            Self::Ready { .. } => true,
            Self::RetryScheduled {
                next_attempt_at, ..
            } => now.value() >= next_attempt_at.value(),
            Self::Delivered { .. } | Self::DeadLetter { .. } => false,
        }
    }

    pub fn record_success(self, delivered_at: UnixMillis) -> Result<Self, DeliveryTransitionError> {
        if self.is_terminal() {
            return Err(DeliveryTransitionError::TerminalState);
        }
        Ok(Self::Delivered {
            attempts: self.attempts().increment()?,
            delivered_at,
        })
    }

    pub fn record_failure(
        self,
        failed_at: UnixMillis,
        next_attempt_at: Option<UnixMillis>,
        attempt_limit: AttemptLimit,
        failure_class: DeliveryFailureClass,
    ) -> Result<Self, DeliveryTransitionError> {
        if self.is_terminal() {
            return Err(DeliveryTransitionError::TerminalState);
        }
        if self.attempts().value() >= attempt_limit.value() {
            return Err(DeliveryTransitionError::AttemptLimitAlreadyReached);
        }

        let attempts = self.attempts().increment()?;
        if attempts.value() == attempt_limit.value() {
            if next_attempt_at.is_some() {
                return Err(DeliveryTransitionError::UnexpectedTerminalRetrySchedule);
            }
            return Ok(Self::DeadLetter {
                attempts,
                terminal_at: failed_at,
                failure_class,
            });
        }

        let Some(next_attempt_at) = next_attempt_at else {
            return Err(DeliveryTransitionError::MissingRetrySchedule);
        };
        if next_attempt_at.value() <= failed_at.value() {
            return Err(DeliveryTransitionError::InvalidRetrySchedule);
        }

        Ok(Self::RetryScheduled {
            attempts,
            last_attempt_at: failed_at,
            next_attempt_at,
            failure_class,
        })
    }
}

impl Default for DeliveryState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryRestoreError {
    InvalidAttemptCount,
    InvalidRetrySchedule,
}

impl fmt::Display for DeliveryRestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAttemptCount => "persisted notification delivery attempt count is invalid",
            Self::InvalidRetrySchedule => "persisted notification retry schedule is invalid",
        })
    }
}

impl std::error::Error for DeliveryRestoreError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryTransitionError {
    AttemptLimitAlreadyReached,
    AttemptOverflow,
    InvalidRetrySchedule,
    MissingRetrySchedule,
    TerminalState,
    UnexpectedTerminalRetrySchedule,
}

impl fmt::Display for DeliveryTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptLimitAlreadyReached => {
                "non-terminal notification delivery already reached its attempt limit"
            }
            Self::AttemptOverflow => "notification delivery attempt counter overflow",
            Self::InvalidRetrySchedule => {
                "notification retry schedule must be strictly after the failed attempt"
            }
            Self::MissingRetrySchedule => "non-terminal notification failure requires retry time",
            Self::TerminalState => "notification delivery is already terminal",
            Self::UnexpectedTerminalRetrySchedule => {
                "terminal notification failure must not carry retry time"
            }
        })
    }
}

impl std::error::Error for DeliveryTransitionError {}

#[cfg(test)]
mod tests {
    use super::{
        AttemptLimit, DeliveryFailureClass, DeliveryRestoreError, DeliveryState,
        DeliveryTransitionError, InvalidAttemptLimit,
    };
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn attempt_limit_is_positive_and_bounded() {
        assert_eq!(AttemptLimit::new(0), Err(InvalidAttemptLimit));
        assert_eq!(AttemptLimit::new(65), Err(InvalidAttemptLimit));
        assert_eq!(AttemptLimit::new(1).map(AttemptLimit::value), Ok(1));
        assert_eq!(AttemptLimit::new(64).map(AttemptLimit::value), Ok(64));
    }

    #[test]
    fn retry_must_be_present_and_move_time_forward() -> Result<(), Box<dyn std::error::Error>> {
        let limit = AttemptLimit::new(3)?;
        assert_eq!(
            DeliveryState::new().record_failure(
                UnixMillis::new(10),
                None,
                limit,
                DeliveryFailureClass::DependencyUnavailable,
            ),
            Err(DeliveryTransitionError::MissingRetrySchedule)
        );
        assert_eq!(
            DeliveryState::new().record_failure(
                UnixMillis::new(10),
                Some(UnixMillis::new(10)),
                limit,
                DeliveryFailureClass::DependencyUnavailable,
            ),
            Err(DeliveryTransitionError::InvalidRetrySchedule)
        );
        Ok(())
    }

    #[test]
    fn max_attempt_requires_terminal_shape_and_reaches_dead_letter()
    -> Result<(), Box<dyn std::error::Error>> {
        let limit = AttemptLimit::new(2)?;
        let first = DeliveryState::new().record_failure(
            UnixMillis::new(10),
            Some(UnixMillis::new(20)),
            limit,
            DeliveryFailureClass::DependencyUnavailable,
        )?;
        assert!(matches!(first, DeliveryState::RetryScheduled { .. }));
        assert_eq!(first.last_attempt_at(), Some(UnixMillis::new(10)));
        assert_eq!(
            first.record_failure(
                UnixMillis::new(20),
                Some(UnixMillis::new(30)),
                limit,
                DeliveryFailureClass::DependencyUnavailable,
            ),
            Err(DeliveryTransitionError::UnexpectedTerminalRetrySchedule)
        );

        let terminal = first.record_failure(
            UnixMillis::new(20),
            None,
            limit,
            DeliveryFailureClass::DependencyUnavailable,
        )?;
        assert!(matches!(terminal, DeliveryState::DeadLetter { .. }));
        assert_eq!(terminal.attempts().value(), 2);
        assert!(!terminal.is_due(UnixMillis::new(100)));
        Ok(())
    }

    #[test]
    fn restore_is_fail_closed_for_malformed_persisted_state()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            DeliveryState::restore_ready(1),
            Err(DeliveryRestoreError::InvalidAttemptCount)
        );
        assert_eq!(
            DeliveryState::restore_retry_scheduled(
                1,
                UnixMillis::new(20),
                UnixMillis::new(20),
                DeliveryFailureClass::InternalFailure,
            ),
            Err(DeliveryRestoreError::InvalidRetrySchedule)
        );
        assert_eq!(
            DeliveryState::restore_retry_scheduled(
                64,
                UnixMillis::new(20),
                UnixMillis::new(21),
                DeliveryFailureClass::InternalFailure,
            ),
            Err(DeliveryRestoreError::InvalidAttemptCount)
        );
        assert!(DeliveryState::restore_delivered(1, UnixMillis::new(30)).is_ok());
        assert!(
            DeliveryState::restore_dead_letter(
                64,
                UnixMillis::new(40),
                DeliveryFailureClass::Rejected,
            )
            .is_ok()
        );
        Ok(())
    }

    #[test]
    fn terminal_transition_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let delivered = DeliveryState::new().record_success(UnixMillis::new(5))?;
        assert_eq!(
            delivered.record_success(UnixMillis::new(6)),
            Err(DeliveryTransitionError::TerminalState)
        );
        Ok(())
    }
}
