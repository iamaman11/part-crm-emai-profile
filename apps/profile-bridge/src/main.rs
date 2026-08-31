#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;
use profile_bridge::shipping_composition::{
    ShippingDeliveryCommand, run_claim, run_delivery_command,
};
use profile_bridge::windows_delivery_handoff::{
    HANDOFF_ACTIVATE_ARGUMENT, HANDOFF_ARRIVAL_ARGUMENT,
};
use std::env;
use std::fmt;
use std::process::ExitCode;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeCliError {
    MissingClaimUri,
    UnexpectedArgument,
    InvalidClaimUri,
    LaunchFailed,
    DeliveryFailed,
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
