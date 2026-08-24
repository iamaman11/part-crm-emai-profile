use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

const MAX_IDENTIFIER_BYTES: usize = 256;
const REVIEW_CLAIM_DOMAIN: &str = "external-evidence-review-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyError {
    code: &'static str,
    detail: String,
}

impl EvidencePolicyError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Display for EvidencePolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for EvidencePolicyError {}

fn validate_identifier(field: &'static str, value: String) -> Result<String, EvidencePolicyError> {
    if value.is_empty() {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not be empty"),
        ));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} exceeds {MAX_IDENTIFIER_BYTES} bytes"),
        ));
    }
    if value.trim() != value {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not contain leading or trailing whitespace"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not contain control characters"),
        ));
    }
    Ok(value)
}

macro_rules! evidence_identifier {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, EvidencePolicyError> {
                validate_identifier($field, value.into()).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

evidence_identifier!(EvidenceIssuer, "issuer");
evidence_identifier!(EvidenceSource, "source");
evidence_identifier!(EvidenceTarget, "target");
evidence_identifier!(EvidenceEnvironment, "environment");
evidence_identifier!(EvidenceSubject, "subject");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceTrustState {
    Trusted,
    Untrusted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBindingV1 {
    pub issuer: EvidenceIssuer,
    pub source: EvidenceSource,
    pub target: EvidenceTarget,
    pub environment: EvidenceEnvironment,
    pub subject: EvidenceSubject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialPolicyObservationV1 {
    pub extension_production_mutation: bool,
    pub credential_id: String,
    pub environment_scope: Vec<String>,
    pub allowed_mutator: String,
    pub mutation_allowed: bool,
    pub provider_mutation_forbidden: bool,
    pub required_provider_permissions: Vec<String>,
    pub forbidden_provider_permission_classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialAttestationObservationV1 {
    pub schema_version: u64,
    pub kind: String,
    pub environment: String,
    pub token_id: String,
    pub account_id: String,
    pub permission_names: Vec<String>,
    pub production_scope: bool,
    pub mutation_capability: bool,
    pub token_management_capability: bool,
    pub plaintext_token_included: bool,
    pub attestation_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialTokenVerifyObservationV1 {
    pub http_status: u16,
    pub success: bool,
    pub error_count: usize,
    pub token_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialReadObservationV1 {
    pub workers_deployments_read: bool,
    pub d1_catalog_read: bool,
    pub r2_bucket_read: bool,
    pub queue_read: bool,
    pub worker_secret_names_read: bool,
    pub mutation_probe: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceObservationV2 {
    pub binding: EvidenceBindingV1,
    pub source_run_id: u64,
    pub source_run_attempt: u32,
    pub observed_at_unix_seconds: i64,
    pub valid_until_unix_seconds: i64,
    pub credential_policy: OperationalCredentialPolicyObservationV1,
    pub attestation: OperationalCredentialAttestationObservationV1,
    pub token_verify: OperationalCredentialTokenVerifyObservationV1,
    pub deployment_account_id: String,
    pub reads: OperationalCredentialReadObservationV1,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceEnvelopeV2 {
    pub binding: EvidenceBindingV1,
    pub source_run_id: u64,
    pub source_run_attempt: u32,
    pub observed_at_unix_seconds: i64,
    pub valid_until_unix_seconds: i64,
    pub trust_state: EvidenceTrustState,
    pub outcome: EvidenceOutcome,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyV2 {
    expected_binding: EvidenceBindingV1,
    max_validity_seconds: i64,
    expected_credential_id: String,
    expected_attestation_kind: String,
    expected_attestation_source: String,
    expected_mutation_probe: String,
}

impl EvidencePolicyV2 {
    pub fn new(
        expected_binding: EvidenceBindingV1,
        max_validity_seconds: u64,
        expected_credential_id: impl Into<String>,
        expected_attestation_kind: impl Into<String>,
        expected_attestation_source: impl Into<String>,
        expected_mutation_probe: impl Into<String>,
    ) -> Result<Self, EvidencePolicyError> {
        let max_validity_seconds = i64::try_from(max_validity_seconds).map_err(|_| {
            EvidencePolicyError::new(
                "HOSTED_EVIDENCE_POLICY_INVALID",
                "max validity does not fit the supported timestamp domain",
            )
        })?;
        if max_validity_seconds <= 0 {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_POLICY_INVALID",
                "max validity must be greater than zero",
            ));
        }
        Ok(Self {
            expected_binding,
            max_validity_seconds,
            expected_credential_id: validate_identifier(
                "expected_credential_id",
                expected_credential_id.into(),
            )?,
            expected_attestation_kind: validate_identifier(
                "expected_attestation_kind",
                expected_attestation_kind.into(),
            )?,
            expected_attestation_source: validate_identifier(
                "expected_attestation_source",
                expected_attestation_source.into(),
            )?,
            expected_mutation_probe: validate_identifier(
                "expected_mutation_probe",
                expected_mutation_probe.into(),
            )?,
        })
    }

    pub fn evaluate(
        &self,
        observation: HostedEvidenceObservationV2,
        evaluated_at_unix_seconds: i64,
    ) -> Result<HostedEvidenceEnvelopeV2, EvidencePolicyError> {
        if observation.binding != self.expected_binding {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_BINDING_MISMATCH",
                "issuer/source/target/environment/subject binding does not match the expected consumer",
            ));
        }
        validate_freshness(
            observation.source_run_id,
            observation.source_run_attempt,
            observation.observed_at_unix_seconds,
            observation.valid_until_unix_seconds,
            evaluated_at_unix_seconds,
            self.max_validity_seconds,
        )?;
        if observation.production_mutation
            || observation.credential_policy.extension_production_mutation
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_PRODUCTION_MUTATION_FORBIDDEN",
                "PF-2 hosted evidence must prove production_mutation=false at observation and credential-policy boundaries",
            ));
        }
        validate_credential_policy(self, &observation)?;
        validate_attestation(self, &observation)?;
        validate_token_verify(&observation)?;
        validate_read_observations(self, &observation)?;

        Ok(HostedEvidenceEnvelopeV2 {
            binding: observation.binding,
            source_run_id: observation.source_run_id,
            source_run_attempt: observation.source_run_attempt,
            observed_at_unix_seconds: observation.observed_at_unix_seconds,
            valid_until_unix_seconds: observation.valid_until_unix_seconds,
            trust_state: EvidenceTrustState::Trusted,
            outcome: EvidenceOutcome::Passed,
            production_mutation: false,
        })
    }
}

fn validate_freshness(
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    evaluated_at_unix_seconds: i64,
    max_validity_seconds: i64,
) -> Result<(), EvidencePolicyError> {
    if source_run_id == 0 || source_run_attempt == 0 {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_RUN_IDENTITY_INVALID",
            "source run id and attempt must both be greater than zero",
        ));
    }
    if observed_at_unix_seconds <= 0
        || valid_until_unix_seconds <= 0
        || evaluated_at_unix_seconds <= 0
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TIMESTAMP_INVALID",
            "observation, validity and evaluation timestamps must be positive Unix seconds",
        ));
    }
    let validity_seconds = valid_until_unix_seconds
        .checked_sub(observed_at_unix_seconds)
        .ok_or_else(|| {
            EvidencePolicyError::new(
                "HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID",
                "validity window arithmetic overflowed",
            )
        })?;
    if validity_seconds <= 0 {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID",
            "valid_until must be strictly later than observed_at",
        ));
    }
    if validity_seconds > max_validity_seconds {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_FRESHNESS_WINDOW_TOO_LARGE",
            format!(
                "validity window {validity_seconds}s exceeds policy maximum {max_validity_seconds}s"
            ),
        ));
    }
    if evaluated_at_unix_seconds < observed_at_unix_seconds {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_OBSERVATION_FROM_FUTURE",
            "evaluation time precedes the observation time",
        ));
    }
    if evaluated_at_unix_seconds >= valid_until_unix_seconds {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_EXPIRED_OR_REPLAYED",
            "evidence is expired at the explicit evaluation time",
        ));
    }
    Ok(())
}

fn validate_credential_policy(
    policy: &EvidencePolicyV2,
    observation: &HostedEvidenceObservationV2,
) -> Result<(), EvidencePolicyError> {
    let credential = &observation.credential_policy;
    if credential.credential_id != policy.expected_credential_id {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_CREDENTIAL_ID_MISMATCH",
            "observed credential policy is not the expected operational credential",
        ));
    }
    if credential.environment_scope.len() != 1
        || credential.environment_scope[0] != observation.binding.environment.as_str()
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_CREDENTIAL_SCOPE_INVALID",
            "operational credential policy must be scoped only to the bound environment",
        ));
    }
    if credential.allowed_mutator != "NONE"
        || credential.mutation_allowed
        || !credential.provider_mutation_forbidden
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_CREDENTIAL_MUTATION_AUTHORITY_FORBIDDEN",
            "operational observation credential must have no mutation authority",
        ));
    }
    let required = unique_nonempty_set(
        &credential.required_provider_permissions,
        "required_provider_permissions",
    )?;
    let forbidden = unique_nonempty_set(
        &credential.forbidden_provider_permission_classes,
        "forbidden_provider_permission_classes",
    )?;
    if required.is_empty() || forbidden.is_empty() {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PERMISSION_POLICY_INVALID",
            "required and forbidden permission sets must both be non-empty",
        ));
    }
    if !required.is_disjoint(&forbidden) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PERMISSION_POLICY_INVALID",
            "required and forbidden permission sets must be disjoint",
        ));
    }
    Ok(())
}

fn validate_attestation(
    policy: &EvidencePolicyV2,
    observation: &HostedEvidenceObservationV2,
) -> Result<(), EvidencePolicyError> {
    let attestation = &observation.attestation;
    if attestation.schema_version != 1
        || attestation.kind != policy.expected_attestation_kind
        || attestation.attestation_source != policy.expected_attestation_source
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ATTESTATION_CONTRACT_INVALID",
            "operational credential attestation identity or source drifted",
        ));
    }
    if attestation.environment != observation.binding.environment.as_str() {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ATTESTATION_ENVIRONMENT_MISMATCH",
            "attestation environment does not match the evidence binding",
        ));
    }
    if attestation.production_scope
        || attestation.mutation_capability
        || attestation.token_management_capability
        || attestation.plaintext_token_included
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ATTESTATION_AUTHORITY_INVALID",
            "attestation must prove no production, mutation, token-management or plaintext-token authority",
        ));
    }
    if !is_token_id(&attestation.token_id) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_ID_INVALID",
            "attested token id is malformed",
        ));
    }
    if !is_lower_hex(&attestation.account_id, 32)
        || !is_lower_hex(&observation.deployment_account_id, 32)
        || attestation.account_id != observation.deployment_account_id
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH",
            "attested account must exactly match the observed deployment account",
        ));
    }
    let required = unique_nonempty_set(
        &observation.credential_policy.required_provider_permissions,
        "required_provider_permissions",
    )?;
    let forbidden = unique_nonempty_set(
        &observation
            .credential_policy
            .forbidden_provider_permission_classes,
        "forbidden_provider_permission_classes",
    )?;
    let attested = unique_nonempty_set(
        &attestation.permission_names,
        "attestation.permission_names",
    )?;
    if attested != required {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PERMISSION_MISMATCH",
            "attested permissions must exactly match required read-only permissions",
        ));
    }
    if !attested.is_disjoint(&forbidden) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_FORBIDDEN_PERMISSION",
            "attestation contains a forbidden permission class",
        ));
    }
    Ok(())
}

fn validate_token_verify(
    observation: &HostedEvidenceObservationV2,
) -> Result<(), EvidencePolicyError> {
    let verify = &observation.token_verify;
    if verify.http_status != 200 || !verify.success || verify.error_count != 0 {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_VERIFY_FAILED",
            "token verification did not return one successful error-free response",
        ));
    }
    if verify.token_id != observation.attestation.token_id {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_BINDING_MISMATCH",
            "verified token id does not match the issuance attestation",
        ));
    }
    if verify.status != "active" {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_INACTIVE",
            "verified token status must be active",
        ));
    }
    Ok(())
}

fn validate_read_observations(
    policy: &EvidencePolicyV2,
    observation: &HostedEvidenceObservationV2,
) -> Result<(), EvidencePolicyError> {
    let reads = &observation.reads;
    if !(reads.workers_deployments_read
        && reads.d1_catalog_read
        && reads.r2_bucket_read
        && reads.queue_read
        && reads.worker_secret_names_read)
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_REQUIRED_READ_INCOMPLETE",
            "all required provider read observations must succeed",
        ));
    }
    if reads.mutation_probe != policy.expected_mutation_probe {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_MUTATION_PROBE_INVALID",
            "mutation probe must remain forbidden and unexecuted",
        ));
    }
    Ok(())
}

fn unique_nonempty_set<'a>(
    values: &'a [String],
    field: &'static str,
) -> Result<BTreeSet<&'a str>, EvidencePolicyError> {
    let mut set = BTreeSet::new();
    for value in values {
        if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_PERMISSION_POLICY_INVALID",
                format!("{field} contains an invalid value"),
            ));
        }
        if !set.insert(value.as_str()) {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_PERMISSION_POLICY_INVALID",
                format!("{field} contains duplicate values"),
            ));
        }
    }
    Ok(set)
}

fn is_token_id(value: &str) -> bool {
    (8..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewAttestationStatus {
    Passed,
    Failed,
}

impl ReviewAttestationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationObservationV1 {
    pub expected_repository: EvidenceTarget,
    pub observed_repository: EvidenceTarget,
    pub evidence_id: EvidenceSubject,
    pub gate: EvidenceSource,
    pub status: ReviewAttestationStatus,
    pub expected_reference: String,
    pub observed_reference: String,
    pub expected_reviewer: String,
    pub observed_reviewer: Option<String>,
    pub expected_reviewed_at: String,
    pub observed_reviewed_at: Option<String>,
    pub claim_sha256: String,
    pub observed_body: Option<String>,
    pub provider_object_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewAttestationPolicyV1;

impl ReviewAttestationPolicyV1 {
    pub fn evaluate(
        self,
        observation: &ReviewAttestationObservationV1,
    ) -> Result<(), EvidencePolicyError> {
        if !observation.provider_object_available {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_PROVIDER_OBJECT_UNAVAILABLE",
                "the exact referenced GitHub review/comment object is unavailable",
            ));
        }
        if !observation
            .expected_repository
            .as_str()
            .eq_ignore_ascii_case(observation.observed_repository.as_str())
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REPOSITORY_MISMATCH",
                "the observed review object belongs to a different repository",
            ));
        }
        if observation.expected_reference != observation.observed_reference {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REFERENCE_MISMATCH",
                "the observed provider object does not match the record review reference",
            ));
        }
        let observed_reviewer = observation.observed_reviewer.as_deref().ok_or_else(|| {
            EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REVIEWER_MISSING",
                "the observed provider object has no reviewer identity",
            )
        })?;
        if !observation
            .expected_reviewer
            .eq_ignore_ascii_case(observed_reviewer)
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REVIEWER_MISMATCH",
                "the observed reviewer does not match the terminal evidence record",
            ));
        }
        let observed_reviewed_at =
            observation.observed_reviewed_at.as_deref().ok_or_else(|| {
                EvidencePolicyError::new(
                    "HOSTED_REVIEW_ATTESTATION_TIMESTAMP_MISSING",
                    "the observed provider object has no effective review timestamp",
                )
            })?;
        if observation.expected_reviewed_at != observed_reviewed_at {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_TIMESTAMP_MISMATCH",
                "the observed review timestamp does not match the terminal evidence record",
            ));
        }
        if observation.claim_sha256.len() != 64
            || !observation
                .claim_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_CLAIM_DIGEST_INVALID",
                "claim_sha256 must be exactly 64 lowercase hexadecimal characters",
            ));
        }
        let observed_body = observation.observed_body.as_deref().ok_or_else(|| {
            EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_BODY_MISSING",
                "the observed provider object has no review body",
            )
        })?;
        let expected_body = format!(
            "{REVIEW_CLAIM_DOMAIN}\nevidence_id={}\ngate={}\nstatus={}\nclaim_sha256={}",
            observation.evidence_id.as_str(),
            observation.gate.as_str(),
            observation.status.as_str(),
            observation.claim_sha256,
        );
        if observed_body != expected_body {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_BODY_MISMATCH",
                "the observed GitHub review body does not match the canonical terminal claim",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyV2,
        EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
        HostedEvidenceObservationV2, OperationalCredentialAttestationObservationV1,
        OperationalCredentialPolicyObservationV1, OperationalCredentialReadObservationV1,
        OperationalCredentialTokenVerifyObservationV1, ReviewAttestationObservationV1,
        ReviewAttestationPolicyV1, ReviewAttestationStatus,
    };

    fn binding(subject: &str) -> Result<EvidenceBindingV1, Box<dyn std::error::Error>> {
        Ok(EvidenceBindingV1 {
            issuer: EvidenceIssuer::new("github-actions")?,
            source: EvidenceSource::new("github-governance-gate/operational-credential-state")?,
            target: EvidenceTarget::new("iamaman11/part-crm-emai-profile")?,
            environment: EvidenceEnvironment::new("staging")?,
            subject: EvidenceSubject::new(subject)?,
        })
    }

    fn observation() -> Result<HostedEvidenceObservationV2, Box<dyn std::error::Error>> {
        let required = vec![
            "D1 Read".to_owned(),
            "Queues Read".to_owned(),
            "Workers R2 Storage Read".to_owned(),
            "Workers Scripts Read".to_owned(),
        ];
        Ok(HostedEvidenceObservationV2 {
            binding: binding("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?,
            source_run_id: 42,
            source_run_attempt: 1,
            observed_at_unix_seconds: 1_700_000_000,
            valid_until_unix_seconds: 1_700_003_600,
            credential_policy: OperationalCredentialPolicyObservationV1 {
                extension_production_mutation: false,
                credential_id: "cloudflare.staging-observation-api".to_owned(),
                environment_scope: vec!["staging".to_owned()],
                allowed_mutator: "NONE".to_owned(),
                mutation_allowed: false,
                provider_mutation_forbidden: true,
                required_provider_permissions: required.clone(),
                forbidden_provider_permission_classes: vec![
                    "API Tokens Write".to_owned(),
                    "D1 Write".to_owned(),
                    "Queues Write".to_owned(),
                    "Workers R2 Storage Write".to_owned(),
                    "Workers Scripts Write".to_owned(),
                ],
            },
            attestation: OperationalCredentialAttestationObservationV1 {
                schema_version: 1,
                kind: "AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION".to_owned(),
                environment: "staging".to_owned(),
                token_id: "observe-token-id-1234".to_owned(),
                account_id: "a".repeat(32),
                permission_names: required,
                production_scope: false,
                mutation_capability: false,
                token_management_capability: false,
                plaintext_token_included: false,
                attestation_source: "CLOUDFLARE_TOKEN_ISSUANCE_POLICY".to_owned(),
            },
            token_verify: OperationalCredentialTokenVerifyObservationV1 {
                http_status: 200,
                success: true,
                error_count: 0,
                token_id: "observe-token-id-1234".to_owned(),
                status: "active".to_owned(),
            },
            deployment_account_id: "a".repeat(32),
            reads: OperationalCredentialReadObservationV1 {
                workers_deployments_read: true,
                d1_catalog_read: true,
                r2_bucket_read: true,
                queue_read: true,
                worker_secret_names_read: true,
                mutation_probe: "FORBIDDEN_NOT_EXECUTED".to_owned(),
            },
            production_mutation: false,
        })
    }

    fn policy() -> Result<EvidencePolicyV2, Box<dyn std::error::Error>> {
        Ok(EvidencePolicyV2::new(
            binding("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?,
            3_600,
            "cloudflare.staging-observation-api",
            "AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION",
            "CLOUDFLARE_TOKEN_ISSUANCE_POLICY",
            "FORBIDDEN_NOT_EXECUTED",
        )?)
    }

    fn review_observation() -> Result<ReviewAttestationObservationV1, Box<dyn std::error::Error>> {
        let digest = "11".repeat(32);
        Ok(ReviewAttestationObservationV1 {
            expected_repository: EvidenceTarget::new("acme/profile-platform")?,
            observed_repository: EvidenceTarget::new("acme/profile-platform")?,
            evidence_id: EvidenceSubject::new("ev-20260806-terminal")?,
            gate: EvidenceSource::new("product_license")?,
            status: ReviewAttestationStatus::Passed,
            expected_reference:
                "https://github.com/acme/profile-platform/issues/9#issuecomment-101".to_owned(),
            observed_reference:
                "https://github.com/acme/profile-platform/issues/9#issuecomment-101".to_owned(),
            expected_reviewer: "reviewer-one".to_owned(),
            observed_reviewer: Some("Reviewer-One".to_owned()),
            expected_reviewed_at: "2026-08-06T14:40:00Z".to_owned(),
            observed_reviewed_at: Some("2026-08-06T14:40:00Z".to_owned()),
            claim_sha256: digest.clone(),
            observed_body: Some(format!(
                "external-evidence-review-v1\nevidence_id=ev-20260806-terminal\ngate=product_license\nstatus=passed\nclaim_sha256={digest}"
            )),
            provider_object_available: true,
        })
    }

    #[test]
    fn derives_trusted_passed_only_from_raw_facts() -> Result<(), Box<dyn std::error::Error>> {
        let envelope = policy()?.evaluate(observation()?, 1_700_000_010)?;
        assert_eq!(envelope.trust_state, EvidenceTrustState::Trusted);
        assert_eq!(envelope.outcome, EvidenceOutcome::Passed);
        assert!(!envelope.production_mutation);
        Ok(())
    }

    #[test]
    fn rejects_binding_policy_and_permission_drift() -> Result<(), Box<dyn std::error::Error>> {
        let mut foreign = observation()?;
        foreign.binding.target = EvidenceTarget::new("other/repository")?;
        assert_eq!(
            policy()?
                .evaluate(foreign, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_BINDING_MISMATCH")
        );

        let mut mutable = observation()?;
        mutable.credential_policy.mutation_allowed = true;
        assert_eq!(
            policy()?
                .evaluate(mutable, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_CREDENTIAL_MUTATION_AUTHORITY_FORBIDDEN")
        );

        let mut permission_drift = observation()?;
        permission_drift.attestation.permission_names.pop();
        assert_eq!(
            policy()?
                .evaluate(permission_drift, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_PERMISSION_MISMATCH")
        );
        Ok(())
    }

    #[test]
    fn rejects_token_account_read_and_mutation_drift() -> Result<(), Box<dyn std::error::Error>> {
        let mut inactive = observation()?;
        inactive.token_verify.status = "disabled".to_owned();
        assert_eq!(
            policy()?
                .evaluate(inactive, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_TOKEN_INACTIVE")
        );

        let mut foreign_account = observation()?;
        foreign_account.deployment_account_id = "b".repeat(32);
        assert_eq!(
            policy()?
                .evaluate(foreign_account, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH")
        );

        let mut read_failed = observation()?;
        read_failed.reads.d1_catalog_read = false;
        assert_eq!(
            policy()?
                .evaluate(read_failed, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_REQUIRED_READ_INCOMPLETE")
        );

        let mut probe = observation()?;
        probe.reads.mutation_probe = "EXECUTED".to_owned();
        assert_eq!(
            policy()?
                .evaluate(probe, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_MUTATION_PROBE_INVALID")
        );
        Ok(())
    }

    #[test]
    fn rejects_impossible_oversized_future_and_replayed_freshness()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut impossible = observation()?;
        impossible.valid_until_unix_seconds = impossible.observed_at_unix_seconds;
        assert_eq!(
            policy()?
                .evaluate(impossible, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID")
        );

        let mut oversized = observation()?;
        oversized.valid_until_unix_seconds = oversized.observed_at_unix_seconds + 3_601;
        assert_eq!(
            policy()?
                .evaluate(oversized, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_FRESHNESS_WINDOW_TOO_LARGE")
        );

        assert_eq!(
            policy()?
                .evaluate(observation()?, 1_699_999_999)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_OBSERVATION_FROM_FUTURE")
        );
        assert_eq!(
            policy()?
                .evaluate(observation()?, 1_700_003_600)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_EXPIRED_OR_REPLAYED")
        );
        Ok(())
    }

    #[test]
    fn identifiers_reject_ambiguous_whitespace() {
        assert!(EvidenceIssuer::new(" github-actions").is_err());
        assert!(EvidenceSource::new("").is_err());
    }

    #[test]
    fn review_attestation_accepts_exact_provider_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        ReviewAttestationPolicyV1.evaluate(&review_observation()?)?;
        Ok(())
    }

    #[test]
    fn review_attestation_rejects_provider_binding_drift() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut foreign_repository = review_observation()?;
        foreign_repository.observed_repository = EvidenceTarget::new("other/repository")?;
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&foreign_repository)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_REPOSITORY_MISMATCH")
        );
        Ok(())
    }

    #[test]
    fn review_attestation_rejects_unavailable_or_mutated_provider_object()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut unavailable = review_observation()?;
        unavailable.provider_object_available = false;
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&unavailable)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_PROVIDER_OBJECT_UNAVAILABLE")
        );

        let mut wrong_body = review_observation()?;
        wrong_body.observed_body = Some("external-evidence-review-v1\nwrong=true".to_owned());
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&wrong_body)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_BODY_MISMATCH")
        );
        Ok(())
    }
}
