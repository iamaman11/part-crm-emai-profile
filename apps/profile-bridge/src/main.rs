#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;
#[cfg(windows)]
use profile_bridge::local_profile::{DeliveryActivationGuard, MaterializationRoot};
use profile_bridge::shipping_composition::run_claim;
#[cfg(windows)]
use profile_bridge::windows_delivery_handoff::{
    DeliveryHandoffCoordinator, DeliveryHandoffRestartDisposition,
};
use profile_bridge::windows_delivery_handoff::{
    HANDOFF_ACTIVATE_ARGUMENT, HANDOFF_ARRIVAL_ARGUMENT,
};
use std::env;
use std::fmt;
#[cfg(windows)]
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(windows)]
const MATERIALIZATION_ROOT_ENV: &str = "PROFILE_BRIDGE_MATERIALIZATION_ROOT";

fn main() -> ExitCode {
    match run(env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run<I>(arguments: I) -> Result<(), BridgeCliError>
where
    I: IntoIterator<Item = String>,
{
    match parse_command(arguments)? {
        BridgeCommand::Claim(claim) => run_claim_after_delivery_recovery(&claim),
        BridgeCommand::DeliveryActivateStaged => run_delivery_activate_staged(),
        BridgeCommand::DeliveryHandoffArrived => run_delivery_handoff_arrived(),
    }
}

enum BridgeCommand {
    Claim(ClaimUri),
    DeliveryActivateStaged,
    DeliveryHandoffArrived,
}

fn parse_command<I>(arguments: I) -> Result<BridgeCommand, BridgeCliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let argument = arguments.next().ok_or(BridgeCliError::MissingClaimUri)?;
    if arguments.next().is_some() {
        return Err(BridgeCliError::UnexpectedArgument);
    }
    match argument.as_str() {
        HANDOFF_ACTIVATE_ARGUMENT => Ok(BridgeCommand::DeliveryActivateStaged),
        HANDOFF_ARRIVAL_ARGUMENT => Ok(BridgeCommand::DeliveryHandoffArrived),
        _ => ClaimUri::parse(&argument)
            .map(BridgeCommand::Claim)
            .map_err(|_| BridgeCliError::InvalidClaimUri),
    }
}

fn run_claim_after_delivery_recovery(claim: &ClaimUri) -> Result<(), BridgeCliError> {
    #[cfg(windows)]
    {
        let (coordinator, current_executable) = delivery_coordinator()?;
        if coordinator
            .restart_recovery_pending()
            .map_err(|_| BridgeCliError::DeliveryFailed)?
        {
            let guard = delivery_activation_guard()?;
            let disposition = coordinator
                .recover_or_resume_started(&guard, std::process::id(), &current_executable)
                .map_err(|_| BridgeCliError::DeliveryFailed)?;
            match disposition {
                DeliveryHandoffRestartDisposition::None => drop(guard),
                DeliveryHandoffRestartDisposition::TransferScheduled => {
                    hold_activation_guard_until_process_exit(guard);
                    return Err(BridgeCliError::DeliveryRestartScheduled);
                }
                DeliveryHandoffRestartDisposition::RecoveryRequired => {
                    return Err(BridgeCliError::DeliveryRecoveryRequired);
                }
            }
        }
    }
    run_claim(claim).map_err(|_| BridgeCliError::LaunchFailed)
}

fn run_delivery_activate_staged() -> Result<(), BridgeCliError> {
    #[cfg(windows)]
    {
        let (coordinator, current_executable) = delivery_coordinator()?;
        let guard = delivery_activation_guard()?;
        coordinator
            .start_activation(&guard, std::process::id(), &current_executable)
            .map_err(|_| BridgeCliError::DeliveryFailed)?;
        hold_activation_guard_until_process_exit(guard);
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err(BridgeCliError::UnsupportedDeliveryCommand)
    }
}

fn run_delivery_handoff_arrived() -> Result<(), BridgeCliError> {
    #[cfg(windows)]
    {
        let (coordinator, current_executable) = delivery_coordinator()?;
        let guard = delivery_activation_guard()?;
        coordinator
            .complete_arrival(&guard, &current_executable)
            .map_err(|_| BridgeCliError::DeliveryFailed)?;
        hold_activation_guard_until_process_exit(guard);
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err(BridgeCliError::UnsupportedDeliveryCommand)
    }
}

#[cfg(windows)]
fn delivery_coordinator() -> Result<(DeliveryHandoffCoordinator, PathBuf), BridgeCliError> {
    let current_executable = env::current_exe().map_err(|_| BridgeCliError::DeliveryFailed)?;
    let coordinator = DeliveryHandoffCoordinator::from_current_executable(&current_executable)
        .map_err(|_| BridgeCliError::DeliveryFailed)?;
    Ok((coordinator, current_executable))
}

#[cfg(windows)]
fn delivery_activation_guard() -> Result<DeliveryActivationGuard, BridgeCliError> {
    let root = env::var_os(MATERIALIZATION_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(BridgeCliError::DeliveryFailed)?;
    if !root.is_absolute() {
        return Err(BridgeCliError::DeliveryFailed);
    }
    let materialization =
        MaterializationRoot::open_or_create(root).map_err(|_| BridgeCliError::DeliveryFailed)?;
    DeliveryActivationGuard::acquire(&materialization).map_err(|_| BridgeCliError::DeliveryFailed)
}

#[cfg(windows)]
fn hold_activation_guard_until_process_exit(guard: DeliveryActivationGuard) {
    // Successful handoff paths return directly from `main`. Intentionally retaining the OS-backed
    // exclusive admission handle until process termination prevents a new Profile Bridge writer from
    // entering between the durable handoff snapshot and the old process actually disappearing.
    std::mem::forget(guard);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeCliError {
    MissingClaimUri,
    UnexpectedArgument,
    InvalidClaimUri,
    LaunchFailed,
    #[cfg(windows)]
    DeliveryFailed,
    #[cfg(windows)]
    DeliveryRestartScheduled,
    #[cfg(windows)]
    DeliveryRecoveryRequired,
    UnsupportedDeliveryCommand,
}

impl fmt::Display for BridgeCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingClaimUri => {
                "a single Profile Bridge claim URI or delivery command is required"
            }
            Self::UnexpectedArgument => "unexpected additional argument",
            Self::InvalidClaimUri => "claim URI is invalid",
            Self::LaunchFailed => "authorized Profile Bridge launch failed closed",
            #[cfg(windows)]
            Self::DeliveryFailed => "Profile Bridge delivery handoff failed closed",
            #[cfg(windows)]
            Self::DeliveryRestartScheduled => {
                "Profile Bridge delivery recovery was scheduled; retry after handoff"
            }
            #[cfg(windows)]
            Self::DeliveryRecoveryRequired => "Profile Bridge delivery recovery is required",
            Self::UnsupportedDeliveryCommand => "Profile Bridge delivery commands require Windows",
        })
    }
}

impl std::error::Error for BridgeCliError {}

#[cfg(test)]
mod tests {
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
    fn delivery_commands_are_bounded_and_carry_no_claim() -> Result<(), Box<dyn std::error::Error>>
    {
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
        assert!(!BridgeCliError::LaunchFailed.to_string().contains("claim_"));
    }
}
