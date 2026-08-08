use core::fmt;
use profile_platform_primitives::UnixMillis;

const MAX_CONFIGURED_ATTEMPTS: u16 = 64;

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
    pub const fn next_attempt_at(self) -> Option<UnixMillis> {
        match self {
            Self::RetryScheduled {
                next_attempt_at, ..
            } => Some(next_attempt_at),
            Self::Ready { .. } | Self::Delivered { .. } | Self::DeadLetter { .. } => None,
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
        next_attempt_at: UnixMillis,
        attempt_limit: AttemptLimit,
        failure_class: DeliveryFailureClass,
    ) -> Result<Self, DeliveryTransitionError> {
        if self.is_terminal() {
            return Err(DeliveryTransitionError::TerminalState);
        }

        let attempts = self.attempts().increment()?;
        if attempts.value() >= attempt_limit.value() {
            return Ok(Self::DeadLetter {
                attempts,
                terminal_at: failed_at,
                failure_class,
            });
        }
        if next_attempt_at.value() <= failed_at.value() {
            return Err(DeliveryTransitionError::InvalidRetrySchedule);
        }

        Ok(Self::RetryScheduled {
            attempts,
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
pub enum DeliveryTransitionError {
    AttemptOverflow,
    InvalidRetrySchedule,
    TerminalState,
}

impl fmt::Display for DeliveryTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptOverflow => "notification delivery attempt counter overflow",
            Self::InvalidRetrySchedule => {
                "notification retry schedule must be strictly after the failed attempt"
            }
            Self::TerminalState => "notification delivery is already terminal",
        })
    }
}

impl std::error::Error for DeliveryTransitionError {}

#[cfg(test)]
mod tests {
    use super::{
        AttemptLimit, DeliveryFailureClass, DeliveryState, DeliveryTransitionError,
        InvalidAttemptLimit,
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
    fn retry_must_move_time_forward() -> Result<(), Box<dyn std::error::Error>> {
        let limit = AttemptLimit::new(3)?;
        let result = DeliveryState::new().record_failure(
            UnixMillis::new(10),
            UnixMillis::new(10),
            limit,
            DeliveryFailureClass::DependencyUnavailable,
        );
        assert_eq!(result, Err(DeliveryTransitionError::InvalidRetrySchedule));
        Ok(())
    }

    #[test]
    fn max_attempt_reaches_dead_letter_deterministically() -> Result<(), Box<dyn std::error::Error>>
    {
        let limit = AttemptLimit::new(2)?;
        let first = DeliveryState::new().record_failure(
            UnixMillis::new(10),
            UnixMillis::new(20),
            limit,
            DeliveryFailureClass::DependencyUnavailable,
        )?;
        assert!(matches!(first, DeliveryState::RetryScheduled { .. }));

        let terminal = first.record_failure(
            UnixMillis::new(20),
            UnixMillis::new(30),
            limit,
            DeliveryFailureClass::DependencyUnavailable,
        )?;
        assert!(matches!(terminal, DeliveryState::DeadLetter { .. }));
        assert_eq!(terminal.attempts().value(), 2);
        assert!(!terminal.is_due(UnixMillis::new(100)));
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
