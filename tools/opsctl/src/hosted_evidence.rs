use crate::canonical::{canonical_json, canonical_pretty_json, parse_strict_json, sha256_hex};
use opsctl_core::hosted_evidence::{
    EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyError,
    EvidencePolicyV3, EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
    HostedEvidenceEnvelopeV3, HostedEvidenceObservationV3,
    OperationalCredentialAccountObservationV1, OperationalCredentialAttestationObservationV1,
    OperationalCredentialPolicyObservationV1, OperationalCredentialReadObservationV2,
    OperationalCredentialTokenVerifyObservationV1, ReviewAttestationObservationV1,
    ReviewAttestationPolicyV1, ReviewAttestationStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};

const OBSERVATION_KIND: &str = "HOSTED_EVIDENCE_RAW_OBSERVATION";
const ARTIFACT_KIND: &str = "HOSTED_EVIDENCE_ARTIFACT";
const ENVELOPE_KIND: &str = "HOSTED_EVIDENCE_ENVELOPE";
const REVIEW_OBSERVATION_KIND: &str = "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION";
const REVIEW_RESULT_KIND: &str = "EXTERNAL_REVIEW_ATTESTATION_RESULT";
const REVIEW_CLAIM_DOMAIN: &str = "external-evidence-review-v1";
const OBSERVATION_SCHEMA_VERSION: u64 = 3;
const ARTIFACT_SCHEMA_VERSION: u64 = 3;
const ENVELOPE_SCHEMA_VERSION: u64 = 3;
const REVIEW_SCHEMA_VERSION: u64 = 1;
const DIGEST_ALGORITHM: &str = "SHA-256";
const DIGEST_SCOPE: &str = "RFC8785_CANONICAL_OPERATIONAL_CREDENTIAL_EVIDENCE_V3_BYTES";
const OPERATIONAL_CREDENTIAL_ISSUER: &str = "github-actions";
const OPERATIONAL_CREDENTIAL_SOURCE: &str = "github-governance-gate/operational-credential-state";
const HOSTED_EVIDENCE_TARGET: &str = "iamaman11/part-crm-emai-profile";
const OPERATIONAL_CREDENTIAL_ENVIRONMENT: &str = "staging";
const OPERATIONAL_CREDENTIAL_MAX_VALIDITY_SECONDS: u64 = 6 * 60 * 60;
const OPERATIONAL_CREDENTIAL_ID: &str = "cloudflare.staging-observation-api";
const OPERATIONAL_CREDENTIAL_ATTESTATION_KIND: &str =
    "AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION";
const OPERATIONAL_CREDENTIAL_ATTESTATION_SOURCE: &str = "CLOUDFLARE_TOKEN_ISSUANCE_POLICY";
const OPERATIONAL_CREDENTIAL_ACCOUNT_NAME: &str = "pvisakp";
const OPERATIONAL_CREDENTIAL_MUTATION_PROBE: &str = "FORBIDDEN_NOT_EXECUTED";

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CredentialPolicyDto {
    extension_production_mutation: bool,
    credential_id: String,
    environment_scope: Vec<String>,
    allowed_mutator: String,
    mutation_allowed: bool,
    provider_mutation_forbidden: bool,
    required_provider_permissions: Vec<String>,
    forbidden_provider_permission_classes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OperationalCredentialAttestationDto {
    schema_version: u64,
    kind: String,
    environment: String,
    token_id: String,
    account_id: String,
    permission_names: Vec<String>,
    production_scope: bool,
    mutation_capability: bool,
    token_management_capability: bool,
    plaintext_token_included: bool,
    attestation_source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TokenVerifyDto {
    http_status: u16,
    success: bool,
    error_count: usize,
    token_id: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AccountObservationDto {
    http_status: u16,
    success: bool,
    error_count: usize,
    account_id: String,
    account_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadObservationsDto {
    workers_deployments_http_status: u16,
    workers_deployments_success: bool,
    workers_deployments_error_count: usize,
    d1_catalog_exit_code: i32,
    r2_bucket_exit_code: i32,
    queue_exit_code: i32,
    worker_secret_names_exit_code: i32,
    mutation_probe: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    credential_policy: CredentialPolicyDto,
    attestation: OperationalCredentialAttestationDto,
    token_verify: TokenVerifyDto,
    account: AccountObservationDto,
    deployment_account_id: String,
    reads: ReadObservationsDto,
    production_mutation: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TrustStateDto {
    Trusted,
    Untrusted,
    Unknown,
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

impl From<TrustStateDto> for EvidenceTrustState {
    fn from(value: TrustStateDto) -> Self {
        match value {
            TrustStateDto::Trusted => Self::Trusted,
            TrustStateDto::Untrusted => Self::Untrusted,
            TrustStateDto::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OutcomeDto {
    Pass,
    Fail,
}

impl From<EvidenceOutcome> for OutcomeDto {
    fn from(value: EvidenceOutcome) -> Self {
        match value {
            EvidenceOutcome::Passed => Self::Pass,
            EvidenceOutcome::Failed => Self::Fail,
        }
    }
}

impl From<OutcomeDto> for EvidenceOutcome {
    fn from(value: OutcomeDto) -> Self {
        match value {
            OutcomeDto::Pass => Self::Passed,
            OutcomeDto::Fail => Self::Failed,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DigestDto {
    algorithm: String,
    scope: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDto {
    schema_version: u64,
    kind: String,
    digest: DigestDto,
    observation: ObservationDto,
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
    validate_observation_contract(&dto)?;
    let observation = observation_from_dto(&dto)?;
    let envelope = operational_credential_policy(expected_subject)?
        .evaluate(observation, evaluated_at_unix_seconds)?;
    let rendered = render_artifact(&dto, &envelope)?;
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
    validate_observation_contract(&dto.observation)?;
    let observed_digest = evidence_digest(&dto.observation, &dto.envelope)?;
    if dto.digest.value != observed_digest {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_DIGEST_MISMATCH: canonical evidence digest does not match artifact",
        ));
    }

    let observation = observation_from_dto(&dto.observation)?;
    let reevaluated = operational_credential_policy(expected_subject)?
        .evaluate(observation, evaluated_at_unix_seconds)?;
    let supplied_envelope = envelope_from_dto(&dto.envelope)?;
    if reevaluated != supplied_envelope {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_SEMANTIC_ROUNDTRIP_MISMATCH: supplied verdict is not the typed Rust decision for the raw observation",
        ));
    }
    let rendered = render_artifact(&dto.observation, &reevaluated)?;
    if artifact_json != rendered {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ARTIFACT_NOT_CANONICAL: durable artifact bytes must use the canonical pretty projection",
        ));
    }
    Ok(rendered)
}

fn validate_observation_contract(dto: &ObservationDto) -> Result<(), HostedEvidenceAdapterError> {
    if dto.schema_version != OBSERVATION_SCHEMA_VERSION || dto.kind != OBSERVATION_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_OBSERVATION_CONTRACT: unsupported schema_version or kind",
        ));
    }
    Ok(())
}

fn validate_artifact_contract(dto: &ArtifactDto) -> Result<(), HostedEvidenceAdapterError> {
    if dto.schema_version != ARTIFACT_SCHEMA_VERSION || dto.kind != ARTIFACT_KIND {
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

fn observation_from_dto(
    dto: &ObservationDto,
) -> Result<HostedEvidenceObservationV3, HostedEvidenceAdapterError> {
    Ok(HostedEvidenceObservationV3 {
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
        credential_policy: OperationalCredentialPolicyObservationV1 {
            extension_production_mutation: dto.credential_policy.extension_production_mutation,
            credential_id: dto.credential_policy.credential_id.clone(),
            environment_scope: dto.credential_policy.environment_scope.clone(),
            allowed_mutator: dto.credential_policy.allowed_mutator.clone(),
            mutation_allowed: dto.credential_policy.mutation_allowed,
            provider_mutation_forbidden: dto.credential_policy.provider_mutation_forbidden,
            required_provider_permissions: dto
                .credential_policy
                .required_provider_permissions
                .clone(),
            forbidden_provider_permission_classes: dto
                .credential_policy
                .forbidden_provider_permission_classes
                .clone(),
        },
        attestation: OperationalCredentialAttestationObservationV1 {
            schema_version: dto.attestation.schema_version,
            kind: dto.attestation.kind.clone(),
            environment: dto.attestation.environment.clone(),
            token_id: dto.attestation.token_id.clone(),
            account_id: dto.attestation.account_id.clone(),
            permission_names: dto.attestation.permission_names.clone(),
            production_scope: dto.attestation.production_scope,
            mutation_capability: dto.attestation.mutation_capability,
            token_management_capability: dto.attestation.token_management_capability,
            plaintext_token_included: dto.attestation.plaintext_token_included,
            attestation_source: dto.attestation.attestation_source.clone(),
        },
        token_verify: OperationalCredentialTokenVerifyObservationV1 {
            http_status: dto.token_verify.http_status,
            success: dto.token_verify.success,
            error_count: dto.token_verify.error_count,
            token_id: dto.token_verify.token_id.clone(),
            status: dto.token_verify.status.clone(),
        },
        account: OperationalCredentialAccountObservationV1 {
            http_status: dto.account.http_status,
            success: dto.account.success,
            error_count: dto.account.error_count,
            account_id: dto.account.account_id.clone(),
            account_name: dto.account.account_name.clone(),
        },
        deployment_account_id: dto.deployment_account_id.clone(),
        reads: OperationalCredentialReadObservationV2 {
            workers_deployments_http_status: dto.reads.workers_deployments_http_status,
            workers_deployments_success: dto.reads.workers_deployments_success,
            workers_deployments_error_count: dto.reads.workers_deployments_error_count,
            d1_catalog_exit_code: dto.reads.d1_catalog_exit_code,
            r2_bucket_exit_code: dto.reads.r2_bucket_exit_code,
            queue_exit_code: dto.reads.queue_exit_code,
            worker_secret_names_exit_code: dto.reads.worker_secret_names_exit_code,
            mutation_probe: dto.reads.mutation_probe.clone(),
        },
        production_mutation: dto.production_mutation,
    })
}

fn envelope_from_dto(
    dto: &EnvelopeDto,
) -> Result<HostedEvidenceEnvelopeV3, HostedEvidenceAdapterError> {
    if dto.schema_version != ENVELOPE_SCHEMA_VERSION || dto.kind != ENVELOPE_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_EVIDENCE_ENVELOPE_CONTRACT: unsupported schema_version or kind",
        ));
    }
    Ok(HostedEvidenceEnvelopeV3 {
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
        trust_state: dto.trust_state.into(),
        outcome: dto.outcome.into(),
        production_mutation: dto.production_mutation,
    })
}

fn envelope_to_dto(envelope: &HostedEvidenceEnvelopeV3) -> EnvelopeDto {
    EnvelopeDto {
        schema_version: ENVELOPE_SCHEMA_VERSION,
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

fn evidence_digest(
    observation: &ObservationDto,
    envelope: &EnvelopeDto,
) -> Result<String, HostedEvidenceAdapterError> {
    let value = json!({
        "observation": observation,
        "envelope": envelope,
    });
    let canonical = canonical_json(&value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_CANONICAL: {error}"))
    })?;
    Ok(sha256_hex(canonical.as_bytes()))
}

fn render_artifact(
    observation: &ObservationDto,
    envelope: &HostedEvidenceEnvelopeV3,
) -> Result<String, HostedEvidenceAdapterError> {
    let envelope = envelope_to_dto(envelope);
    let artifact = ArtifactDto {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        kind: ARTIFACT_KIND.to_owned(),
        digest: DigestDto {
            algorithm: DIGEST_ALGORITHM.to_owned(),
            scope: DIGEST_SCOPE.to_owned(),
            value: evidence_digest(observation, &envelope)?,
        },
        observation: observation.clone(),
        envelope,
    };
    let value = serde_json::to_value(artifact).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    canonical_pretty_json(&value).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_EVIDENCE_ARTIFACT_CANONICAL: {error}"))
    })
}

fn operational_credential_policy(
    expected_subject: &str,
) -> Result<EvidencePolicyV3, HostedEvidenceAdapterError> {
    let binding = EvidenceBindingV1 {
        issuer: EvidenceIssuer::new(OPERATIONAL_CREDENTIAL_ISSUER)?,
        source: EvidenceSource::new(OPERATIONAL_CREDENTIAL_SOURCE)?,
        target: EvidenceTarget::new(HOSTED_EVIDENCE_TARGET)?,
        environment: EvidenceEnvironment::new(OPERATIONAL_CREDENTIAL_ENVIRONMENT)?,
        subject: EvidenceSubject::new(expected_subject)?,
    };
    EvidencePolicyV3::new(
        binding,
        OPERATIONAL_CREDENTIAL_MAX_VALIDITY_SECONDS,
        OPERATIONAL_CREDENTIAL_ID,
        OPERATIONAL_CREDENTIAL_ATTESTATION_KIND,
        OPERATIONAL_CREDENTIAL_ATTESTATION_SOURCE,
        OPERATIONAL_CREDENTIAL_ACCOUNT_NAME,
        OPERATIONAL_CREDENTIAL_MUTATION_PROBE,
    )
    .map_err(Into::into)
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
    if dto.schema_version != REVIEW_SCHEMA_VERSION || dto.kind != REVIEW_OBSERVATION_KIND {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_OBSERVATION_CONTRACT: unsupported schema_version or kind",
        ));
    }

    let declared_repository = EvidenceTarget::new(dto.repository)?;
    let expected_repository = EvidenceTarget::new(HOSTED_EVIDENCE_TARGET)?;
    if !declared_repository
        .as_str()
        .eq_ignore_ascii_case(expected_repository.as_str())
    {
        return Err(HostedEvidenceAdapterError::new(
            "HOSTED_REVIEW_ATTESTATION_REPOSITORY_BINDING_MISMATCH: observation repository does not match the canonical hosted-evidence target",
        ));
    }
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
        schema_version: REVIEW_SCHEMA_VERSION,
        kind: REVIEW_RESULT_KIND,
        verified_records,
    };
    let value = serde_json::to_value(result).map_err(|error| {
        HostedEvidenceAdapterError::new(format!("HOSTED_REVIEW_ATTESTATION_RESULT_SCHEMA: {error}"))
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

fn record_string<'a>(
    record: &'a Value,
    field: &str,
) -> Result<&'a str, HostedEvidenceAdapterError> {
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

#[cfg(test)]
mod tests {
    use super::{
        HostedEvidenceAdapterError, claim_sha256_for_record, object_string, record_string,
        seal_operational_credential_json, verify_external_review_attestations_json,
        verify_operational_credential_json,
    };
    use serde_json::{Value, json};

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OBSERVED_AT: i64 = 1_700_000_000;
    const EVALUATED_AT: i64 = 1_700_000_010;
    const REVIEW_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";

    fn observation() -> Value {
        let required = json!([
            "D1 Read",
            "Queues Read",
            "Workers R2 Storage Read",
            "Workers Scripts Read"
        ]);
        json!({
            "schema_version": 3,
            "kind": "HOSTED_EVIDENCE_RAW_OBSERVATION",
            "issuer": "github-actions",
            "source": "github-governance-gate/operational-credential-state",
            "target": "iamaman11/part-crm-emai-profile",
            "environment": "staging",
            "subject": SUBJECT,
            "source_run_id": 123456,
            "source_run_attempt": 1,
            "observed_at_unix_seconds": OBSERVED_AT,
            "valid_until_unix_seconds": OBSERVED_AT + 3600,
            "credential_policy": {
                "extension_production_mutation": false,
                "credential_id": "cloudflare.staging-observation-api",
                "environment_scope": ["staging"],
                "allowed_mutator": "NONE",
                "mutation_allowed": false,
                "provider_mutation_forbidden": true,
                "required_provider_permissions": required,
                "forbidden_provider_permission_classes": [
                    "API Tokens Write",
                    "D1 Write",
                    "Queues Write",
                    "Workers R2 Storage Write",
                    "Workers Scripts Write"
                ]
            },
            "attestation": {
                "schema_version": 1,
                "kind": "AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION",
                "environment": "staging",
                "token_id": "observe-token-id-1234",
                "account_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "permission_names": [
                    "D1 Read",
                    "Queues Read",
                    "Workers R2 Storage Read",
                    "Workers Scripts Read"
                ],
                "production_scope": false,
                "mutation_capability": false,
                "token_management_capability": false,
                "plaintext_token_included": false,
                "attestation_source": "CLOUDFLARE_TOKEN_ISSUANCE_POLICY"
            },
            "token_verify": {
                "http_status": 200,
                "success": true,
                "error_count": 0,
                "token_id": "observe-token-id-1234",
                "status": "active"
            },
            "account": {
                "http_status": 200,
                "success": true,
                "error_count": 0,
                "account_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "account_name": "pvisakp"
            },
            "deployment_account_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "reads": {
                "workers_deployments_http_status": 200,
                "workers_deployments_success": true,
                "workers_deployments_error_count": 0,
                "d1_catalog_exit_code": 0,
                "r2_bucket_exit_code": 0,
                "queue_exit_code": 0,
                "worker_secret_names_exit_code": 0,
                "mutation_probe": "FORBIDDEN_NOT_EXECUTED"
            },
            "production_mutation": false
        })
    }

    fn seal(value: &Value) -> Result<String, super::HostedEvidenceAdapterError> {
        seal_operational_credential_json(&value.to_string(), EVALUATED_AT, SUBJECT)
    }

    #[test]
    fn raw_v3_roundtrips_to_rust_derived_verdict() -> Result<(), Box<dyn std::error::Error>> {
        let artifact = seal(&observation())?;
        let verified = verify_operational_credential_json(&artifact, EVALUATED_AT, SUBJECT)?;
        assert_eq!(artifact, verified);
        assert!(artifact.contains("\"trust_state\": \"TRUSTED\""));
        assert!(artifact.contains("\"outcome\": \"PASS\""));
        assert!(artifact.contains("\"account_name\": \"pvisakp\""));
        Ok(())
    }

    #[test]
    fn raw_v3_rejects_injected_verdict_legacy_shape_and_unknown_fields() {
        let mut verdict = observation();
        verdict["trust_state"] = Value::String("TRUSTED".to_owned());
        verdict["outcome"] = Value::String("PASS".to_owned());
        assert!(seal(&verdict).is_err());

        let mut legacy = observation();
        legacy["schema_version"] = json!(2);
        legacy["reads"] = json!({
            "workers_deployments_read": true,
            "d1_catalog_read": true,
            "r2_bucket_read": true,
            "queue_read": true,
            "worker_secret_names_read": true,
            "mutation_probe": "FORBIDDEN_NOT_EXECUTED"
        });
        assert!(seal(&legacy).is_err());

        let mut secret = observation();
        secret["token"] = Value::String("must-not-cross-boundary".to_owned());
        assert!(seal(&secret).is_err());
    }

    #[test]
    fn raw_v3_fails_closed_on_account_http_and_exit_drift() {
        let mut account = observation();
        account["account"]["account_name"] = json!("wrong-account");
        assert!(seal(&account).is_err());

        let mut deployment = observation();
        deployment["reads"]["workers_deployments_http_status"] = json!(404);
        deployment["reads"]["workers_deployments_success"] = json!(false);
        assert!(seal(&deployment).is_err());

        let mut wrangler = observation();
        wrangler["reads"]["r2_bucket_exit_code"] = json!(1);
        assert!(seal(&wrangler).is_err());
    }

    #[test]
    fn artifact_digest_and_semantic_verdict_remain_tamper_evident()
    -> Result<(), Box<dyn std::error::Error>> {
        let artifact = seal(&observation())?;
        let mut value: Value = serde_json::from_str(&artifact)?;
        value["digest"]["value"] = Value::String("0".repeat(64));
        let tampered = serde_json::to_string_pretty(&value)? + "\n";
        assert!(verify_operational_credential_json(&tampered, EVALUATED_AT, SUBJECT).is_err());
        Ok(())
    }

    fn terminal_record(evidence_id: &str, reference: &str) -> Value {
        json!({
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
        })
    }

    fn observed_record(record: Value) -> Result<Value, HostedEvidenceAdapterError> {
        let review = record
            .get("review")
            .and_then(Value::as_object)
            .ok_or_else(|| HostedEvidenceAdapterError::new("fixture review object missing"))?;
        let reference = object_string(review, "review_reference", "fixture.review")?.to_owned();
        let digest = claim_sha256_for_record(&record)?;
        let evidence_id = record_string(&record, "evidence_id")?.to_owned();
        let gate = record_string(&record, "gate")?.to_owned();
        let status = record_string(&record, "status")?.to_owned();
        Ok(json!({
            "record": record,
            "review_repository": REVIEW_REPOSITORY,
            "review_reference": reference,
            "provider_object": {
                "available": true,
                "login": "reviewer-one",
                "body": format!(
                    "external-evidence-review-v1\nevidence_id={evidence_id}\ngate={gate}\nstatus={status}\nclaim_sha256={digest}"
                ),
                "effective_timestamp": "2026-08-06T14:40:00Z"
            }
        }))
    }

    #[test]
    fn external_review_adapter_still_uses_typed_policy() -> Result<(), Box<dyn std::error::Error>> {
        let record = terminal_record(
            "ev-20260806-terminal",
            "https://github.com/iamaman11/part-crm-emai-profile/issues/9#issuecomment-101",
        );
        let batch = json!({
            "schema_version": 1,
            "kind": "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION",
            "repository": REVIEW_REPOSITORY,
            "records": [observed_record(record)?]
        });
        let result = verify_external_review_attestations_json(&batch.to_string())?;
        assert!(result.contains("\"verified_records\": 1"));
        Ok(())
    }
}
