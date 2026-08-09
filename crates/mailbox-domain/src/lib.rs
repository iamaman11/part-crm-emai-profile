#![forbid(unsafe_code)]

mod binding;
mod job;
mod observation;
mod runtime_lane;

use core::fmt;

pub use binding::{MailboxBinding, MailboxBindingStatus};
pub use job::{MailboxJob, MailboxJobRestore, MailboxJobStatus, validate_cursor};
pub use observation::{
    MailboxFailureDisposition, MailboxObservation, MailboxProviderFailure,
    MailboxProviderFailureClass, validate_bounded_item_count, validate_provider_status,
};
pub use runtime_lane::{MailboxProvider, MailboxRuntimeLane};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxError {
    AlreadyRevoked,
    BindingNotExecutable,
    BindingRevoked,
    CursorTooLong,
    InvalidBindingStatus,
    InvalidBindingTransition,
    InvalidBoundedItemCount,
    InvalidFailureRetryHint,
    InvalidJobTransition,
    InvalidProvider,
    InvalidJobStatus,
    InvalidProviderStatus,
    InvalidMaxAttempts,
    JobNotDue,
    MaxAttemptsReached,
    InvalidRetryTime,
    VersionOverflow,
}

impl fmt::Display for MailboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyRevoked => "mailbox binding already revoked",
            Self::BindingNotExecutable => "mailbox binding is not executable",
            Self::BindingRevoked => "mailbox binding is revoked",
            Self::CursorTooLong => "mailbox cursor exceeds bounded length",
            Self::InvalidBindingStatus => "mailbox binding status is invalid",
            Self::InvalidBindingTransition => "mailbox binding transition is invalid",
            Self::InvalidBoundedItemCount => "mailbox observation item count exceeds bound",
            Self::InvalidFailureRetryHint => {
                "mailbox provider retry hint is invalid for failure class"
            }
            Self::InvalidJobTransition => "mailbox job transition is invalid",
            Self::InvalidProvider => "mailbox provider is invalid",
            Self::InvalidJobStatus => "mailbox job status is invalid",
            Self::InvalidProviderStatus => "mailbox provider status is invalid",
            Self::InvalidMaxAttempts => "mailbox job max attempts are invalid",
            Self::JobNotDue => "mailbox job is not due",
            Self::MaxAttemptsReached => "mailbox job attempts are exhausted",
            Self::InvalidRetryTime => "mailbox retry time must be in the future",
            Self::VersionOverflow => "mailbox aggregate version overflow",
        })
    }
}

impl std::error::Error for MailboxError {}
