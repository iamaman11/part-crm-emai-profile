#![forbid(unsafe_code)]

pub mod cursor;
pub mod delivery;

pub use cursor::{CursorAdvanceError, NotificationCursor};
pub use delivery::{
    AttemptLimit, DeliveryAttemptCount, DeliveryFailureClass, DeliveryRestoreError, DeliveryState,
    DeliveryTransitionError,
};
