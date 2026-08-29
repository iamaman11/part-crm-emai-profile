#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;
use profile_bridge::shipping_composition::run_claim;
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
    let claim = parse_claim(arguments)?;
    run_claim(&claim).map_err(|_| BridgeCliError::LaunchFailed)
}

fn parse_claim<I>(arguments: I) -> Result<ClaimUri, BridgeCliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let uri = arguments.next().ok_or(BridgeCliError::MissingClaimUri)?;
    if arguments.next().is_some() {
        return Err(BridgeCliError::UnexpectedArgument);
    }
    ClaimUri::parse(&uri).map_err(|_| BridgeCliError::InvalidClaimUri)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeCliError {
    MissingClaimUri,
    UnexpectedArgument,
    InvalidClaimUri,
    LaunchFailed,
}

impl fmt::Display for BridgeCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingClaimUri => "a single Profile Bridge claim URI is required",
            Self::UnexpectedArgument => "unexpected additional argument",
            Self::InvalidClaimUri => "claim URI is invalid",
            Self::LaunchFailed => "authorized Profile Bridge launch failed closed",
        })
    }
}

impl std::error::Error for BridgeCliError {}

#[cfg(test)]
mod tests {
    use super::{BridgeCliError, parse_claim};

    #[test]
    fn valid_claim_crosses_only_the_cli_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let claim = parse_claim([
            "profile-bridge".to_owned(),
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
        ])?;
        assert_eq!(claim.scheme(), "profilebridge");
        Ok(())
    }

    #[test]
    fn invalid_cli_input_returns_generic_error() {
        let error = parse_claim([
            "profile-bridge".to_owned(),
            "profilebridge://claim/secret?leak=true".to_owned(),
        ]);
        assert_eq!(error, Err(BridgeCliError::InvalidClaimUri));
        assert!(!BridgeCliError::InvalidClaimUri.to_string().contains("secret"));
        assert!(!BridgeCliError::LaunchFailed.to_string().contains("claim_"));
    }
}
