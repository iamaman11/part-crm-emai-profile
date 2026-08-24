use crate::canonical::{canonical_json, canonical_pretty_json, parse_strict_json, sha256_hex};
use opsctl_core::hosted_evidence::{
    EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyError,
    EvidencePolicyV1, EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
    HostedEvidenceEnvelopeV1, HostedEvidenceObservationV1,
};
use opsctl_core::review_evidence::{
    ProviderReviewFactV1, PullRequestReviewState, RequiredReviewClaimV1,
    ReviewAttestationPolicyV1, ReviewKind, ReviewObservationV1, ReviewPolicyError, ReviewStatus,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

const OBSERVATION_KIND: &str = "REVIEW_EVIDENCE_OBSERVATION";
const ARTIFACT_KIND: &str = "REVIEW_EVIDENCE_ARTIFACT";
const PAYLOAD_KIND: &str = "REVIEW_EVIDENCE_PAYLOAD";
const ENVELOPE_KIND: &str = "HOSTED_EVIDENCE_ENVELOPE";
const SCHEMA_VERSION: u64 = 1;
const DIGEST_ALGORITHM: &str = "SHA-256";
const DIGEST_SCOPE: &str = "RFC8785_CANONICAL_REVIEW_PAYLOAD_BYTES";
const ISSUER: &str = "github-actions";
const SOURCE: &str = "external-review-attestation-gate/github-review-attestations";
const TARGET: &str = "iamaman11/part-crm-emai-profile";
const ENVIRONMENT: &str = "governance";
const MAX_VALIDITY_SECONDS: u64 = 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewEvidenceAdapterError {
    message: String,
}

impl ReviewEvidenceAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReviewEvidenceAdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReviewEvidenceAdapterError {}

impl From<EvidencePolicyError> for ReviewEvidenceAdapterError {
    fn from(error: EvidencePolicyError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<ReviewPolicyError> for ReviewEvidenceAdapterError {
    fn from(error: ReviewPolicyError) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewObservationDto {
    schema_version: u64,
    kind: String,
    repository: String,
    subject: String,
    provider_repository: String,
    provider_subject: String,
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    production_mutation: bool,
    required_claims: Vec<RequiredReviewClaimDto>,
    provider_reviews: Vec<ProviderReviewDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequiredReviewClaimDto {
    evidence_id: String,
    gate: String,
    status: String,
    subject: String,
    claim_sha256: String,
    reviewer: String,
    reviewed_at_unix_seconds: i64,
    execution_window_start_unix_seconds: i64,
    review_kind: String,
    review_reference: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderReviewDto {
    review_kind: String,
    review_reference: String,
    author: String,
    body: String,
    observed_at_unix_seconds: i64,
    pull_request_state: Option<String>,
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
    trust_state: String,
    outcome: String,
    production_mutation: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PayloadDto {
    schema_version: u64,
    kind: String,
    observation: ReviewObservationDto,
    envelope: EnvelopeDto,
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
    payload: PayloadDto,
}

#[must_use]
pub fn is_review_observation_json(input: &str) -> bool {
    contract_kind(input).as_deref() == Some(OBSERVATION_KIND)
}

#[must_use]
pub fn is_review_artifact_json(input: &str) -> bool {
    contract_kind(input).as_deref() == Some(ARTIFACT_KIND)
}

pub fn seal_review_json(
    observation_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, ReviewEvidenceAdapterError> {
    validate_subject(expected_subject)?;
    let observation = parse_observation(observation_json, expected_subject)?;
    review_policy(expected_subject)?.evaluate(&domain_observation(&observation)?)?;
    let envelope = hosted_policy(expected_subject)?.evaluate(
        hosted_observation(&observation)?,
        evaluated_at_unix_seconds,
    )?;
    let rendered = render_artifact(observation, &envelope)?;
    let replay = verify_review_json(&rendered, evaluated_at_unix_seconds, expected_subject)?;
    if rendered != replay {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ROUNDTRIP_MISMATCH: seal/verify changed durable bytes",
        ));
    }
    Ok(rendered)
}

pub fn verify_review_json(
    artifact_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, ReviewEvidenceAdapterError> {
    validate_subject(expected_subject)?;
    let value = parse_strict_json(artifact_json).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_JSON: {error}"))
    })?;
    let artifact: ArtifactDto = serde_json::from_value(value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    validate_artifact(&artifact)?;

    let payload_value = serde_json::to_value(&artifact.payload).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_SCHEMA: {error}"))
    })?;
    let canonical_payload = canonical_json(&payload_value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_CANONICAL: {error}"))
    })?;
    if sha256_hex(canonical_payload.as_bytes()) != artifact.digest.value {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_MISMATCH: canonical payload digest differs",
        ));
    }

    validate_observation(&artifact.payload.observation, expected_subject)?;
    review_policy(expected_subject)?
        .evaluate(&domain_observation(&artifact.payload.observation)?)?;
    let durable_envelope = envelope_from_dto(&artifact.payload.envelope)?;
    let evaluated_envelope = hosted_policy(expected_subject)?
        .evaluate(durable_envelope.as_observation(), evaluated_at_unix_seconds)?;
    if durable_envelope != evaluated_envelope {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ENVELOPE_MISMATCH: durable envelope differs from EvidencePolicyV1 output",
        ));
    }

    let rendered = render_artifact(artifact.payload.observation, &evaluated_envelope)?;
    if artifact_json != rendered {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_NOT_CANONICAL: durable artifact bytes must be canonical pretty JSON",
        ));
    }
    Ok(rendered)
}

fn contract_kind(input: &str) -> Option<String> {
    let value = parse_strict_json(input).ok()?;
    value.get("kind")?.as_str().map(str::to_owned)
}

fn parse_observation(
    input: &str,
    expected_subject: &str,
) -> Result<ReviewObservationDto, ReviewEvidenceAdapterError> {
    let value = parse_strict_json(input).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_OBSERVATION_JSON: {error}"))
    })?;
    let observation = serde_json::from_value(value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_OBSERVATION_SCHEMA: {error}"))
    })?;
    validate_observation(&observation, expected_subject)?;
    Ok(observation)
}

fn validate_observation(
    observation: &ReviewObservationDto,
    expected_subject: &str,
) -> Result<(), ReviewEvidenceAdapterError> {
    if observation.schema_version != SCHEMA_VERSION || observation.kind != OBSERVATION_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_OBSERVATION_CONTRACT: unsupported schema version or kind",
        ));
    }
    if observation.repository != TARGET || observation.subject != expected_subject {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_BINDING_MISMATCH: declared repository or subject differs from consumer binding",
        ));
    }
    Ok(())
}

fn validate_artifact(artifact: &ArtifactDto) -> Result<(), ReviewEvidenceAdapterError> {
    if artifact.schema_version != SCHEMA_VERSION || artifact.kind != ARTIFACT_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ARTIFACT_CONTRACT: unsupported schema version or kind",
        ));
    }
    if artifact.payload.schema_version != SCHEMA_VERSION || artifact.payload.kind != PAYLOAD_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_PAYLOAD_CONTRACT: unsupported schema version or kind",
        ));
    }
    if artifact.digest.algorithm != DIGEST_ALGORITHM || artifact.digest.scope != DIGEST_SCOPE {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_CONTRACT: digest algorithm or scope drifted",
        ));
    }
    if !is_lower_hex(&artifact.digest.value, 64) {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_INVALID: digest must be lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn domain_observation(
    observation: &ReviewObservationDto,
) -> Result<ReviewObservationV1, ReviewEvidenceAdapterError> {
    let required_claims = observation
        .required_claims
        .iter()
        .map(|claim| {
            Ok(RequiredReviewClaimV1 {
                evidence_id: claim.evidence_id.clone(),
                gate: claim.gate.clone(),
                status: parse_status(&claim.status)?,
                subject: claim.subject.clone(),
                claim_sha256: claim.claim_sha256.clone(),
                reviewer: claim.reviewer.clone(),
                reviewed_at_unix_seconds: claim.reviewed_at_unix_seconds,
                execution_window_start_unix_seconds: claim.execution_window_start_unix_seconds,
                review_kind: parse_kind(&claim.review_kind)?,
                review_reference: claim.review_reference.clone(),
            })
        })
        .collect::<Result<Vec<_>, ReviewEvidenceAdapterError>>()?;
    let provider_reviews = observation
        .provider_reviews
        .iter()
        .map(|review| {
            Ok(ProviderReviewFactV1 {
                review_kind: parse_kind(&review.review_kind)?,
                review_reference: review.review_reference.clone(),
                author: review.author.clone(),
                body: review.body.clone(),
                observed_at_unix_seconds: review.observed_at_unix_seconds,
                pull_request_state: review
                    .pull_request_state
                    .as_deref()
                    .map(parse_state)
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, ReviewEvidenceAdapterError>>()?;
    Ok(ReviewObservationV1 {
        repository: observation.repository.clone(),
        subject: observation.subject.clone(),
        provider_repository: observation.provider_repository.clone(),
        provider_subject: observation.provider_subject.clone(),
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
        required_claims,
        provider_reviews,
    })
}

fn parse_kind(value: &str) -> Result<ReviewKind, ReviewEvidenceAdapterError> {
    match value {
        "ISSUE_COMMENT" => Ok(ReviewKind::IssueComment),
        "PULL_REQUEST_REVIEW" => Ok(ReviewKind::PullRequestReview),
        _ => Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_KIND_INVALID: unsupported review_kind",
        )),
    }
}

fn parse_status(value: &str) -> Result<ReviewStatus, ReviewEvidenceAdapterError> {
    match value {
        "PASSED" => Ok(ReviewStatus::Passed),
        "FAILED" => Ok(ReviewStatus::Failed),
        _ => Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_STATUS_INVALID: unsupported review status",
        )),
    }
}

fn parse_state(value: &str) -> Result<PullRequestReviewState, ReviewEvidenceAdapterError> {
    match value {
        "APPROVED" => Ok(PullRequestReviewState::Approved),
        "CHANGES_REQUESTED" => Ok(PullRequestReviewState::ChangesRequested),
        "COMMENTED" => Ok(PullRequestReviewState::Commented),
        "DISMISSED" => Ok(PullRequestReviewState::Dismissed),
        "PENDING" => Ok(PullRequestReviewState::Pending),
        _ => Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_STATE_INVALID: unsupported pull-request review state",
        )),
    }
}

fn review_policy(expected_subject: &str) -> Result<ReviewAttestationPolicyV1, ReviewEvidenceAdapterError> {
    ReviewAttestationPolicyV1::new(TARGET, expected_subject).map_err(Into::into)
}

fn hosted_policy(expected_subject: &str) -> Result<EvidencePolicyV1, ReviewEvidenceAdapterError> {
    EvidencePolicyV1::new(binding(expected_subject)?, MAX_VALIDITY_SECONDS).map_err(Into::into)
}

fn hosted_observation(
    observation: &ReviewObservationDto,
) -> Result<HostedEvidenceObservationV1, ReviewEvidenceAdapterError> {
    Ok(HostedEvidenceObservationV1 {
        binding: binding(&observation.subject)?,
        source_run_id: observation.source_run_id,
        source_run_attempt: observation.source_run_attempt,
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
        valid_until_unix_seconds: observation.valid_until_unix_seconds,
        trust_state: EvidenceTrustState::Trusted,
        outcome: EvidenceOutcome::Passed,
        production_mutation: observation.production_mutation,
    })
}

fn binding(subject: &str) -> Result<EvidenceBindingV1, ReviewEvidenceAdapterError> {
    Ok(EvidenceBindingV1 {
        issuer: EvidenceIssuer::new(ISSUER)?,
        source: EvidenceSource::new(SOURCE)?,
        target: EvidenceTarget::new(TARGET)?,
        environment: EvidenceEnvironment::new(ENVIRONMENT)?,
        subject: EvidenceSubject::new(subject)?,
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
        trust_state: "TRUSTED".to_owned(),
        outcome: "PASS".to_owned(),
        production_mutation: envelope.production_mutation,
    }
}

fn envelope_from_dto(dto: &EnvelopeDto) -> Result<HostedEvidenceEnvelopeV1, ReviewEvidenceAdapterError> {
    if dto.schema_version != SCHEMA_VERSION
        || dto.kind != ENVELOPE_KIND
        || dto.trust_state != "TRUSTED"
        || dto.outcome != "PASS"
    {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ENVELOPE_CONTRACT: unsupported envelope contract",
        ));
    }
    Ok(HostedEvidenceEnvelopeV1 {
        binding: EvidenceBindingV1 {
            issuer: EvidenceIssuer::new(dto.issuer.clone())?,
            source: EvidenceSource::new(dto.source.clone())?,
            target: EvidenceTarget::new(dto.target.clone())?,
            environment: EvidenceEnvironment::new(dto.environment.clone())?,
            subject: EvidenceSubject::new(dto.subject.clone())?,
        },
        source_run_id: dto.source_run_id,
        source_run_attempt: dto.source_run_attempt,
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        valid_until_unix_seconds: dto.valid_until_unix_seconds,
        trust_state: EvidenceTrustState::Trusted,
        outcome: EvidenceOutcome::Passed,
        production_mutation: dto.production_mutation,
    })
}

fn render_artifact(
    observation: ReviewObservationDto,
    envelope: &HostedEvidenceEnvelopeV1,
) -> Result<String, ReviewEvidenceAdapterError> {
    let payload = PayloadDto {
        schema_version: SCHEMA_VERSION,
        kind: PAYLOAD_KIND.to_owned(),
        observation,
        envelope: envelope_to_dto(envelope),
    };
    let payload_value = serde_json::to_value(&payload).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_SCHEMA: {error}"))
    })?;
    let canonical_payload = canonical_json(&payload_value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_CANONICAL: {error}"))
    })?;
    let artifact = ArtifactDto {
        schema_version: SCHEMA_VERSION,
        kind: ARTIFACT_KIND.to_owned(),
        digest: DigestDto {
            algorithm: DIGEST_ALGORITHM.to_owned(),
            scope: DIGEST_SCOPE.to_owned(),
            value: sha256_hex(canonical_payload.as_bytes()),
        },
        payload,
    };
    let value = serde_json::to_value(artifact).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    canonical_pretty_json(&value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_CANONICAL: {error}"))
    })
}

fn validate_subject(subject: &str) -> Result<(), ReviewEvidenceAdapterError> {
    if !is_lower_hex(subject, 40) {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_SUBJECT_INVALID: expected subject must be a lowercase 40-character commit SHA",
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

#[cfg(test)]
mod tests {
    use super::{seal_review_json, verify_review_json};
    use serde_json::{Value, json};

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OBSERVED_AT: i64 = 1_700_000_000;
    const EVALUATED_AT: i64 = 1_700_000_010;

    fn observation() -> Value {
        json!({
            "schema_version": 1,
            "kind": "REVIEW_EVIDENCE_OBSERVATION",
            "repository": "iamaman11/part-crm-emai-profile",
            "subject": SUBJECT,
            "provider_repository": "iamaman11/part-crm-emai-profile",
            "provider_subject": SUBJECT,
            "source_run_id": 42,
            "source_run_attempt": 1,
            "observed_at_unix_seconds": OBSERVED_AT,
            "valid_until_unix_seconds": OBSERVED_AT + 3600,
            "production_mutation": false,
            "required_claims": [],
            "provider_reviews": []
        })
    }

    fn rendered(value: &Value) -> String {
        serde_json::to_string(value).expect("json")
    }

    #[test]
    fn canonical_roundtrip_is_byte_stable() {
        let input = rendered(&observation());
        let first = seal_review_json(&input, EVALUATED_AT, SUBJECT).expect("seal");
        let second = seal_review_json(&input, EVALUATED_AT, SUBJECT).expect("seal again");
        assert_eq!(first, second);
        assert_eq!(
            first,
            verify_review_json(&first, EVALUATED_AT, SUBJECT).expect("verify")
        );
    }

    #[test]
    fn version_unknown_duplicate_and_secret_fields_fail_closed() {
        let mut value = observation();
        value["schema_version"] = json!(2);
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());

        let mut value = observation();
        value["token"] = json!("secret");
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());

        let duplicate = format!(
            "{{\"schema_version\":1,\"schema_version\":1,\"kind\":\"REVIEW_EVIDENCE_OBSERVATION\",\"repository\":\"iamaman11/part-crm-emai-profile\",\"subject\":\"{SUBJECT}\",\"provider_repository\":\"iamaman11/part-crm-emai-profile\",\"provider_subject\":\"{SUBJECT}\",\"source_run_id\":42,\"source_run_attempt\":1,\"observed_at_unix_seconds\":{OBSERVED_AT},\"valid_until_unix_seconds\":{},\"production_mutation\":false,\"required_claims\":[],\"provider_reviews\":[]}}",
            OBSERVED_AT + 3600
        );
        assert!(seal_review_json(&duplicate, EVALUATED_AT, SUBJECT).is_err());
    }

    #[test]
    fn generic_hosted_policy_owns_binding_freshness_replay_and_mutation() {
        let mut value = observation();
        value["provider_subject"] = json!("cccccccccccccccccccccccccccccccccccccccc");
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());

        let mut value = observation();
        value["valid_until_unix_seconds"] = json!(EVALUATED_AT);
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());

        let mut value = observation();
        value["valid_until_unix_seconds"] = json!(OBSERVED_AT + 3601);
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());

        let mut value = observation();
        value["production_mutation"] = json!(true);
        assert!(seal_review_json(&rendered(&value), EVALUATED_AT, SUBJECT).is_err());
    }

    #[test]
    fn digest_tamper_and_noncanonical_durable_bytes_fail_closed() {
        let artifact =
            seal_review_json(&rendered(&observation()), EVALUATED_AT, SUBJECT).expect("seal");
        let mut value: Value = serde_json::from_str(&artifact).expect("artifact");
        value["digest"]["value"] =
            json!("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
        let tampered = serde_json::to_string_pretty(&value).expect("tampered");
        assert!(verify_review_json(&tampered, EVALUATED_AT, SUBJECT).is_err());

        let noncanonical = artifact.trim_end().to_owned();
        assert_ne!(artifact, noncanonical);
        assert!(verify_review_json(&noncanonical, EVALUATED_AT, SUBJECT).is_err());
    }
}
