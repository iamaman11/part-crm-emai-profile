#![forbid(unsafe_code)]

pub mod canonical;
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

use repository::{resolve_d1_repository_root, resolve_repo_root};

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
            })
            .map_err(|error| OpsctlError::new("d1", error.to_string()))
        }
        Invocation::D1Repository { root } => {
            let repo_root = resolve_d1_repository_root(root.as_deref())?;
            d1::repository_projection(&repo_root)
                .map_err(|error| OpsctlError::new("d1", error.to_string()))
        }
        Invocation::ReleaseFinalize { root, request_json } => {
            let repo_root = resolve_repo_root(root.as_deref(), "release")?;
            let input = std::fs::read_to_string(&request_json).map_err(|error| {
                OpsctlError::new(
                    "release",
                    format!(
                        "RELEASE_FINALIZE_REQUEST_UNAVAILABLE: {}: {error}",
                        request_json.display()
                    ),
                )
            })?;
            release::finalize::finalize_json(&repo_root, &input)
                .map_err(|error| OpsctlError::new("release", error.to_string()))
        }
        Invocation::Release {
            root,
            action,
            release_set,
            source_root,
            artifact_root,
            profile_id,
            environment,
            evidence_json,
            current_release_set,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "release")?;
            let release_source_root = source_root.as_deref().unwrap_or(&repo_root);
            release::commands::run(release::commands::ReleaseRunRequest {
                root: &repo_root,
                source_root: release_source_root,
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
            source_root,
            profile_id,
            environment,
            snapshot,
            evidence_json,
            current_release_set,
            known_good_release_set,
            expected_current_release_set_id,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "promotion")?;
            let release_source_root = source_root.as_deref().unwrap_or(&repo_root);
            promotion::commands::run(promotion::commands::PromotionRunRequest {
                root: &repo_root,
                source_root: release_source_root,
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
    use super::{CredentialsAction, Invocation, OpsctlError, execute, parse_invocation};
    use serde_json::Value;
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn parser_args(values: &[Value]) -> Result<Vec<OsString>, String> {
        let mut args = vec![OsString::from("opsctl")];
        for value in values {
            let argument = value
                .as_str()
                .ok_or_else(|| "operator parser probe arguments must be strings".to_owned())?;
            args.push(OsString::from(argument));
        }
        Ok(args)
    }

    fn invocation_command(invocation: &Invocation) -> Option<String> {
        match invocation {
            Invocation::Help | Invocation::Version => None,
            Invocation::Run { command, .. } => Some(format!("opsctl {}", command.name())),
            Invocation::Credentials { action, .. } => {
                Some(format!("opsctl credentials {}", action.name()))
            }
            Invocation::D1 { action, .. } => Some(format!("opsctl d1 {}", action.name())),
            Invocation::D1Repository { .. } => Some("opsctl d1 repository".to_owned()),
            Invocation::ReleaseFinalize { .. } => Some("opsctl release finalize".to_owned()),
            Invocation::Release { action, .. } => Some(format!("opsctl release {}", action.name())),
            Invocation::Promotion { action, .. } => {
                Some(format!("opsctl promotion {}", action.name()))
            }
        }
    }

    #[test]
    fn credentials_status_preserves_lifecycle_metadata_contract() -> Result<(), OpsctlError> {
        let output = execute(Invocation::Credentials {
            root: Some(repository_root()),
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
        let output = execute(Invocation::Credentials {
            root: Some(repository_root()),
            action: CredentialsAction::RotationPlan,
        })?;
        assert!(output.contains("\"kind\": \"OPERATOR_CONTRACT_AUTHORITY\""));
        assert!(output.contains("\"mode\": \"READ_ONLY_METADATA_ONLY\""));
        assert!(output.contains("\"production_mutation\": false"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }

    #[test]
    fn operator_registry_active_probes_match_parser_and_reserved_probes_fail_closed()
    -> Result<(), String> {
        let text =
            fs::read_to_string(repository_root().join("architecture/operator-contract.json"))
                .map_err(|error| format!("operator contract must be readable: {error}"))?;
        let authority: Value = serde_json::from_str(&text)
            .map_err(|error| format!("operator contract must be JSON: {error}"))?;
        let surfaces = authority["operator_surfaces"]
            .as_object()
            .ok_or_else(|| "operator_surfaces must be an object".to_owned())?;
        let mut active = BTreeSet::new();

        for (id, entry) in surfaces {
            assert_eq!(entry["status"].as_str(), Some("ACTIVE"), "{id}");
            assert_eq!(
                entry["mode"].as_str(),
                Some("READ_ONLY_METADATA_ONLY"),
                "{id}"
            );
            assert_eq!(entry["side_effects"].as_str(), Some("NONE"), "{id}");
            assert_eq!(entry["network_authority"].as_bool(), Some(false), "{id}");
            assert_eq!(
                entry["provider_mutation_authority"].as_bool(),
                Some(false),
                "{id}"
            );
            assert_eq!(entry["secret_readback"].as_bool(), Some(false), "{id}");

            let command = entry["command"]
                .as_str()
                .ok_or_else(|| format!("{id} active operator command must be a string"))?;
            assert!(
                active.insert(command.to_owned()),
                "duplicate command: {command}"
            );
            let probe = entry["parser_probe_args"]
                .as_array()
                .ok_or_else(|| format!("{id} active operator parser probe must be an array"))?;
            let invocation = parse_invocation(parser_args(probe)?)
                .map_err(|error| format!("{id} registry probe did not parse: {error}"))?;
            assert_eq!(
                invocation_command(&invocation).as_deref(),
                Some(command),
                "{id}"
            );
        }
        assert_eq!(active.len(), 17, "active operator command count drifted");

        let reserved = authority["reserved_namespaces"]
            .as_array()
            .ok_or_else(|| "reserved_namespaces must be an array".to_owned())?;
        assert_eq!(reserved.len(), 3, "Unit B reserved namespace count drifted");
        for entry in reserved {
            assert_eq!(entry["status"].as_str(), Some("RESERVED"));
            assert_eq!(entry["provider_mutation_authority"].as_bool(), Some(false));
            assert_eq!(entry["network_authority"].as_bool(), Some(false));
            assert_eq!(
                entry["production_authorization_authority"].as_bool(),
                Some(false)
            );
            let probes = entry["parser_probe_args"]
                .as_array()
                .ok_or_else(|| "reserved parser probes must be an array".to_owned())?;
            for probe in probes {
                let values = probe
                    .as_array()
                    .ok_or_else(|| "each reserved parser probe must be an array".to_owned())?;
                assert!(
                    parse_invocation(parser_args(values)?).is_err(),
                    "reserved parser probe unexpectedly became active: {values:?}"
                );
            }
        }

        for unknown in ["provision", "deploy", "mutate", "recovery", "readiness"] {
            assert!(
                parse_invocation(vec![OsString::from("opsctl"), OsString::from(unknown)]).is_err(),
                "unknown/reserved command unexpectedly parsed: {unknown}"
            );
        }
        Ok(())
    }
}
