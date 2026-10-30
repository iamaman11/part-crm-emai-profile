#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]

use bridge_domain::ClaimUri;
use profile_bridge::device_pairing::{
    DevicePairingBootstrapError, DevicePairingCompleteUri, DevicePairingStartUri,
    PAIRING_COMPLETE_URI_PREFIX, PAIRING_START_URI_PREFIX, run_pairing_complete, run_pairing_start,
};
use profile_bridge::generation_initialization::{INITIAL_OPEN_URI_PREFIX, InitialProfileOpen};
use profile_bridge::installed_connection::ControlPlaneOrigin;
use profile_bridge::shipping_composition::run_claim;
use profile_bridge::shipping_composition::{
    ShippingCompositionError, ShippingDeliveryCommand, VERIFY_HISTORICAL_CANDIDATE_ARGUMENT,
    run_delivery_command, run_historical_candidate_probe,
};
use profile_bridge::user_diagnostics::UserDiagnostic;
use profile_bridge::user_diagnostics::report::{self, Cause, Cloud, Outcome, Stage};
use std::time::Instant;

const REPORT_COPY_URI: &str = "profilebridge://diagnostics/copy";
use profile_bridge::windows_delivery_handoff::{
    FIRST_INSTALL_ARGUMENT, HANDOFF_ACTIVATE_ARGUMENT, HANDOFF_ARRIVAL_ARGUMENT,
};
use profile_bridge::windows_delivery_install::{
    NativeInstallerError, PREPARE_INSTALL_ARGUMENT, PREPARED_DIRECTORY_PREFIX,
    S1_OBSERVE_ARGUMENT, run_install_preparation, run_s1_observation,
};
use std::env;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;
use zeroize::Zeroizing;

fn main() -> ExitCode {
    match run(env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // S1 outcomes are machine-readable and fail closed without showing a
            // desktop dialog or writing a normal runtime diagnostic report.
            if matches!(error, BridgeCliError::S1NotReady) {
                eprintln!("BRIDGE_S1_NOT_READY");
                return ExitCode::from(3);
            }
            if let BridgeCliError::LaunchFailed(ShippingCompositionError::Operator(owner)) = error {
                // This closed typed outcome contains no claims, paths, credentials or profile data.
                eprintln!("BRIDGE_OPERATOR_OUTCOME={owner:?}");
            }
            eprintln!("{error}");
            error.diagnostic().show_desktop();
            ExitCode::from(2)
        }
    }
}

fn run<I>(arguments: I) -> Result<(), BridgeCliError>
where
    I: IntoIterator<Item = String>,
{
    let command = parse_command(arguments)?;
    let installation_root = match &command {
        BridgeCommand::PrepareInstallation(_, root) => Some(root.clone()),
        _ => None,
    };
    let stage = match &command {
        BridgeCommand::PrepareInstallation(..) | BridgeCommand::ReadOnlyS1(..) => Stage::InstallPrepare,
        BridgeCommand::InitialProfile(_) | BridgeCommand::Claim(_) => Stage::OpenRequested,
        BridgeCommand::VerifyHistoricalCandidate(_) => Stage::LocalRecovery,
        BridgeCommand::PairingStart(_) | BridgeCommand::PairingComplete(_) => Stage::Pairing,
        BridgeCommand::DeliveryActivateStaged => Stage::InstallActivate,
        BridgeCommand::DeliveryInstallStaged(_) | BridgeCommand::DeliveryHandoffArrived => {
            Stage::Delivery
        }
        BridgeCommand::CopyDiagnostics => Stage::Delivery,
    };
    let report_operation = !matches!(&command, BridgeCommand::CopyDiagnostics | BridgeCommand::ReadOnlyS1(..));
    let observe = |outcome, cause, elapsed| {
        if let Some(root) = &installation_root {
            report::record_at(root, stage, outcome, Cloud::Unknown, cause, elapsed);
        } else {
            report::record_current(stage, outcome, Cloud::Unknown, cause, elapsed);
        }
    };
    if report_operation {
        observe(Outcome::Started, Cause::None, 0);
    }
    let start = Instant::now();
    let result = match command {
        BridgeCommand::ReadOnlyS1(bundle, root) => {
            let observation = run_s1_observation(&bundle, &root);
            let ready = serde_json::from_str::<serde_json::Value>(&observation)
                .ok()
                .and_then(|report| {
                    report.get("verdict")
                        .and_then(serde_json::Value::as_str)
                        .map(|verdict| verdict == "READY")
                })
                .unwrap_or(false);
            println!("{observation}");
            if ready {
                Ok(())
            } else {
                Err(BridgeCliError::S1NotReady)
            }
        }
        BridgeCommand::PrepareInstallation(bundle, root) => {
            let result = run_install_preparation(&bundle, &root);
            if let Ok(directory) = &result {
                println!("{PREPARED_DIRECTORY_PREFIX}{directory}");
            }
            // Capture the complete typed verdict for setup before enforcing the exit gate.
            // Human diagnostics stay separate and contain no paths or installer inputs.
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": 1,
                    "kind": "WINDOWS_PROFILE_BRIDGE_INSTALLATION_PREPARATION_OUTCOME",
                    "result": if result.is_ok() { "PREPARED" } else { "FAILED" },
                    "owner_verdict": result.as_ref().err().map(|error| format!("{error:?}")),
                    "activation_performed": false,
                    "health_verified": false,
                })
            );
            result
                .map(|_| ())
                .map_err(BridgeCliError::InstallationFailed)
        }
        BridgeCommand::InitialProfile(intent) => {
            profile_bridge::shipping_composition::run_initial_profile(&intent)
                .map_err(BridgeCliError::LaunchFailed)
        }
        BridgeCommand::Claim(claim) => run_claim(&claim).map_err(BridgeCliError::LaunchFailed),
        BridgeCommand::VerifyHistoricalCandidate(claim) => {
            run_historical_candidate_probe(&claim).map_err(BridgeCliError::LaunchFailed)
        }
        BridgeCommand::PairingStart(uri) => {
            run_pairing_start(&uri).map_err(BridgeCliError::PairingFailed)
        }
        BridgeCommand::PairingComplete(uri) => {
            run_pairing_complete(&uri).map_err(BridgeCliError::PairingFailed)
        }
        BridgeCommand::DeliveryActivateStaged => {
            run_delivery_command(ShippingDeliveryCommand::ActivateStaged)
                .map_err(BridgeCliError::DeliveryFailed)
        }
        BridgeCommand::DeliveryInstallStaged(origin) => {
            run_delivery_command(ShippingDeliveryCommand::InstallStaged(origin))
                .map_err(BridgeCliError::DeliveryFailed)
        }
        BridgeCommand::DeliveryHandoffArrived => {
            run_delivery_command(ShippingDeliveryCommand::HandoffArrived)
                .map_err(BridgeCliError::DeliveryFailed)
        }
        BridgeCommand::CopyDiagnostics => copy_diagnostics(),
    };
    if report_operation {
        let cause = match &result {
            Ok(()) => Cause::None,
            Err(BridgeCliError::InstallationFailed(_)) => Cause::InstallBlocked,
            Err(BridgeCliError::LaunchFailed(
                ShippingCompositionError::HeartbeatAuthorityUncertain,
            )) => Cause::CoordinatorUncertain,
            Err(BridgeCliError::LaunchFailed(
                ShippingCompositionError::CommittedRecoveryRequired,
            )) => Cause::CloudSaveFailed,
            Err(BridgeCliError::LaunchFailed(_)) => Cause::BrowserUnavailable,
            Err(_) => Cause::OtherSafeFailure,
        };
        let elapsed = u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .min(86_400_000);
        // A completed launch command is not proof of a currently open browser:
        // it may have completed delivery recovery or a controlled close/save.
        // Only the operator's live Ready and exact Save witnesses are positive.
        let outcome = match (&result, stage) {
            (Ok(()), Stage::OpenRequested) => Outcome::Observed,
            (Ok(()), _) => Outcome::Succeeded,
            (Err(_), _) => Outcome::Failed,
        };
        observe(outcome, cause, elapsed);
        if result.is_ok()
            && stage == Stage::InstallPrepare
            && let Some(root) = &installation_root
        {
            report::record_at(
                root,
                Stage::InstallVerified,
                Outcome::Observed,
                Cloud::Unknown,
                Cause::None,
                elapsed,
            );
        }
    }
    result
}

fn copy_diagnostics() -> Result<(), BridgeCliError> {
    #[cfg(windows)]
    {
        let text = report::report_current();
        if !windows_desktop_ui::copy_redacted_report(&text) {
            return Err(BridgeCliError::DiagnosticsUnavailable);
        }
        windows_desktop_ui::show_report_copied();
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(BridgeCliError::DiagnosticsUnavailable)
    }
}

enum BridgeCommand {
    PrepareInstallation(PathBuf, PathBuf),
    ReadOnlyS1(PathBuf, PathBuf),
    Claim(ClaimUri),
    VerifyHistoricalCandidate(ClaimUri),
    InitialProfile(InitialProfileOpen),
    PairingStart(DevicePairingStartUri),
    PairingComplete(DevicePairingCompleteUri),
    DeliveryActivateStaged,
    DeliveryInstallStaged(ControlPlaneOrigin),
    DeliveryHandoffArrived,
    CopyDiagnostics,
}

fn parse_command<I>(arguments: I) -> Result<BridgeCommand, BridgeCliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let argument = Zeroizing::new(
        arguments
            .next()
            .ok_or(BridgeCliError::MissingLaunchArgument)?,
    );
    if argument.as_str() == S1_OBSERVE_ARGUMENT {
        let bundle = PathBuf::from(
            arguments
                .next()
                .ok_or(BridgeCliError::InvalidInstallationArgument)?,
        );
        let root = PathBuf::from(
            arguments
                .next()
                .ok_or(BridgeCliError::InvalidInstallationArgument)?,
        );
        if arguments.next().is_some() || !bundle.is_absolute() || !root.is_absolute() {
            return Err(BridgeCliError::InvalidInstallationArgument);
        }
        return Ok(BridgeCommand::ReadOnlyS1(bundle, root));
    }
    if argument.as_str() == PREPARE_INSTALL_ARGUMENT {
        let bundle = PathBuf::from(
            arguments
                .next()
                .ok_or(BridgeCliError::InvalidInstallationArgument)?,
        );
        let root = PathBuf::from(
            arguments
                .next()
                .ok_or(BridgeCliError::InvalidInstallationArgument)?,
        );
        if arguments.next().is_some() {
            return Err(BridgeCliError::UnexpectedArgument);
        }
        if !bundle.is_absolute() || !root.is_absolute() {
            return Err(BridgeCliError::InvalidInstallationArgument);
        }
        return Ok(BridgeCommand::PrepareInstallation(bundle, root));
    }
    if argument.as_str() == VERIFY_HISTORICAL_CANDIDATE_ARGUMENT {
        let claim = Zeroizing::new(arguments.next().ok_or(BridgeCliError::InvalidClaimUri)?);
        if arguments.next().is_some() {
            return Err(BridgeCliError::UnexpectedArgument);
        }
        return ClaimUri::parse(&claim)
            .map(BridgeCommand::VerifyHistoricalCandidate)
            .map_err(|_| BridgeCliError::InvalidClaimUri);
    }
    if argument.as_str() == FIRST_INSTALL_ARGUMENT {
        let origin = arguments
            .next()
            .ok_or(BridgeCliError::InvalidInstallationArgument)?;
        if arguments.next().is_some() {
            return Err(BridgeCliError::UnexpectedArgument);
        }
        return ControlPlaneOrigin::parse(origin)
            .map(BridgeCommand::DeliveryInstallStaged)
            .map_err(|_| BridgeCliError::InvalidInstallationArgument);
    }
    if arguments.next().is_some() {
        return Err(BridgeCliError::UnexpectedArgument);
    }
    match argument.as_str() {
        HANDOFF_ACTIVATE_ARGUMENT => Ok(BridgeCommand::DeliveryActivateStaged),
        HANDOFF_ARRIVAL_ARGUMENT => Ok(BridgeCommand::DeliveryHandoffArrived),
        REPORT_COPY_URI => Ok(BridgeCommand::CopyDiagnostics),
        value if value.starts_with(INITIAL_OPEN_URI_PREFIX) => InitialProfileOpen::parse(value)
            .map(BridgeCommand::InitialProfile)
            .map_err(|_| BridgeCliError::InvalidClaimUri),
        value if value.starts_with(PAIRING_START_URI_PREFIX) => DevicePairingStartUri::parse(value)
            .map(BridgeCommand::PairingStart)
            .map_err(|_| BridgeCliError::InvalidPairingUri),
        value if value.starts_with(PAIRING_COMPLETE_URI_PREFIX) => {
            DevicePairingCompleteUri::parse(value)
                .map(BridgeCommand::PairingComplete)
                .map_err(|_| BridgeCliError::InvalidPairingUri)
        }
        _ => ClaimUri::parse(&argument)
            .map(BridgeCommand::Claim)
            .map_err(|_| BridgeCliError::InvalidClaimUri),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeCliError {
    InstallationFailed(NativeInstallerError),
    MissingLaunchArgument,
    UnexpectedArgument,
    InvalidClaimUri,
    InvalidPairingUri,
    InvalidInstallationArgument,
    LaunchFailed(ShippingCompositionError),
    PairingFailed(DevicePairingBootstrapError),
    DeliveryFailed(ShippingCompositionError),
    DiagnosticsUnavailable,
    S1NotReady,
}

impl BridgeCliError {
    const fn diagnostic(self) -> UserDiagnostic {
        match self {
            Self::InstallationFailed(error) => UserDiagnostic::Installation(error),
            Self::MissingLaunchArgument => UserDiagnostic::StartFromApplication,
            Self::DiagnosticsUnavailable | Self::S1NotReady => UserDiagnostic::ReportUnavailable,
            Self::UnexpectedArgument
            | Self::InvalidClaimUri
            | Self::InvalidPairingUri
            | Self::InvalidInstallationArgument => UserDiagnostic::InvalidHandoff,
            Self::LaunchFailed(error) | Self::DeliveryFailed(error) => {
                UserDiagnostic::Launch(error)
            }
            Self::PairingFailed(error) => UserDiagnostic::Pairing(error),
        }
    }
}

impl fmt::Display for BridgeCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diagnosis = self.diagnostic();
        write!(formatter, "{}: {}", diagnosis.code(), diagnosis.message())
    }
}

impl std::error::Error for BridgeCliError {}

#[cfg(test)]
mod tests {
    use super::FIRST_INSTALL_ARGUMENT;
    use super::{BridgeCliError, BridgeCommand, parse_command};
    use profile_bridge::windows_delivery_handoff::{
        HANDOFF_ACTIVATE_ARGUMENT, HANDOFF_ARRIVAL_ARGUMENT,
    };

    #[test]
    fn valid_claim_crosses_only_the_cli_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let BridgeCommand::Claim(claim) = parse_command([
            "profile-bridge".to_owned(),
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
        ])?
        else {
            return Err("expected claim command".into());
        };
        assert!(!format!("{claim:?}").contains("claim_01JBRIDGE_FEASIBILITY"));
        Ok(())
    }

    #[test]
    fn local_diagnostic_export_is_exact_and_has_no_remote_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_command([
            "profile-bridge".to_owned(),
            super::REPORT_COPY_URI.to_owned(),
        ])?;
        assert!(matches!(parsed, BridgeCommand::CopyDiagnostics));
        for invalid in [
            "profilebridge://diagnostics/copy?origin=https://untrusted.invalid",
            "profilebridge://diagnostics/copy/other",
            "profilebridge://diagnostics/export",
        ] {
            assert!(parse_command(["profile-bridge".to_owned(), invalid.to_owned()]).is_err());
        }
        assert!(
            parse_command([
                "profile-bridge".to_owned(),
                super::REPORT_COPY_URI.to_owned(),
                "unexpected".to_owned(),
            ])
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn pairing_commands_are_bounded_and_completion_tokens_are_redacted()
    -> Result<(), Box<dyn std::error::Error>> {
        let BridgeCommand::PairingStart(start) = parse_command([
            "profile-bridge".to_owned(),
            "profilebridge://pair/start/tenant_01JPAIR/device_01JPAIR".to_owned(),
        ])?
        else {
            return Err("expected pairing start command".into());
        };
        assert_eq!(start.device_id().as_str(), "device_01JPAIR");

        let pairing = "a".repeat(64);
        let challenge = "b".repeat(64);
        let nonce = "c".repeat(64);
        let callback = format!(
            "profilebridge://pair/complete/tenant_01JPAIR/actor_01JPAIR/device_01JPAIR/{pairing}/{challenge}/{nonce}/123456789"
        );
        let BridgeCommand::PairingComplete(complete) =
            parse_command(["profile-bridge".to_owned(), callback])?
        else {
            return Err("expected pairing completion command".into());
        };
        let debug = format!("{complete:?}");
        assert!(!debug.contains(&pairing));
        assert!(!debug.contains(&challenge));
        let error = BridgeCliError::PairingFailed(super::DevicePairingBootstrapError::Rejected);
        assert!(!error.to_string().contains(&pairing));
        assert!(!error.to_string().contains(&challenge));
        Ok(())
    }

    #[test]
    fn s1_read_only_command_requires_two_absolute_paths_without_extra_effect_args()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir();
        let bundle = tmp.join("new-signed-bundle").to_string_lossy().into_owned();
        let root = tmp.join("original-install").to_string_lossy().into_owned();
        let parsed = parse_command([
            "profile-bridge".to_owned(),
            super::S1_OBSERVE_ARGUMENT.to_owned(),
            bundle.clone(),
            root.clone(),
        ])?;
        assert!(matches!(parsed, BridgeCommand::ReadOnlyS1(..)));
        for args in [
            vec![super::S1_OBSERVE_ARGUMENT.to_owned(), bundle.clone()],
            vec![super::S1_OBSERVE_ARGUMENT.to_owned(), "relative".to_owned(), root.clone()],
            vec![super::S1_OBSERVE_ARGUMENT.to_owned(), bundle.clone(), "relative".to_owned()],
            vec![super::S1_OBSERVE_ARGUMENT.to_owned(), bundle, root, "--delivery-prepare".to_owned()],
        ] {
            assert!(parse_command(
                std::iter::once("profile-bridge".to_owned()).chain(args),
            ).is_err());
        }
        Ok(())
    }

    #[test]
    fn installer_preparation_requires_exactly_two_absolute_machine_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir();
        let bundle = directory
            .join("installer-bundle")
            .to_string_lossy()
            .into_owned();
        let root = directory
            .join("installed-bridge")
            .to_string_lossy()
            .into_owned();
        let prefix = [
            "profile-bridge",
            profile_bridge::windows_delivery_install::PREPARE_INSTALL_ARGUMENT,
        ];
        let mut valid = prefix.map(str::to_owned).to_vec();
        valid.extend([bundle.clone(), root.clone()]);
        assert!(matches!(
            parse_command(valid)?,
            BridgeCommand::PrepareInstallation(_, _)
        ));
        for suffix in [
            vec![],
            vec![bundle.clone()],
            vec!["relative".to_owned(), root.clone()],
            vec![
                bundle,
                root,
                "profilebridge://claim/private-capability".to_owned(),
            ],
        ] {
            let mut invalid = prefix.map(str::to_owned).to_vec();
            invalid.extend(suffix);
            assert!(parse_command(invalid).is_err());
        }
        Ok(())
    }

    #[test]
    fn installer_origin_is_explicit_and_cannot_be_a_handoff_uri() {
        for origin in [
            None,
            Some("http://control.example.com"),
            Some("profilebridge://claim/private_capability"),
            Some("https://control.example.com/path"),
        ] {
            let mut arguments = vec![
                "profile-bridge".to_owned(),
                FIRST_INSTALL_ARGUMENT.to_owned(),
            ];
            if let Some(value) = origin {
                arguments.push(value.to_owned());
            }
            assert_eq!(
                parse_command(arguments).err(),
                Some(BridgeCliError::InvalidInstallationArgument)
            );
        }
    }

    #[test]
    fn delivery_commands_are_bounded_and_carry_no_claim() -> Result<(), Box<dyn std::error::Error>>
    {
        assert!(matches!(
            parse_command([
                "profile-bridge".to_owned(),
                FIRST_INSTALL_ARGUMENT.to_owned(),
                "https://control.example.com".to_owned()
            ])?,
            BridgeCommand::DeliveryInstallStaged(_)
        ));
        assert_eq!(
            parse_command([
                "profile-bridge".to_owned(),
                FIRST_INSTALL_ARGUMENT.to_owned(),
                "https://control.example.com".to_owned(),
                "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
            ])
            .err(),
            Some(BridgeCliError::UnexpectedArgument)
        );
        assert!(matches!(
            parse_command([
                "profile-bridge".to_owned(),
                HANDOFF_ACTIVATE_ARGUMENT.to_owned()
            ])?,
            BridgeCommand::DeliveryActivateStaged
        ));
        assert!(matches!(
            parse_command([
                "profile-bridge".to_owned(),
                HANDOFF_ARRIVAL_ARGUMENT.to_owned()
            ])?,
            BridgeCommand::DeliveryHandoffArrived
        ));
        assert_eq!(
            parse_command([
                "profile-bridge".to_owned(),
                HANDOFF_ACTIVATE_ARGUMENT.to_owned(),
                "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
            ])
            .err(),
            Some(BridgeCliError::UnexpectedArgument)
        );
        assert_eq!(
            parse_command([
                "profile-bridge".to_owned(),
                HANDOFF_ARRIVAL_ARGUMENT.to_owned(),
                "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
            ])
            .err(),
            Some(BridgeCliError::UnexpectedArgument)
        );
        Ok(())
    }

    #[test]
    fn historical_candidate_probe_requires_fresh_exact_claim_without_extra_arguments()
    -> Result<(), Box<dyn std::error::Error>> {
        let command = [
            "profile-bridge".to_owned(),
            super::VERIFY_HISTORICAL_CANDIDATE_ARGUMENT.to_owned(),
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
        ];
        assert!(matches!(
            parse_command(command.clone())?,
            BridgeCommand::VerifyHistoricalCandidate(_)
        ));
        for invalid in [
            vec![command[0].clone(), command[1].clone()],
            vec![
                command[0].clone(),
                command[1].clone(),
                "not-a-claim".to_owned(),
            ],
            vec![
                command[0].clone(),
                command[1].clone(),
                command[2].clone(),
                "extra".to_owned(),
            ],
        ] {
            assert!(parse_command(invalid).is_err());
        }
        Ok(())
    }

    #[test]
    fn invalid_cli_input_returns_generic_error() {
        let error = parse_command([
            "profile-bridge".to_owned(),
            "profilebridge://claim/secret?leak=true".to_owned(),
        ]);
        assert_eq!(error.err(), Some(BridgeCliError::InvalidClaimUri));
        assert!(
            !BridgeCliError::InvalidClaimUri
                .to_string()
                .contains("secret")
        );
        assert!(
            !BridgeCliError::LaunchFailed(super::ShippingCompositionError::Operator(
                profile_bridge::operator_flow::OperatorFlowError::Stage(
                    profile_bridge::operator_flow::OperatorFailureStage::CoordinatorClaim,
                ),
            ))
            .to_string()
            .contains("claim_")
        );
        assert!(
            !BridgeCliError::DeliveryFailed(super::ShippingCompositionError::DeliveryHandoff)
                .to_string()
                .contains("claim_")
        );
    }
}
