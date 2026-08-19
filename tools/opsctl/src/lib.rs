#![forbid(unsafe_code)]

mod cli;
pub mod credentials;
pub mod d1;
mod doctor;
mod error;
mod inventory;
pub mod promotion;
pub mod readiness;
pub mod recovery;
pub mod release;
mod repository;
mod status;

pub use cli::{CredentialsAction, HELP, Invocation, ReadCommand, parse_invocation};
pub use error::OpsctlError;

use repository::resolve_repo_root;

/// Execute one already-parsed project-specific operational policy command.
///
/// This function is the library composition root. Its operational policy authority
/// remains read-only and metadata-only: it does not own provider credentials,
/// hidden state, deployment scheduling, or runtime application logic.
pub fn execute(invocation: Invocation) -> Result<String, OpsctlError> {
    match invocation {
        Invocation::Help => Ok(HELP.to_owned()),
        Invocation::Version => Ok(format!("opsctl {}\n", env!("CARGO_PKG_VERSION"))),
        Invocation::Run { root, command } => {
            let repo_root = resolve_repo_root(root.as_deref(), command.name())?;
            match command {
                ReadCommand::Doctor => doctor::run(&repo_root),
                ReadCommand::Status => status::run(&repo_root),
                ReadCommand::Inventory => inventory::run(&repo_root),
            }
        }
        Invocation::Credentials { root, action } => {
            let repo_root = resolve_repo_root(root.as_deref(), "credentials")?;
            match action {
                CredentialsAction::Status => credentials::lifecycle(&repo_root),
                CredentialsAction::RotationPlan => credentials::rotation_plan(&repo_root),
            }
        }
        Invocation::D1 {
            root,
            action,
            component,
            ledger_json,
            release_manifest,
            current_manifest,
            known_good_manifest,
            preconditions_json,
            authority,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "d1")?;
            d1::run(d1::D1RunRequest {
                root: &repo_root,
                action,
                component: &component,
                ledger_json: &ledger_json,
                release_manifest: release_manifest.as_deref(),
                current_manifest: current_manifest.as_deref(),
                known_good_manifest: known_good_manifest.as_deref(),
                preconditions_json: preconditions_json.as_deref(),
                authority_path: authority.as_deref(),
            })
            .map_err(|error| OpsctlError::new("d1", error.to_string()))
        }
        Invocation::Release {
            root,
            action,
            release_set,
            artifact_root,
            profile_id,
            environment,
            evidence_json,
            current_release_set,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "release")?;
            release::commands::run(release::commands::ReleaseRunRequest {
                root: &repo_root,
                action,
                release_set: &release_set,
                artifact_root: artifact_root.as_deref(),
                profile_id: profile_id.as_deref(),
                environment: environment.as_deref(),
                evidence_json: evidence_json.as_deref(),
                current_release_set: current_release_set.as_deref(),
            })
            .map_err(|error| OpsctlError::new("release", error.to_string()))
        }

        Invocation::Promotion {
            root,
            action,
            release_set,
            profile_id,
            environment,
            snapshot,
            evidence_json,
            current_release_set,
            known_good_release_set,
            expected_current_release_set_id,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "promotion")?;
            promotion::commands::run(promotion::commands::PromotionRunRequest {
                root: &repo_root,
                action,
                release_set: &release_set,
                profile_id: &profile_id,
                environment: &environment,
                snapshot: &snapshot,
                evidence_json: &evidence_json,
                current_release_set: current_release_set.as_deref(),
                known_good_release_set: known_good_release_set.as_deref(),
                expected_current_release_set_id: expected_current_release_set_id.as_deref(),
            })
            .map_err(|error| OpsctlError::new("promotion", error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialsAction, Invocation, OpsctlError, execute};
    use std::path::PathBuf;

    #[test]
    fn credentials_status_preserves_lifecycle_metadata_contract() -> Result<(), OpsctlError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = execute(Invocation::Credentials {
            root: Some(root),
            action: CredentialsAction::Status,
        })?;
        assert!(output.contains("\"kind\": \"CREDENTIAL_LIFECYCLE_AUTHORITY\""));
        assert!(output.contains("\"routine_release_rotates_runtime_secrets\": false"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }

    #[test]
    fn credentials_rotation_plan_preserves_metadata_only_operator_contract()
    -> Result<(), OpsctlError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = execute(Invocation::Credentials {
            root: Some(root),
            action: CredentialsAction::RotationPlan,
        })?;
        assert!(output.contains("\"kind\": \"OPERATOR_CONTRACT_AUTHORITY\""));
        assert!(output.contains("\"mode\": \"READ_ONLY_METADATA_ONLY\""));
        assert!(output.contains("\"production_mutation\": false"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }
}
