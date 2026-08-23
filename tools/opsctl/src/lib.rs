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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorEffect {
    ReadOnlyMetadata,
}

impl OperatorEffect {
    #[must_use]
    pub const fn has_side_effects(self) -> bool {
        false
    }

    #[must_use]
    pub const fn has_network_authority(self) -> bool {
        false
    }

    #[must_use]
    pub const fn has_provider_mutation_authority(self) -> bool {
        false
    }

    #[must_use]
    pub const fn has_secret_readback(self) -> bool {
        false
    }

    #[must_use]
    pub const fn has_production_mutation(self) -> bool {
        false
    }
}

impl Invocation {
    /// Semantic effect metadata is derived from the real typed invocation, never from
    /// a serialized operator registry. Help/version are process-local presentation;
    /// every accepted operator command is metadata-only and read-only.
    #[must_use]
    pub const fn operator_effect(&self) -> Option<OperatorEffect> {
        match self {
            Self::Help | Self::Version => None,
            Self::Run { .. }
            | Self::Credentials { .. }
            | Self::D1 { .. }
            | Self::D1Repository { .. }
            | Self::ReleaseFinalize { .. }
            | Self::Release { .. }
            | Self::Promotion { .. } => Some(OperatorEffect::ReadOnlyMetadata),
        }
    }
}

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
    use super::{CredentialsAction, Invocation, OperatorEffect, OpsctlError, execute, parse_invocation};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn parser_args(values: &[&str]) -> Vec<OsString> {
        values.iter().copied().map(OsString::from).collect()
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

    fn assert_read_only_effect(invocation: &Invocation) {
        let effect = invocation.operator_effect();
        assert_eq!(effect, Some(OperatorEffect::ReadOnlyMetadata));
        let effect = effect.expect("operator command must have typed effect metadata");
        assert!(!effect.has_side_effects());
        assert!(!effect.has_network_authority());
        assert!(!effect.has_provider_mutation_authority());
        assert!(!effect.has_secret_readback());
        assert!(!effect.has_production_mutation());
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
    fn credentials_rotation_plan_uses_bounded_lifecycle_owner() -> Result<(), OpsctlError> {
        let output = execute(Invocation::Credentials {
            root: Some(repository_root()),
            action: CredentialsAction::RotationPlan,
        })?;
        assert!(output.contains("\"kind\": \"CREDENTIAL_LIFECYCLE_AUTHORITY\""));
        assert!(output.contains("\"production_mutation\": false"));
        assert!(!output.contains("OPERATOR_CONTRACT_AUTHORITY"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }

    #[test]
    fn active_parser_surfaces_derive_effects_from_typed_invocations() -> Result<(), String> {
        let cases: &[(&str, &[&str])] = &[
            ("opsctl doctor", &["opsctl", "doctor"]),
            ("opsctl status", &["opsctl", "status"]),
            ("opsctl inventory", &["opsctl", "inventory"]),
            ("opsctl credentials status", &["opsctl", "credentials", "status"]),
            (
                "opsctl credentials rotation-plan",
                &["opsctl", "credentials", "rotation-plan"],
            ),
            ("opsctl d1 repository", &["opsctl", "d1", "repository"]),
            (
                "opsctl d1 status",
                &[
                    "opsctl",
                    "d1",
                    "status",
                    "--component",
                    "catalog",
                    "--ledger-json",
                    "ledger.json",
                ],
            ),
            (
                "opsctl d1 plan",
                &[
                    "opsctl",
                    "d1",
                    "plan",
                    "--component",
                    "catalog",
                    "--ledger-json",
                    "ledger.json",
                    "--release-manifest",
                    "release.json",
                ],
            ),
            (
                "opsctl d1 compatibility",
                &[
                    "opsctl",
                    "d1",
                    "compatibility",
                    "--component",
                    "catalog",
                    "--ledger-json",
                    "ledger.json",
                    "--release-manifest",
                    "release.json",
                ],
            ),
            (
                "opsctl d1 verify",
                &[
                    "opsctl",
                    "d1",
                    "verify",
                    "--component",
                    "catalog",
                    "--ledger-json",
                    "ledger.json",
                    "--release-manifest",
                    "release.json",
                ],
            ),
            (
                "opsctl release finalize",
                &[
                    "opsctl",
                    "release",
                    "finalize",
                    "--request-json",
                    "release-finalize-request.json",
                ],
            ),
            (
                "opsctl release inspect",
                &[
                    "opsctl",
                    "release",
                    "inspect",
                    "--release-set",
                    "release-set.json",
                ],
            ),
            (
                "opsctl release verify",
                &[
                    "opsctl",
                    "release",
                    "verify",
                    "--release-set",
                    "release-set.json",
                    "--artifact-root",
                    "artifacts",
                ],
            ),
            (
                "opsctl release compatibility",
                &[
                    "opsctl",
                    "release",
                    "compatibility",
                    "--release-set",
                    "release-set.json",
                    "--profile",
                    "rehearsal-core-v1",
                    "--environment",
                    "rehearsal",
                    "--evidence-json",
                    "evidence.json",
                ],
            ),
            (
                "opsctl promotion plan",
                &[
                    "opsctl",
                    "promotion",
                    "plan",
                    "--release-set",
                    "release-set.json",
                    "--profile",
                    "rehearsal-core-v1",
                    "--environment",
                    "rehearsal",
                    "--snapshot",
                    "snapshot.json",
                    "--evidence-json",
                    "evidence.json",
                ],
            ),
            (
                "opsctl promotion preflight",
                &[
                    "opsctl",
                    "promotion",
                    "preflight",
                    "--release-set",
                    "release-set.json",
                    "--profile",
                    "rehearsal-core-v1",
                    "--environment",
                    "rehearsal",
                    "--snapshot",
                    "snapshot.json",
                    "--evidence-json",
                    "evidence.json",
                ],
            ),
            (
                "opsctl promotion verify",
                &[
                    "opsctl",
                    "promotion",
                    "verify",
                    "--release-set",
                    "release-set.json",
                    "--profile",
                    "rehearsal-core-v1",
                    "--environment",
                    "rehearsal",
                    "--snapshot",
                    "snapshot.json",
                    "--evidence-json",
                    "evidence.json",
                ],
            ),
        ];

        for (expected_command, args) in cases {
            let invocation = parse_invocation(parser_args(args))
                .map_err(|error| format!("{expected_command} did not parse: {error}"))?;
            assert_eq!(
                invocation_command(&invocation).as_deref(),
                Some(*expected_command),
                "{expected_command}"
            );
            assert_read_only_effect(&invocation);
        }
        Ok(())
    }

    #[test]
    fn reserved_and_mutating_surfaces_remain_fail_closed() {
        for args in [
            &["opsctl", "credentials", "readiness"][..],
            &["opsctl", "recovery", "inspect"][..],
            &["opsctl", "recovery", "plan"][..],
            &["opsctl", "recovery", "verify"][..],
            &["opsctl", "readiness"][..],
            &["opsctl", "provision"][..],
            &["opsctl", "deploy"][..],
            &["opsctl", "mutate"][..],
        ] {
            assert!(
                parse_invocation(parser_args(args)).is_err(),
                "reserved/mutating command unexpectedly parsed: {args:?}"
            );
        }
    }
}
