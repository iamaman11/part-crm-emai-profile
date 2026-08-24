use crate::canonical::{canonical_json, canonical_pretty_json, parse_strict_json, sha256_hex};
use opsctl_core::hosted_evidence::{
    EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyError,
    EvidencePolicyV1, EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
    HostedEvidenceEnvelopeV1, HostedEvidenceObservationV1,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

const OBSERVATION_KIND: &str = "HOSTED_EVIDENCE_OBSERVATION";
const ARTIFACT_KIND: &str = "HOSTED_EVIDENCE_ARTIFACT";
const ENVELOPE_KIND: &str = "HOSTED_EVIDENCE_ENVELOPE";
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
    use super::{seal_operational_credential_json, verify_operational_credential_json};
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
}
