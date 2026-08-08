use core::fmt;
use notification_domain::{
    AttemptLimit, DeliveryFailureClass, DeliveryState, DeliveryTransitionError,
};
use profile_platform_primitives::{OutboxEventId, UnixMillis};

const BASIS_POINTS_DENOMINATOR: u64 = 10_000;
const JITTER_HASH_MODULUS: u64 = 1_000_003;
const MAX_JITTER_BASIS_POINTS: u16 = 2_500;
const MAX_RETRY_DELAY_MS: u64 = 86_400_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    base_delay_ms: u64,
    max_delay_ms: u64,
    jitter_basis_points: u16,
    attempt_limit: AttemptLimit,
}

impl RetryPolicy {
    pub fn new(
        base_delay_ms: u64,
        max_delay_ms: u64,
        jitter_basis_points: u16,
        attempt_limit: AttemptLimit,
    ) -> Result<Self, RetryPolicyConfigError> {
        if base_delay_ms == 0 {
            return Err(RetryPolicyConfigError::ZeroBaseDelay);
        }
        if max_delay_ms < base_delay_ms {
            return Err(RetryPolicyConfigError::MaxDelayBeforeBase);
        }
        if max_delay_ms > MAX_RETRY_DELAY_MS {
            return Err(RetryPolicyConfigError::MaxDelayExceedsBound);
        }
        if jitter_basis_points > MAX_JITTER_BASIS_POINTS {
            return Err(RetryPolicyConfigError::JitterExceedsBound);
        }
        Ok(Self {
            base_delay_ms,
            max_delay_ms,
            jitter_basis_points,
            attempt_limit,
        })
    }

    #[must_use]
    pub const fn attempt_limit(self) -> AttemptLimit {
        self.attempt_limit
    }

    pub fn transition_after_failure(
        self,
        state: DeliveryState,
        failed_at: UnixMillis,
        event_id: &OutboxEventId,
        failure_class: DeliveryFailureClass,
    ) -> Result<DeliveryState, RetrySchedulingError> {
        let failed_attempt = state
            .attempts()
            .value()
            .checked_add(1)
            .ok_or(RetrySchedulingError::AttemptOverflow)?;

        if failed_attempt == self.attempt_limit.value() {
            return state
                .record_failure(failed_at, None, self.attempt_limit, failure_class)
                .map_err(RetrySchedulingError::DomainTransition);
        }
        if failed_attempt > self.attempt_limit.value() {
            return Err(RetrySchedulingError::AttemptLimitAlreadyReached);
        }

        let delay_ms = self.retry_delay_ms(event_id, failed_attempt)?;
        let next_attempt_ms = failed_at
            .value()
            .checked_add(delay_ms)
            .ok_or(RetrySchedulingError::TimestampOverflow)?;
        state
            .record_failure(
                failed_at,
                Some(UnixMillis::new(next_attempt_ms)),
                self.attempt_limit,
                failure_class,
            )
            .map_err(RetrySchedulingError::DomainTransition)
    }

    fn retry_delay_ms(
        self,
        event_id: &OutboxEventId,
        failed_attempt: u16,
    ) -> Result<u64, RetrySchedulingError> {
        if failed_attempt == 0 || failed_attempt >= self.attempt_limit.value() {
            return Err(RetrySchedulingError::AttemptOutsideRetryWindow);
        }

        let mut nominal_delay = self.base_delay_ms;
        for _ in 1..failed_attempt {
            if nominal_delay >= self.max_delay_ms {
                break;
            }
            nominal_delay = match nominal_delay.checked_mul(2) {
                Some(doubled) => doubled.min(self.max_delay_ms),
                None => self.max_delay_ms,
            };
        }

        let jitter_span = nominal_delay
            .checked_mul(u64::from(self.jitter_basis_points))
            .ok_or(RetrySchedulingError::DelayArithmeticOverflow)?
            / BASIS_POINTS_DENOMINATOR;
        if jitter_span == 0 {
            return Ok(nominal_delay);
        }

        let jitter_width = jitter_span
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(RetrySchedulingError::DelayArithmeticOverflow)?;
        let sample = deterministic_jitter_sample(event_id, failed_attempt, jitter_width);
        let lower = nominal_delay
            .checked_sub(jitter_span)
            .ok_or(RetrySchedulingError::DelayArithmeticOverflow)?;
        let jittered = lower
            .checked_add(sample)
            .ok_or(RetrySchedulingError::DelayArithmeticOverflow)?;

        Ok(jittered.clamp(1, self.max_delay_ms))
    }
}

fn deterministic_jitter_sample(
    event_id: &OutboxEventId,
    failed_attempt: u16,
    width: u64,
) -> u64 {
    let mut accumulator = 0_u64;
    for byte in event_id.as_str().bytes() {
        accumulator = (accumulator * 131 + u64::from(byte)) % JITTER_HASH_MODULUS;
    }
    accumulator = (accumulator * 257 + u64::from(failed_attempt)) % JITTER_HASH_MODULUS;
    accumulator % width
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyConfigError {
    JitterExceedsBound,
    MaxDelayBeforeBase,
    MaxDelayExceedsBound,
    ZeroBaseDelay,
}

impl fmt::Display for RetryPolicyConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::JitterExceedsBound => "notification retry jitter exceeds accepted bound",
            Self::MaxDelayBeforeBase => {
                "notification retry max delay must be at least the base delay"
            }
            Self::MaxDelayExceedsBound => "notification retry max delay exceeds accepted bound",
            Self::ZeroBaseDelay => "notification retry base delay must be greater than zero",
        })
    }
}

impl std::error::Error for RetryPolicyConfigError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrySchedulingError {
    AttemptLimitAlreadyReached,
    AttemptOutsideRetryWindow,
    AttemptOverflow,
    DelayArithmeticOverflow,
    DomainTransition(DeliveryTransitionError),
    TimestampOverflow,
}

impl fmt::Display for RetrySchedulingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptLimitAlreadyReached => {
                "notification retry requested after automatic attempt limit"
            }
            Self::AttemptOutsideRetryWindow => {
                "notification retry attempt is outside automatic retry window"
            }
            Self::AttemptOverflow => "notification retry attempt counter overflow",
            Self::DelayArithmeticOverflow => "notification retry delay arithmetic overflow",
            Self::DomainTransition(_) => "notification delivery transition rejected retry policy",
            Self::TimestampOverflow => "notification retry timestamp overflow",
        })
    }
}

impl std::error::Error for RetrySchedulingError {}

#[cfg(test)]
mod tests {
    use super::{RetryPolicy, RetryPolicyConfigError, RetrySchedulingError};
    use notification_domain::{AttemptLimit, DeliveryFailureClass, DeliveryState};
    use profile_platform_primitives::{OutboxEventId, UnixMillis};

    fn policy() -> Result<RetryPolicy, Box<dyn std::error::Error>> {
        Ok(RetryPolicy::new(1_000, 60_000, 1_000, AttemptLimit::new(6)?)?)
    }

    #[test]
    fn policy_configuration_is_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let attempts = AttemptLimit::new(3)?;
        assert_eq!(
            RetryPolicy::new(0, 1_000, 0, attempts),
            Err(RetryPolicyConfigError::ZeroBaseDelay)
        );
        assert_eq!(
            RetryPolicy::new(2_000, 1_000, 0, attempts),
            Err(RetryPolicyConfigError::MaxDelayBeforeBase)
        );
        assert_eq!(
            RetryPolicy::new(1_000, 86_400_001, 0, attempts),
            Err(RetryPolicyConfigError::MaxDelayExceedsBound)
        );
        assert_eq!(
            RetryPolicy::new(1_000, 2_000, 2_501, attempts),
            Err(RetryPolicyConfigError::JitterExceedsBound)
        );
        Ok(())
    }

    #[test]
    fn deterministic_backoff_vectors_are_stable_and_non_zero()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = policy()?;
        let event_id = OutboxEventId::parse("outbox_retry_alpha")?;
        let expected_delays = [960_u64, 1_829, 3_617, 8_413, 15_811];
        let mut state = DeliveryState::new();
        let mut failed_at = UnixMillis::new(10_000);

        for expected_delay in expected_delays {
            state = policy.transition_after_failure(
                state,
                failed_at,
                &event_id,
                DeliveryFailureClass::DependencyUnavailable,
            )?;
            let retry_at = state.next_attempt_at().ok_or("missing retry time")?;
            assert_eq!(retry_at.value() - failed_at.value(), expected_delay);
            assert!(retry_at.value() > failed_at.value());
            failed_at = retry_at;
        }
        Ok(())
    }

    #[test]
    fn automatic_attempt_limit_becomes_terminal_without_retry_time()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetryPolicy::new(100, 1_000, 0, AttemptLimit::new(2)?)?;
        let event_id = OutboxEventId::parse("outbox_retry_terminal")?;
        let first = policy.transition_after_failure(
            DeliveryState::new(),
            UnixMillis::new(10),
            &event_id,
            DeliveryFailureClass::Rejected,
        )?;
        assert!(first.next_attempt_at().is_some());

        let terminal = policy.transition_after_failure(
            first,
            UnixMillis::new(110),
            &event_id,
            DeliveryFailureClass::Rejected,
        )?;
        assert!(matches!(terminal, DeliveryState::DeadLetter { .. }));
        assert!(terminal.next_attempt_at().is_none());
        assert_eq!(terminal.attempts().value(), 2);
        Ok(())
    }

    #[test]
    fn timestamp_overflow_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetryPolicy::new(10, 10, 0, AttemptLimit::new(2)?)?;
        let event_id = OutboxEventId::parse("outbox_retry_overflow")?;
        assert_eq!(
            policy.transition_after_failure(
                DeliveryState::new(),
                UnixMillis::new(u64::MAX - 5),
                &event_id,
                DeliveryFailureClass::InternalFailure,
            ),
            Err(RetrySchedulingError::TimestampOverflow)
        );
        Ok(())
    }

    #[test]
    fn jitter_is_bounded_by_configured_max() -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetryPolicy::new(10_000, 10_000, 2_500, AttemptLimit::new(2)?)?;
        let event_id = OutboxEventId::parse("outbox_retry_capped")?;
        let state = policy.transition_after_failure(
            DeliveryState::new(),
            UnixMillis::new(100),
            &event_id,
            DeliveryFailureClass::DependencyUnavailable,
        )?;
        let retry_at = state.next_attempt_at().ok_or("missing retry time")?;
        let delay = retry_at.value() - 100;
        assert!((7_500..=10_000).contains(&delay));
        Ok(())
    }
}
