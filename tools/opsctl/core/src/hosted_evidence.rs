use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

const MAX_IDENTIFIER_BYTES: usize = 256;
const REVIEW_CLAIM_DOMAIN: &str = "external-evidence-review-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidencePolicyDisposition {
    Rejected,
    Retryable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyError {
    code: &'static str,
    detail: String,
    disposition: EvidencePolicyDisposition,
}

impl EvidencePolicyError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            disposition: EvidencePolicyDisposition::Rejected,
        }
    }

    fn retryable(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            disposition: EvidencePolicyDisposition::Retryable,
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

    #[must_use]
    pub const fn disposition(&self) -> EvidencePolicyDisposition {
        self.disposition
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
    pub account_name: String,
    pub permission_names: Vec<String>,
    pub production_scope: bool,
    pub mutation_capability: bool,
    pub token_management_capability: bool,
    pub plaintext_token_included: bool,
    pub attestation_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialTokenVerifyObservationV1 {
    pub http_status: Option<u16>,
    pub success: Option<bool>,
    pub error_count: Option<usize>,
    pub token_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialAccountObservationV1 {
    pub http_status: Option<u16>,
    pub success: Option<bool>,
    pub error_count: Option<usize>,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCredentialReadObservationV4 {
    pub workers_deployments_http_status: Option<u16>,
    pub workers_deployments_success: Option<bool>,
    pub workers_deployments_error_count: Option<usize>,
    pub workers_deployments_response_digest_sha256: Option<String>,
    pub workers_current_release_set_id: String,
    pub d1_catalog_exit_code: Option<i32>,
    pub d1_catalog_output_digest_sha256: Option<String>,
    pub r2_bucket_exit_code: Option<i32>,
    pub r2_bucket_output_digest_sha256: Option<String>,
    pub queue_exit_code: Option<i32>,
    pub worker_secret_names_exit_code: Option<i32>,
    pub mutation_probe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceObservationV4 {
    pub binding: EvidenceBindingV1,
    pub source_run_id: u64,
    pub source_run_attempt: u32,
    pub observed_at_unix_seconds: i64,
    pub valid_until_unix_seconds: i64,
    pub credential_policy: OperationalCredentialPolicyObservationV1,
    pub attestation: OperationalCredentialAttestationObservationV1,
    pub token_verify: OperationalCredentialTokenVerifyObservationV1,
    pub account: OperationalCredentialAccountObservationV1,
    pub deployment_account_id: String,
    pub reads: OperationalCredentialReadObservationV4,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceEnvelopeV4 {
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
pub struct ExpectedAccountBindingV1 {
    account_id: String,
    account_name: String,
}

impl ExpectedAccountBindingV1 {
    pub fn new(
        account_id: impl Into<String>,
        account_name: impl Into<String>,
    ) -> Result<Self, EvidencePolicyError> {
        let account_id = validate_identifier("expected_account_id", account_id.into())?;
        if !is_lower_hex(&account_id, 32) {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_POLICY_INVALID",
                "expected account id must be exactly 32 lowercase hexadecimal characters",
            ));
        }
        Ok(Self {
            account_id,
            account_name: validate_identifier("expected_account_name", account_name.into())?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyV4 {
    expected_binding: EvidenceBindingV1,
    max_validity_seconds: i64,
    expected_credential_id: String,
    expected_attestation_kind: String,
    expected_attestation_source: String,
    expected_account: ExpectedAccountBindingV1,
    expected_mutation_probe: String,
}

impl EvidencePolicyV4 {
    pub fn new(
        expected_binding: EvidenceBindingV1,
        max_validity_seconds: u64,
        expected_credential_id: impl Into<String>,
        expected_attestation_kind: impl Into<String>,
        expected_attestation_source: impl Into<String>,
        expected_account: ExpectedAccountBindingV1,
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
            expected_account,
            expected_mutation_probe: validate_identifier(
                "expected_mutation_probe",
                expected_mutation_probe.into(),
            )?,
        })
    }

    pub fn evaluate(
        &self,
        observation: HostedEvidenceObservationV4,
        evaluated_at_unix_seconds: i64,
    ) -> Result<HostedEvidenceEnvelopeV4, EvidencePolicyError> {
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
        validate_account_observation(self, &observation)?;
        validate_read_observations(self, &observation)?;

        Ok(HostedEvidenceEnvelopeV4 {
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
    policy: &EvidencePolicyV4,
    observation: &HostedEvidenceObservationV4,
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
    policy: &EvidencePolicyV4,
    observation: &HostedEvidenceObservationV4,
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
        || attestation.account_id != policy.expected_account.account_id
        || attestation.account_id != observation.deployment_account_id
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH",
            "attested and deployment account ids must exactly match the expected staging account id",
        ));
    }
    if attestation.account_name != policy.expected_account.account_name {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_NAME_MISMATCH",
            format!(
                "accepted credential attestation account name must exactly match expected staging account {}",
                policy.expected_account.account_name
            ),
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
    observation: &HostedEvidenceObservationV4,
) -> Result<(), EvidencePolicyError> {
    let verify = &observation.token_verify;
    validate_provider_http_status("token verification", verify.http_status)?;
    if verify.success != Some(true) || verify.error_count != Some(0) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_VERIFY_FAILED",
            "token verification did not return one successful error-free provider response",
        ));
    }
    let token_id = verify.token_id.as_deref().ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            "token verification result.token id is required after HTTP 200",
        )
    })?;
    if token_id != observation.attestation.token_id {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_BINDING_MISMATCH",
            "verified token id does not match the issuance attestation",
        ));
    }
    let status = verify.status.as_deref().ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            "token verification result.status is required after HTTP 200",
        )
    })?;
    if status != "active" {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_TOKEN_INACTIVE",
            "verified token status must be active",
        ));
    }
    Ok(())
}

fn validate_account_observation(
    policy: &EvidencePolicyV4,
    observation: &HostedEvidenceObservationV4,
) -> Result<(), EvidencePolicyError> {
    let account = &observation.account;
    validate_provider_http_status("account observation", account.http_status)?;
    if account.success != Some(true) || account.error_count != Some(0) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_OBSERVATION_FAILED",
            "Cloudflare account observation must be provider-successful and error-free",
        ));
    }
    let account_id = account.account_id.as_deref().ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            "Cloudflare account result.id is required after HTTP 200",
        )
    })?;
    if !is_lower_hex(account_id, 32)
        || account_id != policy.expected_account.account_id
        || account_id != observation.deployment_account_id
        || account_id != observation.attestation.account_id
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH",
            "live Cloudflare account id must exactly match the expected, deployment and issuance-attestation account ids",
        ));
    }
    let account_name = account.account_name.as_deref().ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            "Cloudflare account result.name is required after HTTP 200",
        )
    })?;
    if account_name != policy.expected_account.account_name
        || account_name != observation.attestation.account_name
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_ACCOUNT_NAME_MISMATCH",
            format!(
                "live Cloudflare account name must exactly match expected and attested staging account {}",
                policy.expected_account.account_name
            ),
        ));
    }
    Ok(())
}

fn validate_read_observations(
    policy: &EvidencePolicyV4,
    observation: &HostedEvidenceObservationV4,
) -> Result<(), EvidencePolicyError> {
    let reads = &observation.reads;
    validate_provider_http_status(
        "Workers deployments observation",
        reads.workers_deployments_http_status,
    )?;
    if reads.workers_deployments_success != Some(true)
        || reads.workers_deployments_error_count != Some(0)
    {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_WORKERS_DEPLOYMENTS_READ_FAILED",
            "Workers deployments observation must be provider-successful and error-free",
        ));
    }
    validate_required_sha256(
        "workers_deployments_response_digest_sha256",
        reads.workers_deployments_response_digest_sha256.as_deref(),
    )?;
    validate_current_release_set_id(&reads.workers_current_release_set_id)?;
    validate_required_sha256(
        "d1_catalog_output_digest_sha256",
        reads.d1_catalog_output_digest_sha256.as_deref(),
    )?;
    validate_required_sha256(
        "r2_bucket_output_digest_sha256",
        reads.r2_bucket_output_digest_sha256.as_deref(),
    )?;
    validate_wrangler_exit("d1_catalog", reads.d1_catalog_exit_code)?;
    validate_wrangler_exit("r2_bucket", reads.r2_bucket_exit_code)?;
    validate_wrangler_exit("queue", reads.queue_exit_code)?;
    validate_wrangler_exit("worker_secret_names", reads.worker_secret_names_exit_code)?;

    if reads.mutation_probe.as_deref() != Some(policy.expected_mutation_probe.as_str()) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_MUTATION_PROBE_INVALID",
            "mutation probe must remain forbidden and unexecuted",
        ));
    }
    Ok(())
}

fn validate_current_release_set_id(value: &str) -> Result<(), EvidencePolicyError> {
    if value == "NONE" {
        return Ok(());
    }
    let digest = value
        .strip_prefix("release-set-v2-sha256-")
        .or_else(|| value.strip_prefix("release-set-v3-sha256-"));
    if digest.is_some_and(|digest| is_lower_hex(digest, 64)) {
        return Ok(());
    }
    Err(EvidencePolicyError::new(
        "HOSTED_EVIDENCE_RELEASE_SET_ID_INVALID",
        "workers_current_release_set_id must be NONE or one exact Release Set v2/v3 tag",
    ))
}

fn validate_provider_http_status(
    operation: &'static str,
    status: Option<u16>,
) -> Result<(), EvidencePolicyError> {
    let status = status.ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            format!("{operation} HTTP status is required"),
        )
    })?;
    match status {
        200 => Ok(()),
        429 | 500..=599 => Err(EvidencePolicyError::retryable(
            "HOSTED_EVIDENCE_PROVIDER_RETRYABLE",
            format!("{operation} returned retryable HTTP status {status}"),
        )),
        _ => Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_REJECTED",
            format!("{operation} returned rejected HTTP status {status}"),
        )),
    }
}

fn validate_wrangler_exit(
    operation: &'static str,
    exit_code: Option<i32>,
) -> Result<(), EvidencePolicyError> {
    match exit_code {
        Some(0) => Ok(()),
        Some(code) => Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_WRANGLER_READ_REJECTED",
            format!("{operation} read-only Wrangler process exited with code {code}"),
        )),
        None => Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            format!("{operation} Wrangler exit code is required"),
        )),
    }
}

fn validate_required_sha256(
    field: &'static str,
    digest: Option<&str>,
) -> Result<(), EvidencePolicyError> {
    let digest = digest.ok_or_else(|| {
        EvidencePolicyError::new(
            "HOSTED_EVIDENCE_PROVIDER_RESULT_MISSING",
            format!("{field} is required"),
        )
    })?;
    if !is_lower_hex(digest, 64) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_READ_DIGEST_INVALID",
            format!("{field} must be exactly 64 lowercase hexadecimal characters"),
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
        EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome,
        EvidencePolicyDisposition, EvidencePolicyError, EvidencePolicyV4, EvidenceSource,
        EvidenceSubject, EvidenceTarget, EvidenceTrustState, ExpectedAccountBindingV1,
        HostedEvidenceEnvelopeV4, HostedEvidenceObservationV4,
        OperationalCredentialAccountObservationV1, OperationalCredentialAttestationObservationV1,
        OperationalCredentialPolicyObservationV1, OperationalCredentialReadObservationV4,
        OperationalCredentialTokenVerifyObservationV1, ReviewAttestationObservationV1,
        ReviewAttestationPolicyV1, ReviewAttestationStatus,
    };

    type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

    fn policy_error(
        result: Result<HostedEvidenceEnvelopeV4, EvidencePolicyError>,
    ) -> TestResult<EvidencePolicyError> {
        match result {
            Err(error) => Ok(error),
            Ok(_) => Err(std::io::Error::other("expected hosted evidence policy failure").into()),
        }
    }

    fn binding(subject: &str) -> TestResult<EvidenceBindingV1> {
        Ok(EvidenceBindingV1 {
            issuer: EvidenceIssuer::new("github-actions")?,
            source: EvidenceSource::new("github-governance-gate/operational-credential-state")?,
            target: EvidenceTarget::new("iamaman11/part-crm-emai-profile")?,
            environment: EvidenceEnvironment::new("staging")?,
            subject: EvidenceSubject::new(subject)?,
        })
    }

    fn release_set_v3() -> String {
        format!("release-set-v3-sha256-{}", "4".repeat(64))
    }

    fn observation() -> TestResult<HostedEvidenceObservationV4> {
        let required = vec![
            "D1 Read".to_owned(),
            "Queues Read".to_owned(),
            "Workers R2 Storage Read".to_owned(),
            "Workers Scripts Read".to_owned(),
        ];
        Ok(HostedEvidenceObservationV4 {
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
                account_name: "pvisakp".to_owned(),
                permission_names: required,
                production_scope: false,
                mutation_capability: false,
                token_management_capability: false,
                plaintext_token_included: false,
                attestation_source: "CLOUDFLARE_TOKEN_ISSUANCE_POLICY".to_owned(),
            },
            token_verify: OperationalCredentialTokenVerifyObservationV1 {
                http_status: Some(200),
                success: Some(true),
                error_count: Some(0),
                token_id: Some("observe-token-id-1234".to_owned()),
                status: Some("active".to_owned()),
            },
            account: OperationalCredentialAccountObservationV1 {
                http_status: Some(200),
                success: Some(true),
                error_count: Some(0),
                account_id: Some("a".repeat(32)),
                account_name: Some("pvisakp".to_owned()),
            },
            deployment_account_id: "a".repeat(32),
            reads: OperationalCredentialReadObservationV4 {
                workers_deployments_http_status: Some(200),
                workers_deployments_success: Some(true),
                workers_deployments_error_count: Some(0),
                workers_deployments_response_digest_sha256: Some("1".repeat(64)),
                workers_current_release_set_id: release_set_v3(),
                d1_catalog_exit_code: Some(0),
                d1_catalog_output_digest_sha256: Some("2".repeat(64)),
                r2_bucket_exit_code: Some(0),
                r2_bucket_output_digest_sha256: Some("3".repeat(64)),
                queue_exit_code: Some(0),
                worker_secret_names_exit_code: Some(0),
                mutation_probe: Some("FORBIDDEN_NOT_EXECUTED".to_owned()),
            },
            production_mutation: false,
        })
    }

    fn policy() -> TestResult<EvidencePolicyV4> {
        Ok(EvidencePolicyV4::new(
            binding("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?,
            3_600,
            "cloudflare.staging-observation-api",
            "AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION",
            "CLOUDFLARE_TOKEN_ISSUANCE_POLICY",
            ExpectedAccountBindingV1::new("a".repeat(32), "pvisakp")?,
            "FORBIDDEN_NOT_EXECUTED",
        )?)
    }

    #[test]
    fn derives_trusted_passed_only_from_raw_provider_facts() -> TestResult<()> {
        let envelope = policy()?.evaluate(observation()?, 1_700_000_010)?;
        assert_eq!(envelope.trust_state, EvidenceTrustState::Trusted);
        assert_eq!(envelope.outcome, EvidenceOutcome::Passed);
        assert!(!envelope.production_mutation);
        Ok(())
    }

    #[test]
    fn accepts_explicit_clean_environment_release_identity() -> TestResult<()> {
        let mut clean = observation()?;
        clean.reads.workers_current_release_set_id = "NONE".to_owned();
        policy()?.evaluate(clean, 1_700_000_010)?;
        Ok(())
    }

    #[test]
    fn rejects_unsupported_or_malformed_release_identity() -> TestResult<()> {
        for current_id in [
            "release-set-v1-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "release-set-v4-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "release-set-v3-sha256-deadbeef",
        ] {
            let mut invalid = observation()?;
            invalid.reads.workers_current_release_set_id = current_id.to_owned();
            let error = policy_error(policy()?.evaluate(invalid, 1_700_000_010))?;
            assert_eq!(error.code(), "HOSTED_EVIDENCE_RELEASE_SET_ID_INVALID");
        }
        Ok(())
    }

    #[test]
    fn classifies_http_failures_in_pure_core() -> TestResult<()> {
        for status in [401, 403, 404] {
            let mut rejected = observation()?;
            rejected.reads.workers_deployments_http_status = Some(status);
            let error = policy_error(policy()?.evaluate(rejected, 1_700_000_010))?;
            assert_eq!(error.code(), "HOSTED_EVIDENCE_PROVIDER_REJECTED");
            assert_eq!(error.disposition(), EvidencePolicyDisposition::Rejected);
        }
        for status in [429, 500, 503, 599] {
            let mut retryable = observation()?;
            retryable.reads.workers_deployments_http_status = Some(status);
            let error = policy_error(policy()?.evaluate(retryable, 1_700_000_010))?;
            assert_eq!(error.code(), "HOSTED_EVIDENCE_PROVIDER_RETRYABLE");
            assert_eq!(error.disposition(), EvidencePolicyDisposition::Retryable);
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_provider_result_invalid_digest_and_nonzero_wrangler_exit() -> TestResult<()>
    {
        let mut missing = observation()?;
        missing.reads.workers_deployments_success = None;
        let error = policy_error(policy()?.evaluate(missing, 1_700_000_010))?;
        assert_eq!(
            error.code(),
            "HOSTED_EVIDENCE_WORKERS_DEPLOYMENTS_READ_FAILED"
        );

        let mut invalid_digest = observation()?;
        invalid_digest.reads.d1_catalog_output_digest_sha256 = Some("ABC".to_owned());
        let error = policy_error(policy()?.evaluate(invalid_digest, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_READ_DIGEST_INVALID");

        let mut wrangler_failed = observation()?;
        wrangler_failed.reads.d1_catalog_exit_code = Some(1);
        let error = policy_error(policy()?.evaluate(wrangler_failed, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_WRANGLER_READ_REJECTED");
        Ok(())
    }

    #[test]
    fn rejects_account_binding_drift_including_majakojh() -> TestResult<()> {
        let mut wrong_live_name = observation()?;
        wrong_live_name.account.account_name = Some("majakojh".to_owned());
        let error = policy_error(policy()?.evaluate(wrong_live_name, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_ACCOUNT_NAME_MISMATCH");

        let mut wrong_attested_name = observation()?;
        wrong_attested_name.attestation.account_name = "majakojh".to_owned();
        wrong_attested_name.account.account_name = Some("majakojh".to_owned());
        let error = policy_error(policy()?.evaluate(wrong_attested_name, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_ACCOUNT_NAME_MISMATCH");

        let mut wrong_id = observation()?;
        wrong_id.account.account_id = Some("b".repeat(32));
        let error = policy_error(policy()?.evaluate(wrong_id, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH");

        let mut coordinated_wrong_id = observation()?;
        coordinated_wrong_id.attestation.account_id = "b".repeat(32);
        coordinated_wrong_id.deployment_account_id = "b".repeat(32);
        coordinated_wrong_id.account.account_id = Some("b".repeat(32));
        let error = policy_error(policy()?.evaluate(coordinated_wrong_id, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_ACCOUNT_SCOPE_MISMATCH");
        Ok(())
    }

    #[test]
    fn rejects_binding_run_freshness_permission_token_and_mutation_drift() -> TestResult<()> {
        let mut foreign_repository = observation()?;
        foreign_repository.binding.target = EvidenceTarget::new("other/repository")?;
        let error = policy_error(policy()?.evaluate(foreign_repository, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_BINDING_MISMATCH");

        let mut foreign_subject = observation()?;
        foreign_subject.binding.subject = EvidenceSubject::new("b".repeat(40))?;
        let error = policy_error(policy()?.evaluate(foreign_subject, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_BINDING_MISMATCH");

        let mut foreign_environment = observation()?;
        foreign_environment.binding.environment = EvidenceEnvironment::new("production")?;
        let error = policy_error(policy()?.evaluate(foreign_environment, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_BINDING_MISMATCH");

        let mut invalid_attempt = observation()?;
        invalid_attempt.source_run_attempt = 0;
        let error = policy_error(policy()?.evaluate(invalid_attempt, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_RUN_IDENTITY_INVALID");

        let mut permission_missing = observation()?;
        permission_missing.attestation.permission_names.pop();
        let error = policy_error(policy()?.evaluate(permission_missing, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_PERMISSION_MISMATCH");

        let mut forbidden_permission = observation()?;
        forbidden_permission
            .attestation
            .permission_names
            .push("Workers Scripts Write".to_owned());
        assert!(
            policy()?
                .evaluate(forbidden_permission, 1_700_000_010)
                .is_err()
        );

        let mut inactive = observation()?;
        inactive.token_verify.status = Some("disabled".to_owned());
        let error = policy_error(policy()?.evaluate(inactive, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_TOKEN_INACTIVE");

        let mut token_mismatch = observation()?;
        token_mismatch.token_verify.token_id = Some("different-token-id-1234".to_owned());
        let error = policy_error(policy()?.evaluate(token_mismatch, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_TOKEN_BINDING_MISMATCH");

        let mut mutation_probe = observation()?;
        mutation_probe.reads.mutation_probe = Some("EXECUTED".to_owned());
        let error = policy_error(policy()?.evaluate(mutation_probe, 1_700_000_010))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_MUTATION_PROBE_INVALID");

        let mut production_mutation = observation()?;
        production_mutation.production_mutation = true;
        let error = policy_error(policy()?.evaluate(production_mutation, 1_700_000_010))?;
        assert_eq!(
            error.code(),
            "HOSTED_EVIDENCE_PRODUCTION_MUTATION_FORBIDDEN"
        );

        let mut credential_mutation = observation()?;
        credential_mutation.credential_policy.mutation_allowed = true;
        let error = policy_error(policy()?.evaluate(credential_mutation, 1_700_000_010))?;
        assert_eq!(
            error.code(),
            "HOSTED_EVIDENCE_CREDENTIAL_MUTATION_AUTHORITY_FORBIDDEN"
        );

        let mut token_management = observation()?;
        token_management.attestation.token_management_capability = true;
        let error = policy_error(policy()?.evaluate(token_management, 1_700_000_010))?;
        assert_eq!(
            error.code(),
            "HOSTED_EVIDENCE_ATTESTATION_AUTHORITY_INVALID"
        );

        let mut plaintext = observation()?;
        plaintext.attestation.plaintext_token_included = true;
        let error = policy_error(policy()?.evaluate(plaintext, 1_700_000_010))?;
        assert_eq!(
            error.code(),
            "HOSTED_EVIDENCE_ATTESTATION_AUTHORITY_INVALID"
        );

        let error = policy_error(policy()?.evaluate(observation()?, 1_700_003_600))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_EXPIRED_OR_REPLAYED");

        let error = policy_error(policy()?.evaluate(observation()?, 1_699_999_999))?;
        assert_eq!(error.code(), "HOSTED_EVIDENCE_OBSERVATION_FROM_FUTURE");
        Ok(())
    }

    fn review_observation() -> TestResult<ReviewAttestationObservationV1> {
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
    fn review_attestation_preserves_exact_provider_observation_policy() -> TestResult<()> {
        ReviewAttestationPolicyV1.evaluate(&review_observation()?)?;
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
