//! Safe user-facing explanations of existing native owner outcomes.
//!
//! Static text only: raw errors, URLs, claims, keys, responses and local paths cannot enter this API.

use crate::device_pairing::DevicePairingBootstrapError;
use crate::shipping_composition::ShippingCompositionError;
#[cfg(any(windows, test))]
use crate::shipping_control_plane::ShippingControlPlaneError;
#[cfg(any(windows, test))]
use crate::shipping_generation_successor_control::RestartExactCommitObservation;

/// Read-only projection of existing exact checkpoint / authoritative observe
/// facts. Never persists a parallel profile state, creates an authorization,
/// changes a file, or permits a network write.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PendingSaveReadOnlyDiagnosis {
    ServerActiveExactLocalUnresolved,
    ServerNotActiveExactRetryUnresolved,
    CanonicalCheckpointNotFound,
    LocalUnsyncedAwaitingServerDecision,
    UnknownPreserveBytes,
}

#[cfg(any(windows, test))]
impl PendingSaveReadOnlyDiagnosis {
    /// Read-only, locally validated offline close; never a retry grant.
    pub(crate) const fn local_unsynced() -> Self {
        Self::LocalUnsyncedAwaitingServerDecision
    }

    pub(crate) fn from_observation(
        observed: Result<RestartExactCommitObservation, ShippingControlPlaneError>,
    ) -> Self {
        match observed {
            Ok(RestartExactCommitObservation::ActiveExact) => {
                Self::ServerActiveExactLocalUnresolved
            }
            Ok(RestartExactCommitObservation::NotActiveExact) => {
                Self::ServerNotActiveExactRetryUnresolved
            }
            Ok(RestartExactCommitObservation::NoCanonicalCheckpoint) => {
                Self::CanonicalCheckpointNotFound
            }
            Err(_) => Self::UnknownPreserveBytes,
        }
    }

    #[must_use]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::ServerActiveExactLocalUnresolved => {
                "P3_RECOVERY_OBSERVATION=ACTIVE_EXACT;LOCAL_READINESS=UNRESOLVED"
            }
            Self::ServerNotActiveExactRetryUnresolved => {
                "P3_RECOVERY_OBSERVATION=NOT_ACTIVE_EXACT;RETRY_AUTHORITY=UNRESOLVED"
            }
            Self::CanonicalCheckpointNotFound => "P3_RECOVERY_OBSERVATION=NO_CANONICAL_CHECKPOINT",
            Self::LocalUnsyncedAwaitingServerDecision => {
                "P3_RECOVERY=LOCAL_UNSYNCED_VERIFIED;CLOUD_SAVE=NOT_CONFIRMED"
            }
            Self::UnknownPreserveBytes => "P3_RECOVERY_OBSERVATION=UNKNOWN;BYTES_RETAINED",
        }
    }

    /// Safe diagnostic recommendation, NEVER an executable transition or
    /// grant. Existing Bridge / Coordinator / D1 owners must still prove it.
    #[must_use]
    pub(crate) const fn next_safe_step(self) -> &'static str {
        match self {
            Self::ServerActiveExactLocalUnresolved => "VERIFY_OWNED_LOCAL_COMPLETION",
            Self::ServerNotActiveExactRetryUnresolved => "REQUIRE_FRESH_SERVER_RETRY_DECISION",
            Self::CanonicalCheckpointNotFound => "RECHECK_PENDING_LOCAL_EVIDENCE",
            Self::LocalUnsyncedAwaitingServerDecision => "REQUIRE_FRESH_SERVER_RETRY_DECISION",
            Self::UnknownPreserveBytes => "RETRY_READ_ONLY_OBSERVATION",
        }
    }
}

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
                ShippingCompositionError::HeartbeatAuthorityUncertain => {
                    "BRIDGE_HEARTBEAT_RECOVERY_REQUIRED"
                }
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
            Self::Launch(Launch::HeartbeatAuthorityUncertain) => {
                "Connection to the session coordinator could not be confirmed. The browser was closed and its local changes were preserved, but cloud save has not been confirmed. Restore connectivity and review this profile's recovery status; do not delete or replace its files."
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
            Self::Launch(Launch::Operator(
                crate::operator_flow::OperatorFlowError::GenerationSave { reason },
            )) => match reason {
                crate::operator_flow::GenerationSaveFailureReason::ControlTransport
                | crate::operator_flow::GenerationSaveFailureReason::ControlEffectUncertain
                | crate::operator_flow::GenerationSaveFailureReason::UploadOutcomeUncertain
                | crate::operator_flow::GenerationSaveFailureReason::VerificationUnproven => {
                    "The cloud save outcome is not confirmed. Original local profile bytes and the exact save attempt must be retained. Restore connectivity and use the authorized recovery path; never start a new save or delete files to resolve an uncertain reply."
                }
                crate::operator_flow::GenerationSaveFailureReason::ControlRejected => {
                    "The server rejected this save request. Check Profile Operations for current access and session authority. This is not evidence of a network outage or cloud success. Keep the local profile and pending save bytes."
                }
                crate::operator_flow::GenerationSaveFailureReason::SnapshotTooLarge => {
                    "The local browser profile exceeds this installed save implementation's size limit. Changes remain local, not cloud-saved. Update Profile Bridge when supported; do not delete cookies or profile files to reduce the size."
                }
                crate::operator_flow::GenerationSaveFailureReason::LocalIntentWrite
                | crate::operator_flow::GenerationSaveFailureReason::LocalSource
                | crate::operator_flow::GenerationSaveFailureReason::LocalPreparation
                | crate::operator_flow::GenerationSaveFailureReason::CheckpointWrite
                | crate::operator_flow::GenerationSaveFailureReason::CheckpointRetirement => {
                    "Windows could not durably complete the local save evidence or cleanup. Cloud completion is not established by this error. Check disk availability and access from Profile Operations; preserve the original profile and all recovery files."
                }
                crate::operator_flow::GenerationSaveFailureReason::NativeSourceChanged
                | crate::operator_flow::GenerationSaveFailureReason::CheckpointIntegrity
                | crate::operator_flow::GenerationSaveFailureReason::CandidateIdentity
                | crate::operator_flow::GenerationSaveFailureReason::DescriptorMismatch => {
                    "The save identity, original browser bytes or encrypted checkpoint did not match expected evidence. Save was blocked to avoid overwriting user data. Use the supported recovery path; do not delete, rename or repair profile files manually."
                }
                crate::operator_flow::GenerationSaveFailureReason::SealingMaterial
                | crate::operator_flow::GenerationSaveFailureReason::Encryption => {
                    "The profile could not be safely encrypted for cloud save. Original browser data is retained; the cloud save is not confirmed. Check the connected computer and recovery status without resetting device keys."
                }
                crate::operator_flow::GenerationSaveFailureReason::ControlInvalidResponse
                | crate::operator_flow::GenerationSaveFailureReason::ControlClock
                | crate::operator_flow::GenerationSaveFailureReason::OwnerState => {
                    "The save could not establish trustworthy operation or server-response evidence. Existing data is retained; no successful cloud save is claimed. Review Profile Operations before retrying."
                }
            },
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
        Error::GenerationSave { reason } => reason.code(),
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
    fn verified_offline_intent_diagnosis_never_claims_cloud_saved() {
        let status = PendingSaveReadOnlyDiagnosis::local_unsynced();
        assert_eq!(
            status.code(),
            "P3_RECOVERY=LOCAL_UNSYNCED_VERIFIED;CLOUD_SAVE=NOT_CONFIRMED"
        );
        assert_eq!(
            status.next_safe_step(),
            "REQUIRE_FRESH_SERVER_RETRY_DECISION"
        );
        assert!(!status.code().contains("CLOUD_SAVED"));
    }

    #[test]
    fn pending_save_observation_is_only_read_only_evidence_and_never_retry_authority() {
        let cases = [
            (
                Ok(RestartExactCommitObservation::ActiveExact),
                "P3_RECOVERY_OBSERVATION=ACTIVE_EXACT;LOCAL_READINESS=UNRESOLVED",
                "VERIFY_OWNED_LOCAL_COMPLETION",
            ),
            (
                Ok(RestartExactCommitObservation::NotActiveExact),
                "P3_RECOVERY_OBSERVATION=NOT_ACTIVE_EXACT;RETRY_AUTHORITY=UNRESOLVED",
                "REQUIRE_FRESH_SERVER_RETRY_DECISION",
            ),
            (
                Ok(RestartExactCommitObservation::NoCanonicalCheckpoint),
                "P3_RECOVERY_OBSERVATION=NO_CANONICAL_CHECKPOINT",
                "RECHECK_PENDING_LOCAL_EVIDENCE",
            ),
            (
                Err(ShippingControlPlaneError::HttpServerUncertain),
                "P3_RECOVERY_OBSERVATION=UNKNOWN;BYTES_RETAINED",
                "RETRY_READ_ONLY_OBSERVATION",
            ),
            (
                Err(ShippingControlPlaneError::HttpStatus),
                "P3_RECOVERY_OBSERVATION=UNKNOWN;BYTES_RETAINED",
                "RETRY_READ_ONLY_OBSERVATION",
            ),
        ];
        for (observed, expected_code, expected_step) in cases {
            let diagnostic = PendingSaveReadOnlyDiagnosis::from_observation(observed);
            assert_eq!(diagnostic.code(), expected_code);
            assert_eq!(diagnostic.next_safe_step(), expected_step);
            assert!(!diagnostic.next_safe_step().contains("UPLOAD"));
            assert!(!diagnostic.next_safe_step().contains("COMMIT"));
            assert!(!diagnostic.next_safe_step().contains("CLOUD_SAVED"));
        }
    }

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
            (
                Reason::Download(
                    crate::generation_reopen::GenerationObjectDownloadError::Transport,
                ),
                "BRIDGE_GENERATION_OBJECT_TRANSPORT_FAILED",
            ),
            (
                Reason::Download(
                    crate::generation_reopen::GenerationObjectDownloadError::HttpStatus,
                ),
                "BRIDGE_GENERATION_OBJECT_HTTP_REJECTED",
            ),
            (
                Reason::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                        crate::generation_reopen::GenerationObjectDownloadError::Transport,
                    ),
                ),
                "BRIDGE_GENERATION_OBJECT_TRANSPORT_FAILED",
            ),
            (
                Reason::ReadBeforeLocalUse(
                    crate::authoritative_generation::AuthoritativeGenerationPreWriteError::Download(
                        crate::generation_reopen::GenerationObjectDownloadError::SizeMismatch,
                    ),
                ),
                "BRIDGE_GENERATION_INTEGRITY_FAILED",
            ),
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
    fn save_owner_causes_survive_projection_without_raw_server_or_browser_data() {
        use crate::operator_flow::{GenerationSaveFailureReason as Reason, OperatorFlowError};
        let cases = [
            (Reason::ControlTransport, "BRIDGE_SAVE_CONTROL_TRANSPORT_UNKNOWN"),
            (Reason::ControlEffectUncertain, "BRIDGE_SAVE_CONTROL_EFFECT_UNCERTAIN"),
            (Reason::ControlRejected, "BRIDGE_SAVE_CONTROL_REJECTED"),
            (Reason::UploadOutcomeUncertain, "BRIDGE_SAVE_UPLOAD_OUTCOME_UNCERTAIN"),
            (Reason::CheckpointIntegrity, "BRIDGE_SAVE_CHECKPOINT_INTEGRITY_FAILED"),
            (Reason::LocalIntentWrite, "BRIDGE_SAVE_LOCAL_INTENT_FAILED"),
            (Reason::SnapshotTooLarge, "BRIDGE_SAVE_SNAPSHOT_TOO_LARGE"),
            (Reason::NativeSourceChanged, "BRIDGE_SAVE_NATIVE_SOURCE_CHANGED"),
            (Reason::SealingMaterial, "BRIDGE_SAVE_SEALING_MATERIAL_UNAVAILABLE"),
            (Reason::VerificationUnproven, "BRIDGE_SAVE_SERVER_VERIFY_NOT_PROVEN"),
        ];
        for (reason, code) in cases {
            let owned = OperatorFlowError::GenerationSave { reason };
            assert_eq!(owned.stage(), Some(crate::operator_flow::OperatorFailureStage::GenerationSave));
            let diagnostic = UserDiagnostic::Launch(ShippingCompositionError::Operator(owned));
            assert_eq!(diagnostic.code(), code);
            assert!(!diagnostic.message().contains("http"));
            assert!(!diagnostic.message().contains("signed_url"));
            assert!(!diagnostic.message().contains("tenant_id"));
            assert!(!diagnostic.message().contains("password"));
            assert!(!diagnostic.message().contains("Saved in the cloud"));
        }
        let rejection = UserDiagnostic::Launch(ShippingCompositionError::Operator(
            OperatorFlowError::GenerationSave { reason: Reason::ControlRejected },
        ));
        assert!(rejection.message().contains("server rejected"));
        assert!(!rejection.message().contains("network outage"));
        let uncertain = UserDiagnostic::Launch(ShippingCompositionError::Operator(
            OperatorFlowError::GenerationSave { reason: Reason::UploadOutcomeUncertain },
        ));
        assert!(uncertain.message().contains("not confirmed"));
        assert!(uncertain.message().contains("exact save attempt"));
        let old = UserDiagnostic::Launch(ShippingCompositionError::Operator(
            OperatorFlowError::Stage(crate::operator_flow::OperatorFailureStage::GenerationSave),
        ));
        assert_eq!(old.code(), "BRIDGE_GENERATION_SAVE_FAILED");
    }

    #[test]
    fn committed_successor_is_not_reported_as_an_unsaved_failure() {
        let diagnosis = UserDiagnostic::Launch(ShippingCompositionError::CommittedRecoveryRequired);
        assert_eq!(diagnosis.code(), "BRIDGE_SAVED_LOCAL_RECOVERY_REQUIRED");
        assert!(diagnosis.message().contains("confirmed by the server"));
        assert!(diagnosis.message().contains("Keep local profile files"));
    }
}
