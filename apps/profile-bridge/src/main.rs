#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;
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
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let uri = arguments.next().ok_or(BridgeCliError::MissingClaimUri)?;
    if arguments.next().is_some() {
        return Err(BridgeCliError::UnexpectedArgument);
    }
    ClaimUri::parse(&uri).map_err(|_| BridgeCliError::InvalidClaimUri)?;
    Err(BridgeCliError::CompositionUnavailable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeCliError {
    MissingClaimUri,
    UnexpectedArgument,
    InvalidClaimUri,
    CompositionUnavailable,
}

impl fmt::Display for BridgeCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingClaimUri => "a single Profile Bridge claim URI is required",
            Self::UnexpectedArgument => "unexpected additional argument",
            Self::InvalidClaimUri => "claim URI is invalid",
            Self::CompositionUnavailable => {
                "authorized Profile Bridge composition is unavailable; launch refused"
            }
        })
    }
}

impl std::error::Error for BridgeCliError {}

#[cfg(test)]
mod tests {
    use super::{BridgeCliError, run};

    #[test]
    fn valid_claim_never_reports_success_before_authorized_composition_exists() {
        let result = run([
            "profile-bridge".to_owned(),
            "profilebridge://claim/claim_01JBRIDGE_FEASIBILITY".to_owned(),
        ]);
        assert_eq!(result, Err(BridgeCliError::CompositionUnavailable));
        assert!(
            !BridgeCliError::CompositionUnavailable
                .to_string()
                .contains("01JBRIDGE")
        );
    }

    #[test]
    fn invalid_cli_input_returns_generic_error() {
        let error = run([
            "profile-bridge".to_owned(),
            "profilebridge://claim/secret?leak=true".to_owned(),
        ]);
        assert_eq!(error, Err(BridgeCliError::InvalidClaimUri));
    }
}
