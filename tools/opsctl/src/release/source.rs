use crate::release::digest::{canonical_json, sha256_hex};
use crate::release::model::{ReleaseModelError, ReleaseSetManifest};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const ACCEPTED_SOURCE_EVIDENCE_FILE: &str = "accepted-source-evidence.json";
const EVIDENCE_SCHEMA_VERSION: u64 = 1;
const EVIDENCE_KIND: &str = "AR11_ACCEPTED_SOURCE_EVIDENCE";
const PROTECTED_REF: &str = "refs/heads/main";
const COLLECTION_AUTHORITY: &str = "github-actions/github-api";
const PROOF_METHOD: &str = "GITHUB_COMPARE_API";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedSourceVerification {
    pub evidence_sha256: String,
    pub observed_protected_main_sha: String,
    pub lineage_status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineageStatus {
    Identical,
    Ahead,
}

impl LineageStatus {
    fn parse(value: &str) -> Result<Self, ReleaseModelError> {
        match value {
            "identical" => Ok(Self::Identical),
            "ahead" => Ok(Self::Ahead),
            other => Err(source_error(format!(
                "GitHub compare status {other} does not prove accepted protected-main ancestry"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::Ahead => "ahead",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedSourceEvidence {
    repository: String,
    release_set_id: String,
    source_commit_sha: String,
    observed_protected_main_sha: String,
    proof: LineageProof,
    evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LineageProof {
    base_sha: String,
    head_sha: String,
    merge_base_sha: String,
    status: LineageStatus,
    ahead_by: u64,
    behind_by: u64,
}

impl AcceptedSourceEvidence {
    pub fn load(path: &Path) -> Result<Self, ReleaseModelError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            source_error(format!(
                "accepted-source evidence unavailable {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(source_error(
                "accepted-source evidence must be a regular file",
            ));
        }
        let input = fs::read_to_string(path).map_err(|error| {
            source_error(format!(
                "cannot read accepted-source evidence {}: {error}",
                path.display()
            ))
        })?;
        let value: Value = serde_json::from_str(&input)
            .map_err(|error| source_error(format!("invalid accepted-source JSON: {error}")))?;
        Self::parse(value)
    }

    pub fn verify_for_release(
        &self,
        manifest: &ReleaseSetManifest,
    ) -> Result<AcceptedSourceVerification, ReleaseModelError> {
        self.verify_bindings(
            &manifest.release_set_id,
            &manifest.source.repository,
            &manifest.source.commit_sha,
        )
    }

    fn verify_bindings(
        &self,
        release_set_id: &str,
        repository: &str,
        source_commit_sha: &str,
    ) -> Result<AcceptedSourceVerification, ReleaseModelError> {
        if self.release_set_id != release_set_id {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_IDENTITY_MISMATCH: accepted-source evidence release_set_id={} manifest={release_set_id}",
                self.release_set_id
            )));
        }
        if self.repository != repository {
            return Err(source_error(format!(
                "repository mismatch evidence={} manifest={repository}",
                self.repository
            )));
        }
        if self.source_commit_sha != source_commit_sha {
            return Err(source_error(format!(
                "source SHA mismatch evidence={} manifest={source_commit_sha}",
                self.source_commit_sha
            )));
        }
        if self.proof.base_sha != self.source_commit_sha
            || self.proof.head_sha != self.observed_protected_main_sha
            || self.proof.merge_base_sha != self.source_commit_sha
            || self.proof.behind_by != 0
        {
            return Err(source_error(
                "GitHub compare evidence does not prove source as ancestor-or-self of protected main",
            ));
        }

        let lineage_valid = match self.proof.status {
            LineageStatus::Identical => {
                self.source_commit_sha == self.observed_protected_main_sha
                    && self.proof.ahead_by == 0
            }
            LineageStatus::Ahead => {
                self.source_commit_sha != self.observed_protected_main_sha
                    && self.proof.ahead_by > 0
            }
        };
        if !lineage_valid {
            return Err(source_error(format!(
                "GitHub compare status {} does not prove accepted protected-main ancestry",
                self.proof.status.as_str()
            )));
        }

        Ok(AcceptedSourceVerification {
            evidence_sha256: self.evidence_sha256.clone(),
            observed_protected_main_sha: self.observed_protected_main_sha.clone(),
            lineage_status: self.proof.status.as_str().to_owned(),
        })
    }

    fn parse(value: Value) -> Result<Self, ReleaseModelError> {
        let root = exact_object(
            &value,
            "accepted-source evidence",
            &[
                "schema_version",
                "kind",
                "repository",
                "release_set_id",
                "source_commit_sha",
                "protected_ref",
                "protected_ref_verified",
                "observed_protected_main_sha",
                "collection_authority",
                "proof",
                "evidence_sha256",
            ],
        )?;
        if required_u64(root, "schema_version")? != EVIDENCE_SCHEMA_VERSION {
            return Err(source_error(
                "unknown accepted-source evidence schema version",
            ));
        }
        if required_string(root, "kind")? != EVIDENCE_KIND {
            return Err(source_error("accepted-source evidence kind mismatch"));
        }
        if required_string(root, "protected_ref")? != PROTECTED_REF
            || !required_bool(root, "protected_ref_verified")?
        {
            return Err(source_error(
                "evidence does not prove the canonical protected refs/heads/main authority",
            ));
        }
        if required_string(root, "collection_authority")? != COLLECTION_AUTHORITY {
            return Err(source_error(
                "unsupported accepted-source collection authority",
            ));
        }

        let repository = required_string(root, "repository")?;
        let release_set_id = required_string(root, "release_set_id")?;
        let source_commit_sha = required_git_sha(root, "source_commit_sha")?;
        let observed_protected_main_sha = required_git_sha(root, "observed_protected_main_sha")?;
        let evidence_sha256 = required_sha256(root, "evidence_sha256")?;

        let proof_value = required(root, "proof")?;
        let proof_object = exact_object(
            proof_value,
            "accepted-source proof",
            &[
                "method",
                "base_sha",
                "head_sha",
                "merge_base_sha",
                "status",
                "ahead_by",
                "behind_by",
            ],
        )?;
        if required_string(proof_object, "method")? != PROOF_METHOD {
            return Err(source_error(
                "unsupported accepted-source lineage proof method",
            ));
        }
        let proof = LineageProof {
            base_sha: required_git_sha(proof_object, "base_sha")?,
            head_sha: required_git_sha(proof_object, "head_sha")?,
            merge_base_sha: required_git_sha(proof_object, "merge_base_sha")?,
            status: LineageStatus::parse(&required_string(proof_object, "status")?)?,
            ahead_by: required_u64(proof_object, "ahead_by")?,
            behind_by: required_u64(proof_object, "behind_by")?,
        };

        let mut digest_payload = value;
        digest_payload
            .as_object_mut()
            .ok_or_else(|| source_error("accepted-source evidence must be an object"))?
            .remove("evidence_sha256");
        let canonical = canonical_json(&digest_payload).map_err(source_error)?;
        let observed_digest = sha256_hex(canonical.as_bytes());
        if observed_digest != evidence_sha256 {
            return Err(source_error(format!(
                "evidence digest mismatch expected={evidence_sha256} observed={observed_digest}"
            )));
        }

        Ok(Self {
            repository,
            release_set_id,
            source_commit_sha,
            observed_protected_main_sha,
            proof,
            evidence_sha256,
        })
    }
}

pub fn evidence_path_for_release(release_set: &Path) -> Result<PathBuf, ReleaseModelError> {
    let parent = release_set.parent().ok_or_else(|| {
        source_error(format!(
            "Release Set path has no parent for accepted-source evidence: {}",
            release_set.display()
        ))
    })?;
    Ok(parent.join(ACCEPTED_SOURCE_EVIDENCE_FILE))
}

pub fn verify_release_source(
    release_set: &Path,
    manifest: &ReleaseSetManifest,
) -> Result<(PathBuf, AcceptedSourceVerification), ReleaseModelError> {
    let evidence_path = evidence_path_for_release(release_set)?;
    let evidence = AcceptedSourceEvidence::load(&evidence_path)?;
    let verification = evidence.verify_for_release(manifest)?;
    Ok((evidence_path, verification))
}

fn source_error(message: impl Into<String>) -> ReleaseModelError {
    ReleaseModelError::new(format!("SOURCE_NOT_ACCEPTED: {}", message.into()))
}

fn exact_object<'a>(
    value: &'a Value,
    label: &str,
    fields: &[&str],
) -> Result<&'a Map<String, Value>, ReleaseModelError> {
    let object = value
        .as_object()
        .ok_or_else(|| source_error(format!("{label} must be an object")))?;
    for key in object.keys() {
        if !fields.contains(&key.as_str()) {
            return Err(source_error(format!(
                "{label} contains unknown field: {key}"
            )));
        }
    }
    for field in fields {
        if !object.contains_key(*field) {
            return Err(source_error(format!(
                "{label} missing required field: {field}"
            )));
        }
    }
    Ok(object)
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReleaseModelError> {
    object
        .get(key)
        .ok_or_else(|| source_error(format!("missing accepted-source field: {key}")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    required(object, key)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| source_error(format!("accepted-source field {key} must be a string")))
}

fn required_bool(object: &Map<String, Value>, key: &str) -> Result<bool, ReleaseModelError> {
    required(object, key)?
        .as_bool()
        .ok_or_else(|| source_error(format!("accepted-source field {key} must be boolean")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ReleaseModelError> {
    required(object, key)?.as_u64().ok_or_else(|| {
        source_error(format!(
            "accepted-source field {key} must be unsigned integer"
        ))
    })
}

fn required_git_sha(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    let value = required_string(object, key)?;
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(source_error(format!(
            "accepted-source field {key} must be exact 40 lowercase hexadecimal"
        )));
    }
    Ok(value)
}

fn required_sha256(object: &Map<String, Value>, key: &str) -> Result<String, ReleaseModelError> {
    let value = required_string(object, key)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(source_error(format!(
            "accepted-source field {key} must be exact 64 lowercase hexadecimal"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::AcceptedSourceEvidence;
    use crate::release::digest::{canonical_json, sha256_hex};
    use serde_json::{Value, json};

    const RELEASE_ID: &str =
        "release-set-v1-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const SOURCE: &str = "1111111111111111111111111111111111111111";
    const NEWER_MAIN: &str = "2222222222222222222222222222222222222222";

    fn evidence(head: &str, status: &str, ahead_by: u64) -> Result<Value, String> {
        let mut value = json!({
            "schema_version": 1,
            "kind": "AR11_ACCEPTED_SOURCE_EVIDENCE",
            "repository": REPOSITORY,
            "release_set_id": RELEASE_ID,
            "source_commit_sha": SOURCE,
            "protected_ref": "refs/heads/main",
            "protected_ref_verified": true,
            "observed_protected_main_sha": head,
            "collection_authority": "github-actions/github-api",
            "proof": {
                "method": "GITHUB_COMPARE_API",
                "base_sha": SOURCE,
                "head_sha": head,
                "merge_base_sha": SOURCE,
                "status": status,
                "ahead_by": ahead_by,
                "behind_by": 0
            }
        });
        refresh_digest(&mut value)?;
        Ok(value)
    }

    fn refresh_digest(value: &mut Value) -> Result<(), String> {
        let mut payload = value.clone();
        payload
            .as_object_mut()
            .ok_or("fixture must be object")?
            .remove("evidence_sha256");
        value["evidence_sha256"] = Value::String(sha256_hex(canonical_json(&payload)?.as_bytes()));
        Ok(())
    }

    fn parse(value: Value) -> Result<AcceptedSourceEvidence, String> {
        AcceptedSourceEvidence::parse(value).map_err(|error| error.to_string())
    }

    fn verify(value: Value) -> Result<(), String> {
        parse(value)?
            .verify_bindings(RELEASE_ID, REPOSITORY, SOURCE)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn current_protected_main_source_is_accepted() -> Result<(), String> {
        verify(evidence(SOURCE, "identical", 0)?)
    }

    #[test]
    fn historical_ancestor_remains_accepted_after_main_advances() -> Result<(), String> {
        verify(evidence(NEWER_MAIN, "ahead", 3)?)
    }

    #[test]
    fn non_ancestor_merge_base_is_rejected() -> Result<(), String> {
        let mut value = evidence(NEWER_MAIN, "ahead", 2)?;
        value["proof"]["merge_base_sha"] =
            Value::String("3333333333333333333333333333333333333333".to_owned());
        refresh_digest(&mut value)?;
        let error = verify(value)
            .err()
            .ok_or("non-ancestor evidence unexpectedly verified")?;
        assert!(error.contains("SOURCE_NOT_ACCEPTED"));
        Ok(())
    }

    #[test]
    fn diverged_compare_status_is_rejected() -> Result<(), String> {
        let error = parse(evidence(NEWER_MAIN, "diverged", 2)?)
            .err()
            .ok_or("diverged evidence unexpectedly parsed")?;
        assert!(error.contains("SOURCE_NOT_ACCEPTED"));
        Ok(())
    }

    #[test]
    fn publication_repository_and_protection_bindings_fail_closed() -> Result<(), String> {
        let publication = parse(evidence(SOURCE, "identical", 0)?)?;
        let error = publication
            .verify_bindings(
                "release-set-v1-sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                REPOSITORY,
                SOURCE,
            )
            .err()
            .ok_or("wrong publication identity unexpectedly verified")?;
        assert!(error.to_string().contains("RELEASE_IDENTITY_MISMATCH"));

        let repository = parse(evidence(SOURCE, "identical", 0)?)?;
        let error = repository
            .verify_bindings(RELEASE_ID, "other/repository", SOURCE)
            .err()
            .ok_or("wrong repository unexpectedly verified")?;
        assert!(error.to_string().contains("SOURCE_NOT_ACCEPTED"));

        let mut protection = evidence(SOURCE, "identical", 0)?;
        protection["protected_ref_verified"] = Value::Bool(false);
        refresh_digest(&mut protection)?;
        let error = parse(protection)
            .err()
            .ok_or("unprotected branch evidence unexpectedly parsed")?;
        assert!(error.contains("SOURCE_NOT_ACCEPTED"));
        Ok(())
    }

    #[test]
    fn tampered_digest_is_rejected() -> Result<(), String> {
        let mut value = evidence(SOURCE, "identical", 0)?;
        value["evidence_sha256"] = Value::String(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned(),
        );
        let error = parse(value)
            .err()
            .ok_or("tampered evidence unexpectedly parsed")?;
        assert!(error.contains("SOURCE_NOT_ACCEPTED"));
        assert!(error.contains("digest mismatch"));
        Ok(())
    }

    #[test]
    fn unknown_field_and_schema_fail_closed() -> Result<(), String> {
        let mut unknown = evidence(SOURCE, "identical", 0)?;
        unknown["unexpected"] = Value::Bool(true);
        assert!(parse(unknown).is_err());

        let mut version = evidence(SOURCE, "identical", 0)?;
        version["schema_version"] = Value::from(2_u64);
        refresh_digest(&mut version)?;
        assert!(parse(version).is_err());
        Ok(())
    }
}
