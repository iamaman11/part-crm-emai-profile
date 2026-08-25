#![forbid(unsafe_code)]

mod architecture;
pub mod canonical;
mod cli;
pub mod credentials;
pub mod d1;
mod doctor;
mod error;
pub mod hosted_evidence;
pub mod promotion;
pub mod readiness;
pub mod recovery;
pub mod release;
mod repository;
mod status;

pub use cli::{CredentialsAction, HELP, Invocation, ReadCommand, parse_invocation};
pub use error::OpsctlError;
pub use hosted_evidence::HostedEvidenceAction;

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
            | Self::Status { .. }
            | Self::Credentials { .. }
            | Self::HostedEvidence { .. }
            | Self::ExternalReviewAttestations { .. }
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
            }
        }
        Invocation::Status {
            root,
            acceptance_evidence_json,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "status")?;
            status::run(&repo_root, &acceptance_evidence_json)
        }
        Invocation::Credentials { root, action } => {
            let repo_root = resolve_repo_root(root.as_deref(), "credentials")?;
            match action {
                CredentialsAction::Status => credentials::lifecycle(&repo_root),
                CredentialsAction::RotationPlan => credentials::rotation_plan(&repo_root),
            }
        }
        Invocation::HostedEvidence {
            root,
            action,
            input_json,
            evaluated_at_unix_seconds,
            expected_subject,
        } => {
            let _repo_root = resolve_repo_root(root.as_deref(), "hosted-evidence")?;
            let input = read_hosted_evidence_input(&input_json)?;
            match action {
                HostedEvidenceAction::SealOperationalCredential => {
                    hosted_evidence::seal_operational_credential_json(
                        &input,
                        evaluated_at_unix_seconds,
                        &expected_subject,
                    )
                }
                HostedEvidenceAction::VerifyOperationalCredential => {
                    hosted_evidence::verify_operational_credential_json(
                        &input,
                        evaluated_at_unix_seconds,
                        &expected_subject,
                    )
                }
            }
            .map_err(|error| OpsctlError::new("hosted-evidence", error.to_string()))
        }
        Invocation::ExternalReviewAttestations {
            root,
            observation_json,
        } => {
            let _repo_root = resolve_repo_root(root.as_deref(), "hosted-evidence")?;
            let input = read_hosted_evidence_input(&observation_json)?;
            hosted_evidence::verify_external_review_attestations_json(&input)
                .map_err(|error| OpsctlError::new("hosted-evidence", error.to_string()))
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

fn read_hosted_evidence_input(path: &std::path::Path) -> Result<String, OpsctlError> {
    std::fs::read_to_string(path).map_err(|error| {
        OpsctlError::new(
            "hosted-evidence",
            format!(
                "HOSTED_EVIDENCE_INPUT_UNAVAILABLE: {}: {error}",
                path.display()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{CredentialsAction, OperatorEffect, OpsctlError, execute, parse_invocation};
    use serde_json::json;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn assert_read_only_effect(effect: OperatorEffect) {
        assert_eq!(effect, OperatorEffect::ReadOnlyMetadata);
        assert!(!effect.has_side_effects());
        assert!(!effect.has_network_authority());
        assert!(!effect.has_provider_mutation_authority());
        assert!(!effect.has_secret_readback());
        assert!(!effect.has_production_mutation());
    }

    #[test]
    fn parsed_operator_command_has_typed_read_only_effect() -> Result<(), OpsctlError> {
        let invocation = parse_invocation([OsString::from("opsctl"), OsString::from("doctor")])?;
        assert_eq!(
            invocation.operator_effect(),
            Some(OperatorEffect::ReadOnlyMetadata)
        );
        assert_read_only_effect(OperatorEffect::ReadOnlyMetadata);
        Ok(())
    }

    #[test]
    fn hosted_evidence_surface_preserves_zero_effect_authority() -> Result<(), OpsctlError> {
        let invocation = parse_invocation([
            OsString::from("opsctl"),
            OsString::from("hosted-evidence"),
            OsString::from("operational-credential"),
            OsString::from("seal"),
            OsString::from("--observation-json"),
            OsString::from("observation.json"),
            OsString::from("--evaluated-at-unix-seconds"),
            OsString::from("1700000010"),
            OsString::from("--expected-subject"),
            OsString::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ])?;
        assert_eq!(
            invocation.operator_effect(),
            Some(OperatorEffect::ReadOnlyMetadata)
        );
        assert_read_only_effect(OperatorEffect::ReadOnlyMetadata);
        Ok(())
    }

    #[test]
    fn external_review_attestation_surface_preserves_zero_effect_authority()
    -> Result<(), OpsctlError> {
        let invocation = parse_invocation([
            OsString::from("opsctl"),
            OsString::from("hosted-evidence"),
            OsString::from("external-review-attestation"),
            OsString::from("verify"),
            OsString::from("--observation-json"),
            OsString::from("review-observation.json"),
        ])?;
        assert_eq!(
            invocation.operator_effect(),
            Some(OperatorEffect::ReadOnlyMetadata)
        );
        assert_read_only_effect(OperatorEffect::ReadOnlyMetadata);
        Ok(())
    }

    #[test]
    fn external_review_attestation_rejects_duplicate_evidence_ids() {
        let duplicate = json!({
            "schema_version": 1,
            "kind": "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION",
            "repository": "iamaman11/part-crm-emai-profile",
            "records": [
                {
                    "record": {"evidence_id": "ev-20260806-duplicate"},
                    "review_repository": null,
                    "review_reference": null,
                    "provider_object": null
                },
                {
                    "record": {"evidence_id": "ev-20260806-duplicate"},
                    "review_repository": null,
                    "review_reference": null,
                    "provider_object": null
                }
            ]
        });
        let result = super::hosted_evidence::verify_external_review_attestations_json(
            &duplicate.to_string(),
        );
        assert!(result.err().is_some_and(|error| {
            error
                .to_string()
                .contains("HOSTED_REVIEW_ATTESTATION_DUPLICATE_EVIDENCE_ID")
        }));
    }

    #[test]
    fn credentials_status_preserves_lifecycle_metadata_contract() -> Result<(), OpsctlError> {
        let output = execute(super::Invocation::Credentials {
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
        let output = execute(super::Invocation::Credentials {
            root: Some(repository_root()),
            action: CredentialsAction::RotationPlan,
        })?;
        assert!(output.contains("\"kind\": \"CREDENTIAL_LIFECYCLE_AUTHORITY\""));
        assert!(output.contains("\"production_mutation\": false"));
        assert!(!output.contains("OPERATOR_CONTRACT_AUTHORITY"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }
}
