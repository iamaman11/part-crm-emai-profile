use core::{cmp::Ordering, fmt};
use profile_platform_primitives::{OutboxEventId, UnixMillis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationCursor {
    occurred_at: UnixMillis,
    event_id: OutboxEventId,
}

impl NotificationCursor {
    #[must_use]
    pub const fn new(occurred_at: UnixMillis, event_id: OutboxEventId) -> Self {
        Self {
            occurred_at,
            event_id,
        }
    }

    #[must_use]
    pub const fn occurred_at(&self) -> UnixMillis {
        self.occurred_at
    }

    #[must_use]
    pub const fn event_id(&self) -> &OutboxEventId {
        &self.event_id
    }

    pub fn advance_to(self, candidate: Self) -> Result<Self, CursorAdvanceError> {
        match self.compare_position(&candidate) {
            Ordering::Less => Ok(candidate),
            Ordering::Equal => Ok(self),
            Ordering::Greater => Err(CursorAdvanceError::Rewind),
        }
    }

    fn compare_position(&self, other: &Self) -> Ordering {
        self.occurred_at
            .value()
            .cmp(&other.occurred_at.value())
            .then_with(|| self.event_id.as_str().cmp(other.event_id.as_str()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorAdvanceError {
    Rewind,
}

impl fmt::Display for CursorAdvanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("notification cursor cannot move backwards")
    }
}

impl std::error::Error for CursorAdvanceError {}

#[cfg(test)]
mod tests {
    use super::{CursorAdvanceError, NotificationCursor};
    use profile_platform_primitives::{OutboxEventId, UnixMillis};

    fn cursor(time: u64, id: &str) -> Result<NotificationCursor, Box<dyn std::error::Error>> {
        Ok(NotificationCursor::new(
            UnixMillis::new(time),
            OutboxEventId::parse(id)?,
        ))
    }

    #[test]
    fn cursor_advances_monotonically_and_equal_is_idempotent()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = cursor(10, "outbox_01JCURSOR_A")?;
        let same = cursor(10, "outbox_01JCURSOR_A")?;
        assert_eq!(first.clone().advance_to(same)?, first);

        let later = cursor(11, "outbox_01JCURSOR_B")?;
        assert_eq!(first.advance_to(later.clone())?, later);
        Ok(())
    }

    #[test]
    fn cursor_rejects_time_or_tie_break_rewind() -> Result<(), Box<dyn std::error::Error>> {
        let current = cursor(10, "outbox_01JCURSOR_B")?;
        assert_eq!(
            current.clone().advance_to(cursor(9, "outbox_01JCURSOR_Z")?),
            Err(CursorAdvanceError::Rewind)
        );
        assert_eq!(
            current.advance_to(cursor(10, "outbox_01JCURSOR_A")?),
            Err(CursorAdvanceError::Rewind)
        );
        Ok(())
    }
}
