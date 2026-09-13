#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;
use profile_bridge::device_pairing::{
    DevicePairingCompleteUri, DevicePairingStartUri, PAIRING_COMPLETE_URI_PREFIX,
    PAIRING_START_URI_PREFIX, run_pairing_complete, run_pairing_start,
};
use profile_bridge::shipping_composition::run_claim;
use profile_bridge::shipping_composition::{ShippingDeliveryCommand, run_delivery_command};
use profile_bridge::windows_delivery_handoff::{
    HANDOFF_ACTIVATE_ARGUMENT, HANDOFF_ARRIVAL_ARGUMENT,
};
use std::env;
use std::fmt;
use std::process::ExitCode;
use zeroize::Zeroizing;

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
        BridgeCommand::Claim(claim) => run_claim(&claim).map_err(|_| BridgeCliError::LaunchFailed),
        BridgeCommand::PairingStart(uri) => {
            run_pairing_start(&uri).map_err(|_| BridgeCliError::PairingFailed)
        }
        BridgeCommand::PairingComplete(uri) => {
            run_pairing_complete(&uri).map_err(|_| BridgeCliError::PairingFailed)
        }
        BridgeCommand::DeliveryActivateStaged => {
            run_delivery_command(ShippingDeliveryCommand::ActivateStaged)
                .map_err(|_| BridgeCliError::DeliveryFailed)
        }
        BridgeCommand::DeliveryHandoffArrived => {
            run_delivery_command(ShippingDeliveryCommand::HandoffArrived)
                .map_err(|_| BridgeCliError::DeliveryFailed)
        }
    }
}

enum BridgeCommand {
    Claim(ClaimUri),
    PairingStart(DevicePairingStartUri),
    PairingComplete(DevicePairingCompleteUri),
    DeliveryActivateStaged,
    DeliveryHandoffArrived,
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
    if arguments.next().is_some() {
        return Err(BridgeCliError::UnexpectedArgument);
    }
    match argument.as_str() {
        HANDOFF_ACTIVATE_ARGUMENT => Ok(BridgeCommand::DeliveryActivateStaged),
        HANDOFF_ARRIVAL_ARGUMENT => Ok(BridgeCommand::DeliveryHandoffArrived),
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
    MissingLaunchArgument,
    UnexpectedArgument,
    InvalidClaimUri,
    InvalidPairingUri,
    LaunchFailed,
    PairingFailed,
    DeliveryFailed,
}

impl fmt::Display for BridgeCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingLaunchArgument => {
                "a single Profile Bridge launch URI or delivery command is required"
            }
            Self::UnexpectedArgument => "unexpected additional argument",
            Self::InvalidClaimUri => "claim URI is invalid",
            Self::InvalidPairingUri => "device pairing URI is invalid",
            Self::LaunchFailed => "authorized Profile Bridge launch failed closed",
            Self::PairingFailed => "device pairing failed closed",
            Self::DeliveryFailed => "Profile Bridge delivery handoff failed closed",
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
        assert!(!BridgeCliError::PairingFailed.to_string().contains(&pairing));
        assert!(
            !BridgeCliError::PairingFailed
                .to_string()
                .contains(&challenge)
        );
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
        assert!(
            !BridgeCliError::DeliveryFailed
                .to_string()
                .contains("claim_")
        );
    }
}
