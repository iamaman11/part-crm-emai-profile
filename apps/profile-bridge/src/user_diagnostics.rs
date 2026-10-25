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
                ShippingCompositionError::ExternalNetworkConfiguration => {
                    "BRIDGE_MANUAL_NETWORK_SETTINGS"
                }
                ShippingCompositionError::Initialization(error) => match error {
                    crate::generation_initialization::InitialProfileError::RuntimeBundle => {
                        "BRIDGE_RUNTIME_BUNDLE_INVALID"
                    }
                    crate::generation_initialization::InitialProfileError::Runtime => {
                        "BRIDGE_FIRST_OPEN_RUNTIME_FAILED"
                    }
                    crate::generation_initialization::InitialProfileError::Upload
                    | crate::generation_initialization::InitialProfileError::Verification => {
                        "BRIDGE_FIRST_SAVE_UPLOAD_FAILED"
                    }
                    crate::generation_initialization::InitialProfileError::SavedRecovery => {
                        "BRIDGE_SAVED_LOCAL_RECOVERY_REQUIRED"
                    }
                    _ => "BRIDGE_PROFILE_OPEN_INCOMPLETE",
                },
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
                ShippingCompositionError::Operator(error) => operator_code(error),
                ShippingCompositionError::ActiveSessionMissing => "BRIDGE_ACTIVE_SESSION_MISSING",
                ShippingCompositionError::CloseObservation(_) => "BRIDGE_CLOSE_OBSERVATION_FAILED",
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
            Self::Launch(Launch::ExternalNetworkConfiguration) => {
                "Profile Bridge found network settings from a manual installation. Installation needs repair before this profile can start. Existing profile restrictions and data have been preserved."
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
                "The installed browser update has not completed its readiness check. Update Profile Bridge from Install for Windows in the application when a newer repair is available. Keep this computer connected and preserve local profile files; reconnecting will not repair the browser update."
            }
            Self::Launch(Launch::FirstInstallation(_)) => {
                "Profile Bridge installation could not be completed. Return to computer setup in the application and review the diagnostic code. Keep existing profile files until recovery is confirmed."
            }
            Self::Launch(Launch::InstalledConnection(_)) => {
                "Profile Bridge connection settings need repair. Return to computer setup in the application. Keep existing device and profile files; setup must not silently switch the connected server."
            }
            Self::Launch(Launch::Initialization(
                crate::generation_initialization::InitialProfileError::RuntimeBundle,
            )) => {
                "The installed browser package could not be verified. Update Profile Bridge from Install for Windows in the application. Reconnecting this computer will not repair the package. Keep existing device and profile files."
            }
            Self::Launch(Launch::Initialization(
                crate::generation_initialization::InitialProfileError::Runtime,
            )) => {
                "The verified browser package could not prepare the initial browser state. Keep this computer connected and preserve local profile files. Report this diagnostic code; do not reconnect or reset the profile."
            }
            Self::Launch(Launch::Initialization(_)) => {
                "The profile could not open. Return to the application to check access, connection and session status, then try Open again. Keep local profile files; a failed request is not proof of a saved copy."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence,
                    ..
                },
            )) => {
                "The local profile copy is incomplete or differs from the verified cloud copy. Its existing data has been preserved. Return to Profile Operations to review recovery status; do not delete profile files or reconnect this computer."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::InvalidLocalEvidence,
                    ..
                },
            )) => {
                "The local profile evidence does not match the selected verified generation. The application preserved existing data and refused an unsafe launch. Review recovery status in Profile Operations; do not replace the local files manually."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::LocalAccessDenied,
                    ..
                },
            )) => {
                "Windows denied access to the local profile evidence. Existing files were preserved. Check Windows file access and the connected device status from Profile Operations; do not delete or replace the profile."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason:
                        crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Download(
                            crate::generation_reopen::GenerationObjectDownloadError::Transport,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                                crate::generation_reopen::GenerationObjectDownloadError::Transport,
                            ),
                        ),
                    ..
                },
            )) => {
                "The encrypted profile object could not be downloaded because its network request failed. Preserve local files and retry from Profile Operations when connectivity is restored."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason:
                        crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Download(
                            crate::generation_reopen::GenerationObjectDownloadError::HttpStatus,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                                crate::generation_reopen::GenerationObjectDownloadError::HttpStatus,
                            ),
                        ),
                    ..
                },
            )) => {
                "The encrypted profile object request was rejected or could not be completed by storage. Check Profile Operations; do not assume the entire service is unreachable."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason:
                        crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                            crate::generation_reopen::GenerationReopenControlError::Transport,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                            crate::generation_reopen::GenerationReopenControlError::Transport,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                                crate::generation_reopen::GenerationReopenControlError::Transport,
                            ),
                        ),
                    ..
                },
            )) => {
                "The application could not be reached while retrieving the verified profile generation. Check your connection and return to Profile Operations; local profile files were preserved."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason:
                        crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                            crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                            crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                                crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                            ),
                        ),
                    ..
                },
            )) => {
                "The application rejected the profile generation request. Check the current access and session in Profile Operations. Do not delete local data or assume the service is offline."
            }
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::AuthoritativeGeneration {
                    reason:
                        crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                            crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                            crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                        )
                        | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                            crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                                crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                            ),
                        ),
                    ..
                },
            )) => {
                "The profile generation response could not be verified. The profile was not opened; return to Profile Operations and preserve existing files."
            }
            Self::Launch(
                Launch::Operator(_) | Launch::ActiveSessionMissing | Launch::CloseObservation(_),
            ) => {
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

const fn operator_code(error: crate::operator_flow::OperatorFlowError) -> &'static str {
    use crate::operator_flow::{OperatorFailureStage as Stage, OperatorFlowError as Error};
    match error {
        Error::Busy => "BRIDGE_PROFILE_BUSY",
        Error::CleanupRequired => "BRIDGE_CLEANUP_REQUIRED",
        Error::AuthoritativeGeneration {
            reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence,
            ..
        } => "BRIDGE_LOCAL_CACHE_INCOMPLETE",
        Error::AuthoritativeGeneration {
            reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::InvalidLocalEvidence,
            ..
        } => "BRIDGE_LOCAL_CACHE_INTEGRITY_FAILED",
        Error::AuthoritativeGeneration {
            reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::LocalAccessDenied,
            ..
        } => "BRIDGE_LOCAL_CACHE_INACCESSIBLE",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Download(
                    crate::generation_reopen::GenerationObjectDownloadError::Transport,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                        crate::generation_reopen::GenerationObjectDownloadError::Transport,
                    ),
                ),
            ..
        } => "BRIDGE_GENERATION_OBJECT_TRANSPORT_FAILED",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Download(
                    crate::generation_reopen::GenerationObjectDownloadError::HttpStatus,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                        crate::generation_reopen::GenerationObjectDownloadError::HttpStatus,
                    ),
                ),
            ..
        } => "BRIDGE_GENERATION_OBJECT_HTTP_REJECTED",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Download(_)
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(_),
                ),
            ..
        } => "BRIDGE_GENERATION_INTEGRITY_FAILED",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::Transport,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::Transport,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                        crate::generation_reopen::GenerationReopenControlError::Transport,
                    ),
                ),
            ..
        } => "BRIDGE_SERVICE_UNREACHABLE",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                        crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                    ),
                ),
            ..
        } => "BRIDGE_GENERATION_REQUEST_REJECTED",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                        crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                    ),
                ),
            ..
        } => "BRIDGE_GENERATION_RESPONSE_INVALID",
        Error::AuthoritativeGeneration {
            reason:
                crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::Clock,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::Clock,
                )
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningControl(
                        crate::generation_reopen::GenerationReopenControlError::Clock,
                    ),
                ),
            ..
        } => "BRIDGE_CLOCK_UNAVAILABLE",
        Error::AuthoritativeGeneration {
            reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::DescriptorMismatch
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::MetadataMismatch
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::OpeningMaterialMismatch
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::Decryption
                | crate::authoritative_generation::AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::DescriptorMismatch
                    | crate::authoritative_generation::AuthoritativeGenerationPreWriteError::MetadataMismatch
                    | crate::authoritative_generation::AuthoritativeGenerationPreWriteError::OpeningMaterialMismatch
                    | crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Decryption,
                ),
            ..
        } => "BRIDGE_GENERATION_INTEGRITY_FAILED",
        Error::Preflight {
            reason: crate::operator_flow::OperationalRejectionReason::LocalCacheIncomplete,
            ..
        } => "BRIDGE_LOCAL_CACHE_INCOMPLETE",
        Error::Preflight {
            reason: crate::operator_flow::OperationalRejectionReason::LocalCacheInaccessible,
            ..
        } => "BRIDGE_LOCAL_CACHE_INACCESSIBLE",
        _ => match error.stage() {
            Some(Stage::DeviceIdentity) => "BRIDGE_DEVICE_IDENTITY_FAILED",
            Some(Stage::DeviceKey) => "BRIDGE_DEVICE_KEY_UNAVAILABLE",
            Some(Stage::DeviceAuthentication) => "BRIDGE_DEVICE_AUTHENTICATION_FAILED",
            Some(Stage::Enrollment) => "BRIDGE_LAUNCH_REDEMPTION_FAILED",
            Some(Stage::RuntimeBundle) => "BRIDGE_RUNTIME_BUNDLE_INVALID",
            Some(Stage::DeliveryHealthStart) => "BRIDGE_RUNTIME_HEALTH_START_FAILED",
            Some(Stage::CoordinatorClaim) => "BRIDGE_COORDINATOR_CLAIM_FAILED",
            Some(Stage::CoordinatorHeartbeat) => "BRIDGE_COORDINATOR_HEARTBEAT_FAILED",
            Some(Stage::LeaseValidation) => "BRIDGE_LEASE_VALIDATION_FAILED",
            Some(Stage::AuthoritativeGeneration) => "BRIDGE_GENERATION_RESTORE_FAILED",
            Some(Stage::LocalWorkspace) => "BRIDGE_LOCAL_WORKSPACE_FAILED",
            Some(Stage::BrowserPreflight) => "BRIDGE_BROWSER_PREFLIGHT_FAILED",
            Some(Stage::LocalLifecycle) => "BRIDGE_LOCAL_LIFECYCLE_FAILED",
            Some(Stage::RuntimeLaunch) => "BRIDGE_RUNTIME_LAUNCH_FAILED",
            Some(Stage::RuntimeClose) => "BRIDGE_RUNTIME_CLOSE_FAILED",
            Some(Stage::GenerationSave) => "BRIDGE_GENERATION_SAVE_FAILED",
            Some(Stage::RuntimeAbort) => "BRIDGE_RUNTIME_ABORT_FAILED",
            None => "BRIDGE_PROFILE_OPERATION_FAILED",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_cache_error_keeps_owner_diagnostic_code() {
        use crate::operator_flow::{CleanupFailures, OperatorFlowError};
        let error = OperatorFlowError::AuthoritativeGeneration {
            reason: crate::authoritative_generation::AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence,
            cleanup: CleanupFailures::none(),
        };
        let diagnostic = UserDiagnostic::Launch(ShippingCompositionError::Operator(error));
        assert_eq!(diagnostic.code(), "BRIDGE_LOCAL_CACHE_INCOMPLETE");
    }

    #[test]
    fn authoritative_error_codes_preserve_known_causes_without_sensitive_details() {
        use crate::authoritative_generation::AuthoritativeGenerationMaterializationError as Reason;
        use crate::operator_flow::{CleanupFailures, OperatorFlowError};
        let cases = [
            (Reason::LocalAccessDenied, "BRIDGE_LOCAL_CACHE_INACCESSIBLE"),
            (Reason::Download(crate::generation_reopen::GenerationObjectDownloadError::Transport), "BRIDGE_GENERATION_OBJECT_TRANSPORT_FAILED"),
            (Reason::Download(crate::generation_reopen::GenerationObjectDownloadError::HttpStatus), "BRIDGE_GENERATION_OBJECT_HTTP_REJECTED"),
            (Reason::ReadBeforeLocalUse(crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(crate::generation_reopen::GenerationObjectDownloadError::Transport)), "BRIDGE_GENERATION_OBJECT_TRANSPORT_FAILED"),
            (Reason::ReadBeforeLocalUse(crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(crate::generation_reopen::GenerationObjectDownloadError::SizeMismatch)), "BRIDGE_GENERATION_INTEGRITY_FAILED"),
            (
                Reason::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::Transport,
                ),
                "BRIDGE_SERVICE_UNREACHABLE",
            ),
            (
                Reason::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                ),
                "BRIDGE_GENERATION_REQUEST_REJECTED",
            ),
            (
                Reason::ControlPlane(
                    crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                ),
                "BRIDGE_GENERATION_RESPONSE_INVALID",
            ),
            (
                Reason::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::HttpStatus,
                ),
                "BRIDGE_GENERATION_REQUEST_REJECTED",
            ),
            (
                Reason::ControlPlaneBeforeLocalUse(
                    crate::generation_reopen::GenerationReopenControlError::InvalidResponse,
                ),
                "BRIDGE_GENERATION_RESPONSE_INVALID",
            ),
            (
                Reason::MetadataMismatch,
                "BRIDGE_GENERATION_INTEGRITY_FAILED",
            ),
        ];
        for (reason, expected) in cases {
            let diagnostic = UserDiagnostic::Launch(ShippingCompositionError::Operator(
                OperatorFlowError::AuthoritativeGeneration {
                    reason,
                    cleanup: CleanupFailures::none(),
                },
            ));
            assert_eq!(diagnostic.code(), expected);
            assert!(!diagnostic.message().contains("tenant_id="));
            assert!(!diagnostic.message().contains("response body"));
        }
    }

    #[test]
    fn operator_stage_is_preserved_in_safe_user_code() {
        use crate::operator_flow::{OperatorFailureStage, OperatorFlowError};
        let owner = OperatorFlowError::Stage(OperatorFailureStage::CoordinatorClaim);
        let projected = ShippingCompositionError::Operator(owner);
        let diagnostic = UserDiagnostic::Launch(projected);
        assert_eq!(diagnostic.code(), "BRIDGE_COORDINATOR_CLAIM_FAILED");
        assert!(diagnostic.message().contains("Keep local profile files"));
        assert_ne!(
            diagnostic.code(),
            UserDiagnostic::Launch(ShippingCompositionError::CloseObservation(
                bridge_domain::BridgePortError::Unavailable
            ),)
            .code()
        );
    }

    #[test]
    fn invalid_package_and_first_identity_failure_have_distinct_actionable_codes() {
        use crate::generation_initialization::InitialProfileError;
        let package = UserDiagnostic::Launch(ShippingCompositionError::Initialization(
            InitialProfileError::RuntimeBundle,
        ));
        let identity = UserDiagnostic::Launch(ShippingCompositionError::Initialization(
            InitialProfileError::Runtime,
        ));
        assert_eq!(package.code(), "BRIDGE_RUNTIME_BUNDLE_INVALID");
        assert_eq!(identity.code(), "BRIDGE_FIRST_OPEN_RUNTIME_FAILED");
        assert!(
            package
                .message()
                .contains("Reconnecting this computer will not repair")
        );
        assert!(identity.message().contains("do not reconnect or reset"));
    }

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
