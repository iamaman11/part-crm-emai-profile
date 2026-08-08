use application_ports::{
    IntegrationEventPortError, IntegrationEventPortErrorClass, NotificationPortError,
    NotificationPortErrorClass,
};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationOperationError {
    Forbidden,
    InvalidInput,
    Conflict,
    IntegrityFailure,
    DependencyUnavailable,
    InternalFailure,
}

impl fmt::Display for NotificationOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Forbidden => "notification operation is forbidden",
            Self::InvalidInput => "notification operation input is invalid",
            Self::Conflict => "notification operation conflicted with durable state",
            Self::IntegrityFailure => "notification durable state failed integrity validation",
            Self::DependencyUnavailable => "notification dependency is unavailable",
            Self::InternalFailure => "notification operation failed internally",
        })
    }
}

impl std::error::Error for NotificationOperationError {}

impl From<NotificationPortError> for NotificationOperationError {
    fn from(error: NotificationPortError) -> Self {
        match error.class() {
            NotificationPortErrorClass::Conflict => Self::Conflict,
            NotificationPortErrorClass::IntegrityFailure => Self::IntegrityFailure,
            NotificationPortErrorClass::InternalFailure => Self::InternalFailure,
            NotificationPortErrorClass::DependencyUnavailable => Self::DependencyUnavailable,
        }
    }
}

impl From<IntegrationEventPortError> for NotificationOperationError {
    fn from(error: IntegrationEventPortError) -> Self {
        match error.class() {
            IntegrationEventPortErrorClass::Conflict => Self::Conflict,
            IntegrationEventPortErrorClass::IntegrityFailure => Self::IntegrityFailure,
            IntegrationEventPortErrorClass::InternalFailure => Self::InternalFailure,
            IntegrationEventPortErrorClass::DependencyUnavailable => Self::DependencyUnavailable,
        }
    }
}
