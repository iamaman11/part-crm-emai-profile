use crate::canonical::{canonical_json, canonical_pretty_json, parse_strict_json, sha256_hex};
use opsctl_core::hosted_evidence::{
    EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyError,
    EvidencePolicyV1, EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
    HostedEvidenceEnvelopeV1, HostedEvidenceObservationV1, ReviewAttestationObservationV1,
    ReviewAttestationPolicyV1, ReviewAttestationStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};

const OBSERVATION_KIND: &str = "HOSTED_EVIDENCE_OBSERVATION";
const ARTIFACT_KIND: &str = "HOSTED_EVIDENCE_ARTIFACT";
const ENVELOPE_KIND: &str = "HOSTED_EVIDENCE_ENVELOPE";
const REVIEW_OBSERVATION_KIND: &str = "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION";
const REVIEW_RESULT_KIND: &str = "EXTERNAL_REVIEW_ATTESTATION_RESULT";
const REVIEW_CLAIM_DOMAIN: &str = "external-evidence-review-v1";
const SCHEMA_VERSION: u64 = 1;
const DIGEST_ALGORITHM: &str = "SHA-256";
const DIGEST_SCOPE: &str = "RFC8785_CANONICAL_ENVELOPE_BYTES";
const OPERATIONAL_CREDENTIAL_ISSUER: &str = "github-actions";
const OPERATIONAL_CREDENTIAL_SOURCE: &str = "github-governance-gate/operational-credential-state";
const OPERATIONAL_CREDENTIAL_TARGET: &str = "iamaman11/part-crm-emai-profile";
const OPERATIONAL_CREDENTIAL_ENVIRONMENT: &str = "staging";
const OPERATIONAL_CREDENTIAL_MAX_VALIDITY_SECONDS: u64 = 6 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedEvidenceAction {
    SealOperationalCredential,
    VerifyOperationalCredential,
}

impl HostedEvidenceAction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SealOperationalCredential => "seal",
            Self::VerifyOperationalCredential => "verify",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceAdapterError {
    message: String,
}

impl HostedEvidenceAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for HostedEvidenceAdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HostedEvidenceAdapterError {}

impl From<EvidencePolicyError> for HostedEvidenceAdapterError {
    fn from(error: EvidencePolicyError) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationDto {
    schema_version: u64,
    kind: String,
    issuer: String,
    source: String,
    target: String,
    environment: String,
    subject: String,
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    trust_state: TrustStateDto,
    outcome: OutcomeDto,
    production_mutation: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TrustStateDto {
    Trusted,
    Untrusted,
    Unknown,
}

impl From<TrustStateDto> for EvidenceTrustState {
    fn from(value: TrustStateDto) -> Self {
        match value {
            TrustStateDto::Trusted => Self::Trusted,
            TrustStateDto::Untrusted => Self::Untrusted,
            TrustStateDto::Unknown => Self::Unknown,
        }
    }
}

impl From<EvidenceTrustState> for TrustStateDto {
    fn from(value: EvidenceTrustState) -> Self {
        match value {
            EvidenceTrustState::Trusted => Self::Trusted,
            EvidenceTrustState::Untrusted => Self::Untrusted,
            EvidenceTrustState::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OutcomeDto {
    Pass,
    Fail,
}

impl From<OutcomeDto> for EvidenceOutcome {
    fn from(value: OutcomeDto) -> Self {
        match value {
            OutcomeDto::Pass => Self::Passed,
            OutcomeDto::Fail => Self::Failed,
        }
    }
}

impl From<EvidenceOutcome> for OutcomeDto {
    fn from(value: EvidenceOutcome) -> Self {
        match value {
            EvidenceOutcome::Passed => Self::Pass,
            EvidenceOutcome::Failed => Self::Fail,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeDto {
    schema_version: u64,
    kind: String,
    issuer: String,
    source: String,
    target: String,
    environment: String,
    subject: String,
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    trust_state: TrustStateDto,
    outcome: OutcomeDto,
    production_mutation: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DigestDto {
    algorithm: String,
    scope: String,
    value: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDto {
    schema_version: u64,
    kind: String,
    digest: DigestDto,
    envelope: EnvelopeDto,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewObservationBatchDto {
    schema_version: u64,
    kind: String,
    repository: String,
    records: Vec<ReviewObservedRecordDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewObservedRecordDto {
    record: Value,
    review_repository: Option<String>,
    review_reference: Option<String>,
    provider_object: Option<ProviderReviewObservationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderReviewObservationDto {
    available: bool,
    login: Option<String>,
    body: Option<String>,
    effective_timestamp: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewResultDto {
    schema_version: u64,
    kind: &'static str,
    verified_records: usize,
}

pub fn seal_operational_credential_json(
    observation_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, HostedEvidenceAdapterError> {
    validate_expected_subject(expected_subject)?;
    let value = parse_strict_json(observation_json).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_OBSERVATION_JSON: {error}"))
    })?;
    let dto: ObservationDto = serde_json::from_value(value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_OBSERVATION_SCHEMA: {error}"))
    })?;
    if dto.schema_version != SCHEMA_VERSION || dto.kind != OBSERVATION_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_OBSERVATION_CONTRACT: unsupported schema_version or kind",
        ));
    }
    let observation = observation_from_dto(dto)?;
    let policy = operational_credential_policy(expected_subject)?;
    let envelope = policy.evaluate(observation, evaluated_at_unix_seconds)?;
    let rendered = render_artifact(&envelope)?;
    let verified =
        verify_operational_credential_json(&rendered, evaluated_at_unix_seconds, expected_subject)?;
    if rendered != verified {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ROUNDTRIP_MISMATCH: seal/verify changed canonical evidence bytes",
        ));
    }
    Ok(rendered)
}

pub fn verify_operational_credential_json(
    artifact_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, HostedEvidenceAdapterError> {
    validate_expected_subject(expected_subject)?;
    let value = parse_strict_json(artifact_json).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_JSON: {error}"))
    })?;
    let dto: ArtifactDto = serde_json::from_value(value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    validate_artifact_contract(&dto)?;
    let supplied_digest = dto.digest.value.clone();
    let envelope = envelope_from_dto(dto.envelope)?;
    let envelope_value = serde_json::to_value(envelope_to_dto(&envelope)).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ENVELOPE_SCHEMA: {error}"))
    })?;
    let canonical_envelope = canonical_json(&envelope_value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ENVELOPE_CANONICAL: {error}"))
    })?;
    let observed_digest = sha256_hex(canonical_envelope.as_bytes());
    if supplied_digest != observed_digest {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_DIGEST_MISMATCH: canonical envelope digest does not match artifact",
        ));
    }

    let policy = operational_credential_policy(expected_subject)?;
    let reevaluated = policy.evaluate(envelope.as_observation(), evaluated_at_unix_seconds)?;
    if reevaluated != envelope {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_SEMANTIC_ROUNDTRIP_MISMATCH: parsed envelope changed after typed policy evaluation",
        ));
    }
    let rendered = render_artifact(&reevaluated)?;
    if artifact_json != rendered {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ARTIFACT_NOT_CANONICAL: durable artifact bytes must use the canonical pretty projection",
        ));
    }
    Ok(rendered)
}

pub fn verify_external_review_attestations_json(
    observation_json: &str,
) -> Result<String, HostedEvidenceAdapterError> {
    let value = parse_strict_json(observation_json).map_err(|error| {
        HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_OBSERVATION_JSON: {error}"
        ))
    })?;
    let dto: ReviewObservationBatchDto = serde_json::from_value(value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_OBSERVATION_SCHEMA: {error}"
        ))
    })?;
    if dto.schema_version != SCHEMA_VERSION || dto.kind != REVIEW_OBSERVATION_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_OBSERVATION_CONTRACT: unsupported schema_version or kind",
        ));
    }

    let expected_repository = EvidenceTarget::new(dto.repository)?;
    let mut seen_ids = HashSet::new();
    let mut superseded_ids = HashSet::new();
    for observed in &dto.records {
        let evidence_id = record_string(&observed.record, "evidence_id")?;
        if !seen_ids.insert(evidence_id.to_owned()) {
            return Err(HostedEvidenceAdapterError::new(format!(
                "HOSTED_REVIEW_ATTESTATION_DUPLICATE_EVIDENCE_ID: {evidence_id}"
            )));
        }
        if let Some(supersedes) = optional_record_string(&observed.record, "supersedes")? {
            superseded_ids.insert(supersedes.to_owned());
        }
    }

    let mut verified_records = 0usize;
    for observed in dto.records {
        let evidence_id = record_string(&observed.record, "evidence_id")?.to_owned();
        let status_text = record_string(&observed.record, "status")?;
        let status = match status_text {
            "passed" => Some(ReviewAttestationStatus::Passed),
            "failed" => Some(ReviewAttestationStatus::Failed),
            "pending" => None,
            other => {
                return Err(HostedEvidenceAdapterError::new(format!(
                    "HOSTED_REVIEW_ATTESTATION_STATUS_INVALID: unsupported evidence status {other}"
                )));
            }
        };
        let Some(status) = status else {
            continue;
        };
        if superseded_ids.contains(&evidence_id) {
            continue;
        }

        let gate = record_string(&observed.record, "gate")?.to_owned();
        let review = observed
            .record
            .get("review")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                HostedEvidenceAdapterError::new(format!(
                    "HOSTED_REVIEW_ATTESTATION_REVIEW_MISSING: active terminal record {evidence_id} has no review object"
                ))
            })?;
        let expected_reviewer = object_string(review, "github_login", "review")?.to_owned();
        let expected_reference = object_string(review, "review_reference", "review")?.to_owned();
        let expected_reviewed_at = object_string(review, "reviewed_at", "review")?.to_owned();
        let observed_repository = observed.review_repository.ok_or_else(|| {
            HostedEvidenceAdapterError::new(format!(
                "HOSTED_REVIEW_ATTESTATION_REPOSITORY_OBSERVATION_MISSING: {evidence_id}"
            ))
        })?;
        let observed_reference = observed.review_reference.ok_or_else(|| {
            HostedEvidenceAdapterError::new(format!(
                "HOSTED_REVIEW_ATTESTATION_REFERENCE_OBSERVATION_MISSING: {evidence_id}"
            ))
        })?;
        let provider_object = observed.provider_object.ok_or_else(|| {
            HostedEvidenceAdapterError::new(format!(
                "HOSTED_REVIEW_ATTESTATION_PROVIDER_OBSERVATION_MISSING: {evidence_id}"
            ))
        })?;
        let claim_sha256 = claim_sha256_for_record(&observed.record)?;

        ReviewAttestationPolicyV1.evaluate(&ReviewAttestationObservationV1 {
            expected_repository: expected_repository.clone(),
            observed_repository: EvidenceTarget::new(observed_repository)?,
            evidence_id: EvidenceSubject::new(evidence_id)?,
            gate: EvidenceSource::new(gate)?,
            status,
            expected_reference,
            observed_reference,
            expected_reviewer,
            observed_reviewer: provider_object.login,
            expected_reviewed_at,
            observed_reviewed_at: provider_object.effective_timestamp,
            claim_sha256,
            observed_body: provider_object.body,
            provider_object_available: provider_object.available,
        })?;
        verified_records += 1;
    }

    let result = ReviewResultDto {
        schema_version: SCHEMA_VERSION,
        kind: REVIEW_RESULT_KIND,
        verified_records,
    };
    let value = serde_json::to_value(result).map_err(|error| {
        HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_RESULT_SCHEMA: {error}"
        ))
    })?;
    canonical_pretty_json(&value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_RESULT_CANONICAL: {error}"
        ))
    })
}

fn claim_sha256_for_record(record: &Value) -> Result<String, HostedEvidenceAdapterError> {
    let mut bound_record = record.as_object().cloned().ok_or_else(|| {
        HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_RECORD_INVALID: record must be a JSON object",
        )
    })?;
    bound_record.remove("review");
    let payload = json!({
        "domain": REVIEW_CLAIM_DOMAIN,
        "record": Value::Object(bound_record),
    });
    let canonical = canonical_json(&payload).map_err(|error| {
        HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_CLAIM_CANONICAL: {error}"
        ))
    })?;
    Ok(sha256_hex(canonical.as_bytes()))
}

fn record_string<'a>(record: &'a Value, field: &str) -> Result<&'a str, HostedEvidenceAdapterError> {
    let object = record.as_object().ok_or_else(|| {
        HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_RECORD_INVALID: record must be a JSON object",
        )
    })?;
    object_string(object, field, "record")
}

fn optional_record_string<'a>(
    record: &'a Value,
    field: &str,
) -> Result<Option<&'a str>, HostedEvidenceAdapterError> {
    let object = record.as_object().ok_or_else(|| {
        HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_RECORD_INVALID: record must be a JSON object",
        )
    })?;
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => Err(HostedEvidenceAdapterError::new(format!(
            "HOSTED_REVIEW_ATTESTATION_RECORD_FIELD_INVALID: {field} must be a non-empty string"
        ))),
    }
}

fn object_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    owner: &str,
) -> Result<&'a str, HostedEvidenceAdapterError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HostedEvidenceAdapterError::new(format!(
                "HOSTED_REVIEW_ATTESTATION_FIELD_INVALID: {owner}.{field} must be a non-empty string"
            ))
        })
}

fn validate_artifact_contract(dto: &ArtifactDto) -> Result<(), HostedEvidenceAdapterError> {
    if dto.schema_version != SCHEMA_VERSION || dto.kind != ARTIFACT_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ARTIFACT_CONTRACT: unsupported schema_version or kind",
        ));
    }
    if dto.digest.algorithm != DIGEST_ALGORITHM || dto.digest.scope != DIGEST_SCOPE {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_DIGEST_CONTRACT: digest algorithm or scope drifted",
        ));
    }
    if !is_lower_hex(&dto.digest.value, 64) {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_DIGEST_INVALID: digest must be exactly 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn validate_expected_subject(subject: &str) -> Result<(), HostedEvidenceAdapterError> {
    if !is_lower_hex(subject, 40) {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_EXPECTED_SUBJECT_INVALID: expected accepted-source commit must be exactly 40 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn operational_credential_policy(
    expected_subject: &str,
) -> Result<EvidencePolicyV1, HostedEvidenceAdapterError> {
    let binding = EvidenceBindingV1 {
        issuer: EvidenceIssuer::new(OPERATIONAL_CREDENTIAL_ISSUER)?,
        source: EvidenceSource::new(OPERATIONAL_CREDENTIAL_SOURCE)?,
        target: EvidenceTarget::new(OPERATIONAL_CREDENTIAL_TARGET)?,
        environment: EvidenceEnvironment::new(OPERATIONAL_CREDENTIAL_ENVIRONMENT)?,
        subject: EvidenceSubject::new(expected_subject)?,
    };
    EvidencePolicyV1::new(binding, OPERATIONAL_CREDENTIAL_MAX_VALIDITY_SECONDS).map_err(Into::into)
}

fn observation_from_dto(
    dto: ObservationDto,
) -> Result<HostedEvidenceObservationV1, HostedEvidenceAdapterError> {
    Ok(HostedEvidenceObservationV1 {
        binding: EvidenceBindingV1 {
            issuer: EvidenceIssuer::new(dto.issuer)?,
            source: EvidenceSource::new(dto.source)?,
            target: EvidenceTarget::new(dto.target)?,
            environment: EvidenceEnvironment::new(dto.environment)?,
            subject: EvidenceSubject::new(dto.subject)?,
        },
        source_run_id: dto.source_run_id,
        source_run_attempt: dto.source_run_attempt,
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        valid_until_unix_seconds: dto.valid_until_unix_seconds,
        trust_state: dto.trust_state.into(),
        outcome: dto.outcome.into(),
        production_mutation: dto.production_mutation,
    })
}

fn envelope_from_dto(
    dto: EnvelopeDto,
) -> Result<HostedEvidenceEnvelopeV1, HostedEvidenceAdapterError> {
    if dto.schema_version != SCHEMA_VERSION || dto.kind != ENVELOPE_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ENVELOPE_CONTRACT: unsupported schema_version or kind",
        ));
    }
    Ok(HostedEvidenceEnvelopeV1 {
        binding: EvidenceBindingV1 {
            issuer: EvidenceIssuer::new(dto.issuer)?,
            source: EvidenceSource::new(dto.source)?,
            target: EvidenceTarget::new(dto.target)?,
            environment: EvidenceEnvironment::new(dto.environment)?,
            subject: EvidenceSubject::new(dto.subject)?,
        },
        source_run_id: dto.source_run_id,
        source_run_attempt: dto.source_run_attempt,
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        valid_until_unix_seconds: dto.valid_until_unix_seconds,
        trust_state: dto.trust_state.into(),
        outcome: dto.outcome.into(),
        production_mutation: dto.production_mutation,
    })
}

fn envelope_to_dto(envelope: &HostedEvidenceEnvelopeV1) -> EnvelopeDto {
    EnvelopeDto {
        schema_version: SCHEMA_VERSION,
        kind: ENVELOPE_KIND.to_owned(),
        issuer: envelope.binding.issuer.as_str().to_owned(),
        source: envelope.binding.source.as_str().to_owned(),
        target: envelope.binding.target.as_str().to_owned(),
        environment: envelope.binding.environment.as_str().to_owned(),
        subject: envelope.binding.subject.as_str().to_owned(),
        source_run_id: envelope.source_run_id,
        source_run_attempt: envelope.source_run_attempt,
        observed_at_unix_seconds: envelope.observed_at_unix_seconds,
        valid_until_unix_seconds: envelope.valid_until_unix_seconds,
        trust_state: envelope.trust_state.into(),
        outcome: envelope.outcome.into(),
        production_mutation: envelope.production_mutation,
    }
}

fn render_artifact(
    envelope: &HostedEvidenceEnvelopeV1,
) -> Result<String, HostedEvidenceAdapterError> {
    let envelope_dto = envelope_to_dto(envelope);
    let envelope_value = serde_json::to_value(&envelope_dto).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ENVELOPE_SCHEMA: {error}"))
    })?;
    let canonical_envelope = canonical_json(&envelope_value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ENVELOPE_CANONICAL: {error}"))
    })?;
    let artifact = ArtifactDto {
        schema_version: SCHEMA_VERSION,
        kind: ARTIFACT_KIND.to_owned(),
        digest: DigestDto {
            algorithm: DIGEST_ALGORITHM.to_owned(),
            scope: DIGEST_SCOPE.to_owned(),
            value: sha256_hex(canonical_envelope.as_bytes()),
        },
        envelope: envelope_dto,
    };
    let value = serde_json::to_value(artifact).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    canonical_pretty_json(&value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_CANONICAL: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        claim_sha256_for_record, seal_operational_credential_json,
        verify_external_review_attestations_json, verify_operational_credential_json,
    };
    use serde_json::{Value, json};

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OBSERVED_AT: i64 = 1_700_000_000;
    const EVALUATED_AT: i64 = 1_700_000_010;

    fn observation() -> Value {
        json!({
            "schema_version": 1,
            "kind": "HOSTED_EVIDENCE_OBSERVATION",
            "issuer": "github-actions",
            "source": "github-governance-gate/operational-credential-state",
            "target": "iamaman11/part-crm-emai-profile",
            "environment": "staging",
            "subject": SUBJECT,
            "source_run_id": 123456,
            "source_run_attempt": 1,
            "observed_at_unix_seconds": OBSERVED_AT,
            "valid_until_unix_seconds": OBSERVED_AT + 3600,
            "trust_state": "TRUSTED",
            "outcome": "PASS",
            "production_mutation": false
        })
    }

    fn seal(value: &Value) -> Result<String, super::HostedEvidenceAdapterError> {
        seal_operational_credential_json(&value.to_string(), EVALUATED_AT, SUBJECT)
    }

    fn terminal_record(evidence_id: &str, reference: &str, supersedes: Option<&str>) -> Value {
        let mut record = json!({
            "artifact_digests_sha256": ["11".repeat(32)],
            "checks": [{"code": "synthetic_check", "outcome": "pass"}],
            "evidence_id": evidence_id,
            "gate": "product_license",
            "limitations": ["synthetic_fixture_only"],
            "observed_at": "2026-08-06T14:39:00Z",
            "references": ["review-report:sha256:".to_owned() + &"22".repeat(32)],
            "review": {
                "github_login": "reviewer-one",
                "review_reference": reference,
                "reviewed_at": "2026-08-06T14:40:00Z"
            },
            "schema_version": 1,
            "scope": {"environment": "none", "subject_id": "synthetic-attestation-fixture"},
            "status": "passed"
        });
        if let Some(previous) = supersedes {
            record["supersedes"] = Value::String(previous.to_owned());
        }
        record
    }

    fn observed_record(record: Value) -> Value {
        let reference = record["review"]["review_reference"]
            .as_str()
            .expect("fixture review reference")
            .to_owned();
        let digest = claim_sha256_for_record(&record).expect("fixture claim digest");
        let evidence_id = record["evidence_id"].as_str().expect("fixture evidence id");
        let gate = record["gate"].as_str().expect("fixture gate");
        let status = record["status"].as_str().expect("fixture status");
        json!({
            "record": record,
            "review_repository": "acme/profile-platform",
            "review_reference": reference,
            "provider_object": {
                "available": true,
                "login": "reviewer-one",
                "body": format!(
                    "external-evidence-review-v1\nevidence_id={evidence_id}\ngate={gate}\nstatus={status}\nclaim_sha256={digest}"
                ),
                "effective_timestamp": "2026-08-06T14:40:00Z"
            }
        })
    }

    fn review_batch(records: Vec<Value>) -> Value {
        json!({
            "schema_version": 1,
            "kind": "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION",
            "repository": "acme/profile-platform",
            "records": records
        })
    }

    #[test]
    fn strict_adapter_roundtrips_semantically_and_byte_stably()
    -> Result<(), Box<dyn std::error::Error>> {
        let artifact = seal(&observation())?;
        let verified = verify_operational_credential_json(&artifact, EVALUATED_AT, SUBJECT)?;
        assert_eq!(artifact, verified);
        assert!(artifact.contains("\"algorithm\": \"SHA-256\""));
        assert!(artifact.contains("\"scope\": \"RFC8785_CANONICAL_ENVELOPE_BYTES\""));
        assert!(artifact.contains("\"production_mutation\": false"));
        Ok(())
    }

    #[test]
    fn rejects_unknown_fields_and_secret_shaped_legacy_payloads() {
        let mut unknown = observation();
        unknown["token"] = Value::String("must-not-cross-boundary".to_owned());
        assert!(seal(&unknown).is_err());

        let legacy = json!({
            "schema_version": 0,
            "kind": "HOSTED_EVIDENCE",
            "provider": "github",
            "status": "ok"
        });
        assert!(seal(&legacy).is_err());
    }

    #[test]
    fn rejects_malformed_timestamp_and_bad_binding() {
        let mut malformed = observation();
        malformed["observed_at_unix_seconds"] = Value::String("yesterday".to_owned());
        assert!(seal(&malformed).is_err());

        let mut wrong_target = observation();
        wrong_target["target"] = Value::String("other/repository".to_owned());
        assert!(seal(&wrong_target).is_err());

        let mut wrong_source = observation();
        wrong_source["source"] = Value::String("untrusted-shell".to_owned());
        assert!(seal(&wrong_source).is_err());
    }

    #[test]
    fn rejects_untrusted_mutating_and_impossible_freshness() {
        let mut untrusted = observation();
        untrusted["trust_state"] = Value::String("UNTRUSTED".to_owned());
        assert!(seal(&untrusted).is_err());

        let mut mutating = observation();
        mutating["production_mutation"] = Value::Bool(true);
        assert!(seal(&mutating).is_err());

        let mut impossible = observation();
        impossible["valid_until_unix_seconds"] = Value::from(OBSERVED_AT);
        assert!(seal(&impossible).is_err());

        let mut replayed = observation();
        replayed["valid_until_unix_seconds"] = Value::from(EVALUATED_AT);
        assert!(seal(&replayed).is_err());
    }

    #[test]
    fn rejects_bad_digest_and_noncanonical_artifact_bytes() -> Result<(), Box<dyn std::error::Error>>
    {
        let artifact = seal(&observation())?;
        let mut value: Value = serde_json::from_str(&artifact)?;
        value["digest"]["value"] = Value::String("0".repeat(64));
        let tampered = serde_json::to_string_pretty(&value)? + "\n";
        assert!(verify_operational_credential_json(&tampered, EVALUATED_AT, SUBJECT).is_err());

        let compact = serde_json::to_string(&serde_json::from_str::<Value>(&artifact)?)?;
        assert!(verify_operational_credential_json(&compact, EVALUATED_AT, SUBJECT).is_err());
        Ok(())
    }

    #[test]
    fn external_review_adapter_accepts_exact_active_terminal_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = terminal_record(
            "ev-20260806-terminal",
            "https://github.com/acme/profile-platform/issues/9#issuecomment-101",
            None,
        );
        let result = verify_external_review_attestations_json(
            &review_batch(vec![observed_record(record)]).to_string(),
        )?;
        assert!(result.contains("\"verified_records\": 1"));
        Ok(())
    }

    #[test]
    fn external_review_adapter_rejects_unknown_or_legacy_observation_fields() {
        let record = terminal_record(
            "ev-20260806-terminal",
            "https://github.com/acme/profile-platform/issues/9#issuecomment-101",
            None,
        );
        let mut unknown = review_batch(vec![observed_record(record)]);
        unknown["token"] = Value::String("must-not-cross-boundary".to_owned());
        assert!(verify_external_review_attestations_json(&unknown.to_string()).is_err());

        let legacy = json!({
            "schema_version": 0,
            "kind": "EXTERNAL_REVIEW_OBSERVATION",
            "repository": "acme/profile-platform",
            "records": []
        });
        assert!(verify_external_review_attestations_json(&legacy.to_string()).is_err());
    }

    #[test]
    fn external_review_adapter_rejects_provider_mutation_and_foreign_binding() {
        let record = terminal_record(
            "ev-20260806-terminal",
            "https://github.com/acme/profile-platform/issues/9#issuecomment-101",
            None,
        );
        let observed = observed_record(record);

        let mut wrong_body = review_batch(vec![observed.clone()]);
        wrong_body["records"][0]["provider_object"]["body"] =
            Value::String("external-evidence-review-v1\nwrong=true".to_owned());
        assert!(verify_external_review_attestations_json(&wrong_body.to_string()).is_err());

        let mut wrong_reviewer = review_batch(vec![observed.clone()]);
        wrong_reviewer["records"][0]["provider_object"]["login"] =
            Value::String("another-reviewer".to_owned());
        assert!(verify_external_review_attestations_json(&wrong_reviewer.to_string()).is_err());

        let mut edited = review_batch(vec![observed.clone()]);
        edited["records"][0]["provider_object"]["effective_timestamp"] =
            Value::String("2026-08-06T14:41:00Z".to_owned());
        assert!(verify_external_review_attestations_json(&edited.to_string()).is_err());

        let mut deleted = review_batch(vec![observed.clone()]);
        deleted["records"][0]["provider_object"]["available"] = Value::Bool(false);
        assert!(verify_external_review_attestations_json(&deleted.to_string()).is_err());

        let mut foreign = review_batch(vec![observed]);
        foreign["records"][0]["review_repository"] =
            Value::String("other/repository".to_owned());
        assert!(verify_external_review_attestations_json(&foreign.to_string()).is_err());
    }

    #[test]
    fn external_review_adapter_owns_active_terminal_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let old_id = "ev-20260806-old-terminal";
        let old = terminal_record(
            old_id,
            "https://github.com/acme/profile-platform/issues/9#issuecomment-404",
            None,
        );
        let replacement = terminal_record(
            "ev-20260806-active-replacement",
            "https://github.com/acme/profile-platform/issues/9#issuecomment-505",
            Some(old_id),
        );
        let mut old_observed = observed_record(old);
        old_observed["provider_object"]["available"] = Value::Bool(false);
        let result = verify_external_review_attestations_json(
            &review_batch(vec![old_observed, observed_record(replacement)]).to_string(),
        )?;
        assert!(result.contains("\"verified_records\": 1"));
        Ok(())
    }

    #[test]
    fn external_review_adapter_allows_pending_records_without_provider_review()
    -> Result<(), Box<dyn std::error::Error>> {
        let pending = json!({
            "record": {
                "artifact_digests_sha256": [],
                "checks": [],
                "evidence_id": "ev-20260806-pending",
                "gate": "product_license",
                "limitations": [],
                "observed_at": "2026-08-06T14:39:00Z",
                "references": [],
                "schema_version": 1,
                "scope": {"environment": "none", "subject_id": "pending"},
                "status": "pending"
            },
            "review_repository": null,
            "review_reference": null,
            "provider_object": null
        });
        let result = verify_external_review_attestations_json(
            &review_batch(vec![pending]).to_string(),
        )?;
        assert!(result.contains("\"verified_records\": 0"));
        Ok(())
    }
}
