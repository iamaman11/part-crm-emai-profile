//! Safe user-facing explanations of existing native owner outcomes.
//!
//! Static text only: raw errors, URLs, claims, keys, responses and local paths cannot enter this API.

use crate::device_pairing::DevicePairingBootstrapError;
use crate::shipping_composition::ShippingCompositionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserDiagnostic {
    Installation(crate::windows_delivery_install::NativeInstallerError),
    StartFromApplication,
    InvalidHandoff,
    Pairing(DevicePairingBootstrapError),
    Launch(ShippingCompositionError),
}

impl UserDiagnostic {
    #[must_use]
    pub const fn code(self) -> &'static str {
        use DevicePairingBootstrapError as Pairing;
        match self {
            Self::Installation(error) => {
                use crate::windows_delivery_coordinator::WindowsDeliveryCoordinatorError as Preparation;
                use crate::windows_delivery_install::NativeInstallerError as Install;
                match error {
                    Install::UnsupportedPlatform => "BRIDGE_WINDOWS_REQUIRED",
                    Install::InvalidBundle | Install::InputIo => {
                        "BRIDGE_INSTALLATION_PACKAGE_INVALID"
                    }
                    Install::StateStore(_) => "BRIDGE_INSTALLATION_STATE_INVALID",
                    Install::Preparation(Preparation::Policy(_)) => {
                        "BRIDGE_INSTALLATION_ADMISSION_REJECTED"
                    }
                    Install::Preparation(Preparation::FirstInstallation(_)) => {
                        "BRIDGE_INSTALLATION_CONFLICT"
                    }
                    Install::Preparation(Preparation::Download(_)) => {
                        "BRIDGE_INSTALLATION_DOWNLOAD_FAILED"
                    }
                    Install::Preparation(
                        Preparation::Staging(_) | Preparation::StageIdentityMismatch,
                    ) => "BRIDGE_INSTALLATION_FILES_INVALID",
                    Install::InstalledConnection(
                        crate::installed_connection::InstalledConnectionError::Conflict,
                    ) => "BRIDGE_CONNECTION_CONFLICT",
                    _ => "BRIDGE_INSTALLATION_INCOMPLETE",
                }
            }
            Self::StartFromApplication => "BRIDGE_OPEN_FROM_APPLICATION",
            Self::InvalidHandoff => "BRIDGE_INVALID_HANDOFF",
            Self::Pairing(error) => match error {
                Pairing::UnsupportedPlatform => "BRIDGE_WINDOWS_REQUIRED",
                Pairing::Configuration => "BRIDGE_SETUP_INCOMPLETE",
                Pairing::ExistingBinding => "BRIDGE_CONNECTION_EXISTS",
                Pairing::Clock => "BRIDGE_CLOCK_UNAVAILABLE",
                Pairing::ChallengeExpired => "BRIDGE_AUTHORIZATION_EXPIRED",
                Pairing::DeviceKey => "BRIDGE_DEVICE_KEY_UNAVAILABLE",
                Pairing::Transport => "BRIDGE_SERVICE_UNREACHABLE",
                Pairing::Rejected => "BRIDGE_CONNECTION_REJECTED",
                Pairing::InvalidResponse => "BRIDGE_RESPONSE_INVALID",
                Pairing::BindingStorage => "BRIDGE_CONNECTION_NOT_SAVED",
                Pairing::BrowserOpen => "BRIDGE_BROWSER_NOT_OPENED",
                Pairing::ConnectedBrowserReturn => "BRIDGE_CONNECTED_RETURN_PENDING",
                Pairing::CleanupRequired => "BRIDGE_CLEANUP_REQUIRED",
            },
            Self::Launch(error) => match error {
                ShippingCompositionError::UnsupportedPlatform => "BRIDGE_WINDOWS_REQUIRED",
                ShippingCompositionError::Configuration => "BRIDGE_SETUP_INCOMPLETE",
                ShippingCompositionError::Clock => "BRIDGE_CLOCK_UNAVAILABLE",
                ShippingCompositionError::ControlPlane => "BRIDGE_SESSION_NOT_AUTHORIZED",
                ShippingCompositionError::DeliveryHealth => "BRIDGE_RUNTIME_NOT_READY",
                ShippingCompositionError::DeliveryHandoff => "BRIDGE_UPDATE_INTERRUPTED",
                ShippingCompositionError::FirstInstallation(error) => {
                    use crate::windows_delivery_handoff::DeliveryHandoffError as Install;
                    match error {
                        Install::FirstInstallationConflict => "BRIDGE_INSTALLATION_CONFLICT",
                        Install::NoStagedTarget => "BRIDGE_INSTALLATION_NOT_PREPARED",
                        Install::InvalidTarget
                        | Install::InvalidInstalledLayout
                        | Install::CurrentExecutableMismatch
                        | Install::Staging(_) => "BRIDGE_INSTALLATION_FILES_INVALID",
                        _ => "BRIDGE_INSTALLATION_INCOMPLETE",
                    }
                }
                ShippingCompositionError::InstalledConnection(error) => {
                    use crate::installed_connection::InstalledConnectionError as Connection;
                    match error {
                        Connection::Conflict => "BRIDGE_CONNECTION_CONFLICT",
                        Connection::InvalidRecord => "BRIDGE_CONNECTION_INVALID",
                        Connection::Storage | Connection::Layout(_) => "BRIDGE_SETUP_INCOMPLETE",
                    }
                }
                ShippingCompositionError::DeliveryRecoveryRequired => {
                    "BRIDGE_UPDATE_RECOVERY_REQUIRED"
                }
                ShippingCompositionError::Operator => "BRIDGE_PROFILE_OPERATION_FAILED",
                ShippingCompositionError::CommittedRecoveryRequired => {
                    "BRIDGE_SAVED_LOCAL_RECOVERY_REQUIRED"
                }
            },
        }
    }

    #[must_use]
    pub const fn message(self) -> &'static str {
        use DevicePairingBootstrapError as Pairing;
        use ShippingCompositionError as Launch;
        match self {
            Self::Installation(_) => {
                "Profile Bridge installation could not be completed. Review the diagnostic code in setup and try again from the application. Keep existing device and profile files; do not reset installation state manually."
            }
            Self::StartFromApplication => {
                "Open Profile Operations in your browser and choose a profile or Connect this computer. Profile Bridge starts from the application."
            }
            Self::InvalidHandoff => {
                "The application link is invalid. Return to Profile Operations and start the action again. Do not copy or edit the link."
            }
            Self::Pairing(Pairing::UnsupportedPlatform)
            | Self::Launch(Launch::UnsupportedPlatform) => {
                "Opening browser profiles requires a supported Windows computer. Return to the application on Windows."
            }
            Self::Pairing(Pairing::Configuration) | Self::Launch(Launch::Configuration) => {
                "Profile Bridge setup is incomplete. Return to the application to finish computer setup. This computer is not ready to open profiles."
            }
            Self::Pairing(Pairing::ExistingBinding) => {
                "A connection is already stored on this computer. Return to the application and check the existing device before attempting a new connection."
            }
            Self::Pairing(Pairing::Clock) | Self::Launch(Launch::Clock) => {
                "Windows could not provide the current time. Check Windows date and time, then return to the application."
            }
            Self::Pairing(Pairing::ChallengeExpired) => {
                "The connection confirmation has expired. Return to the application and start a new connection confirmation."
            }
            Self::Pairing(Pairing::DeviceKey) => {
                "Windows could not open the protected device key. Return to the application to review this computer's connection. Do not delete device keys or profile data."
            }
            Self::Pairing(Pairing::Transport) => {
                "Profile Bridge could not reach the application. Check your connection, then return to the application to check the connection state before trying again."
            }
            Self::Pairing(Pairing::Rejected) => {
                "The application rejected this connection request. Return to the application and check your access and the current device state before reconnecting."
            }
            Self::Pairing(Pairing::InvalidResponse) => {
                "The application response could not be verified. Connection has not been confirmed. Return to the application and review the connection state."
            }
            Self::Pairing(Pairing::BindingStorage) => {
                "The connection could not be saved on this computer. Return to the application and review the device state before reconnecting. Do not delete profile data."
            }
            Self::Pairing(Pairing::BrowserOpen) => {
                "The authorization page could not be opened in your default browser. Check the Windows default browser, then return to the application and start connection again."
            }
            Self::Pairing(Pairing::ConnectedBrowserReturn) => {
                "The device connection was saved, but the application page could not be opened. Return to your browser and check this computer in the application. Profile runtime readiness has not been confirmed."
            }
            Self::Pairing(Pairing::CleanupRequired) => {
                "The connection attempt needs recovery on this computer. Return to the application and review this device. Do not repeatedly reconnect or delete device keys."
            }
            Self::Launch(Launch::ControlPlane) => {
                "The profile session could not be authorized. Return to the application and check your access and the profile's current session."
            }
            Self::Launch(Launch::DeliveryHealth) => {
                "The installed browser components have not passed their readiness check. Return to computer setup in the application. The profile has not been opened."
            }
            Self::Launch(Launch::DeliveryHandoff | Launch::DeliveryRecoveryRequired) => {
                "The browser component update needs recovery. Return to computer setup in the application. Do not remove local profile files."
            }
            Self::Launch(Launch::FirstInstallation(_)) => {
                "Profile Bridge installation could not be completed. Return to computer setup in the application and review the diagnostic code. Keep existing profile files until recovery is confirmed."
            }
            Self::Launch(Launch::InstalledConnection(_)) => {
                "Profile Bridge connection settings need repair. Return to computer setup in the application. Keep existing device and profile files; setup must not silently switch the connected server."
            }
            Self::Launch(Launch::Operator) => {
                "The profile operation did not complete. Return to the application and check session and save status. Keep local profile files until recovery is confirmed."
            }
            Self::Launch(Launch::CommittedRecoveryRequired) => {
                "The new profile state was confirmed by the server, but local cleanup needs recovery. Return to the application to review the session. Keep local profile files."
            }
        }
    }

    /// Presentation cannot retry, enroll, launch, update or report a successful operation.
    pub fn show_desktop(self) {
        #[cfg(windows)]
        {
            let _ = windows_desktop_ui::show_error_unless_redirected(
                "Profile Operations",
                self.code(),
                self.message(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_setup_and_lost_browser_return_are_distinct() {
        let setup = UserDiagnostic::Pairing(DevicePairingBootstrapError::Configuration);
        let saved = UserDiagnostic::Pairing(DevicePairingBootstrapError::ConnectedBrowserReturn);
        assert_eq!(setup.code(), "BRIDGE_SETUP_INCOMPLETE");
        assert_ne!(setup.code(), saved.code());
        assert!(saved.message().contains("was saved"));
        assert!(saved.message().contains("readiness has not been confirmed"));
    }

    #[test]
    fn committed_successor_is_not_reported_as_an_unsaved_failure() {
        let diagnosis = UserDiagnostic::Launch(ShippingCompositionError::CommittedRecoveryRequired);
        assert_eq!(diagnosis.code(), "BRIDGE_SAVED_LOCAL_RECOVERY_REQUIRED");
        assert!(diagnosis.message().contains("confirmed by the server"));
        assert!(diagnosis.message().contains("Keep local profile files"));
    }
}
