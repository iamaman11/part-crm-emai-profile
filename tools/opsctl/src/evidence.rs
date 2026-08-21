//! Typed, versioned Hosted Operational Evidence policy.
//!
//! The namespace is intentionally offline. GitHub Actions and provider-native tools
//! own orchestration and observation. This module only transforms saved raw
//! observations into a canonical, secret-free envelope and validates local bytes.

use crate::release::digest::{canonical_json, sha256_hex};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

pub const HOSTED_EVIDENCE_SCHEMA_VERSION: u64 = 1;
pub const HOSTED_EVIDENCE_CONTEXT_VERSION: u64 = 1;
pub const EXPECTED_REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const MAX_JSON_BYTES: u64 = 1024 * 1024;

const FORBIDDEN_SECRET_KEYS: &[&str] = &[
    "access_token",
    "api_token",
    "authorization",
    "bearer_token",
    "client_secret",
    "credential_value",
    "key_material",
    "password",
    "plaintext",
    "plaintext_value",
    "private_key",
    "raw_handle",
    "raw_secret",
    "raw_token",
    "secret_value",
    "token_value",
    "value",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceAction {
    Build,
    Validate,
    Inspect,
    Verify,
}

impl EvidenceAction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Validate => "validate",
            Self::Inspect => "inspect",
            Self::Verify => "verify",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EvidenceRunRequest<'a> {
    pub action: EvidenceAction,
    pub evidence_json: Option<&'a Path>,
    pub raw_observation: Option<&'a Path>,
    pub context_json: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceError {
    message: String,
}

impl EvidenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for EvidenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EvidenceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Environment {
    Rehearsal,
    Staging,
    Production,
}

impl Environment {
    fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "rehearsal" => Ok(Self::Rehearsal),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            other => Err(EvidenceError::new(format!(
                "unsupported hosted evidence environment: {other}"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Rehearsal => "rehearsal",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidenceKind {
    CredentialReadiness,
    HostedResourceState,
    ReleaseSetTransition,
}

impl EvidenceKind {
    fn parse(kind: &str, payload_version: u64) -> Result<Self, EvidenceError> {
        if payload_version != 1 {
            return Err(EvidenceError::new(format!(
                "unsupported payload version {payload_version} for evidence kind {kind}"
            )));
        }
        match kind {
            "credential_readiness" => Ok(Self::CredentialReadiness),
            "hosted_resource_state" => Ok(Self::HostedResourceState),
            "release_set_transition" => Ok(Self::ReleaseSetTransition),
            other => Err(EvidenceError::new(format!(
                "unsupported evidence kind: {other}"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::CredentialReadiness => "credential_readiness",
            Self::HostedResourceState => "hosted_resource_state",
            Self::ReleaseSetTransition => "release_set_transition",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkflowIdentity {
    name: String,
    workflow_ref: String,
    run_id: u64,
    run_attempt: u64,
    observation_job: String,
}

impl WorkflowIdentity {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        let object = object(value, "workflow")?;
        reject_unknown_fields(
            object,
            &["name", "workflow_ref", "run_id", "run_attempt", "observation_job"],
            "workflow",
        )?;
        let name = required_string(object, "name", "workflow")?;
        let workflow_ref = required_string(object, "workflow_ref", "workflow")?;
        let run_id = required_u64(object, "run_id", "workflow")?;
        let run_attempt = required_u64(object, "run_attempt", "workflow")?;
        let observation_job = required_string(object, "observation_job", "workflow")?;
        validate_text(&name, "workflow.name", 256)?;
        validate_text(&workflow_ref, "workflow.workflow_ref", 512)?;
        validate_text(&observation_job, "workflow.observation_job", 256)?;
        if run_id == 0 || run_attempt == 0 {
            return Err(EvidenceError::new(
                "workflow run_id and run_attempt must be positive integers",
            ));
        }
        Ok(Self {
            name,
            workflow_ref,
            run_id,
            run_attempt,
            observation_job,
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "name": self.name,
            "workflow_ref": self.workflow_ref,
            "run_id": self.run_id,
            "run_attempt": self.run_attempt,
            "observation_job": self.observation_job,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostedEvidenceContextV1 {
    evidence_kind: EvidenceKind,
    payload_version: u64,
    repository: String,
    source_sha: String,
    source_ref: String,
    workflow: WorkflowIdentity,
    environment: Environment,
    observed_at: String,
    provider_mutation: bool,
    production_mutation: bool,
}

impl HostedEvidenceContextV1 {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        reject_secret_material(value, "context")?;
        let root = object(value, "hosted evidence context")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "evidence_kind",
                "payload_version",
                "repository",
                "source_sha",
                "source_ref",
                "workflow",
                "environment",
                "observed_at",
                "provider_mutation",
                "production_mutation",
            ],
            "hosted evidence context",
        )?;
        parse_context_fields(root)
    }

    fn to_value(&self) -> Value {
        json!({
            "schema_version": HOSTED_EVIDENCE_CONTEXT_VERSION,
            "evidence_kind": self.evidence_kind.as_str(),
            "payload_version": self.payload_version,
            "repository": self.repository,
            "source_sha": self.source_sha,
            "source_ref": self.source_ref,
            "workflow": self.workflow.to_value(),
            "environment": self.environment.as_str(),
            "observed_at": self.observed_at,
            "provider_mutation": self.provider_mutation,
            "production_mutation": self.production_mutation,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CredentialReadinessStatus {
    Ready,
    NotReady,
}

impl CredentialReadinessStatus {
    fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "READY" => Ok(Self::Ready),
            "NOT_READY" => Ok(Self::NotReady),
            other => Err(EvidenceError::new(format!(
                "unsupported credential readiness status: {other}"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::NotReady => "NOT_READY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CredentialReadinessV1 {
    provider: String,
    credential_identity: String,
    provider_metadata_identifier: String,
    status: CredentialReadinessStatus,
    capabilities: Vec<String>,
}

impl CredentialReadinessV1 {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        let object = object(value, "credential_readiness payload")?;
        reject_unknown_fields(
            object,
            &[
                "provider",
                "credential_identity",
                "provider_metadata_identifier",
                "status",
                "capabilities",
            ],
            "credential_readiness payload",
        )?;
        let provider = required_string(object, "provider", "credential_readiness payload")?;
        let credential_identity = required_string(
            object,
            "credential_identity",
            "credential_readiness payload",
        )?;
        let provider_metadata_identifier = required_string(
            object,
            "provider_metadata_identifier",
            "credential_readiness payload",
        )?;
        let status = CredentialReadinessStatus::parse(&required_string(
            object,
            "status",
            "credential_readiness payload",
        )?)?;
        let capabilities = required_string_array(
            object,
            "capabilities",
            "credential_readiness payload",
        )?;
        validate_text(&provider, "payload.provider", 128)?;
        validate_text(
            &credential_identity,
            "payload.credential_identity",
            256,
        )?;
        validate_text(
            &provider_metadata_identifier,
            "payload.provider_metadata_identifier",
            256,
        )?;
        if matches!(status, CredentialReadinessStatus::Ready) && capabilities.is_empty() {
            return Err(EvidenceError::new(
                "READY credential evidence must contain at least one capability",
            ));
        }
        Ok(Self {
            provider,
            credential_identity,
            provider_metadata_identifier,
            status,
            capabilities,
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "provider": self.provider,
            "credential_identity": self.credential_identity,
            "provider_metadata_identifier": self.provider_metadata_identifier,
            "status": self.status.as_str(),
            "capabilities": self.capabilities,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostedResourceStateV1 {
    provider: String,
    resource_type: String,
    resource_id: String,
    state: String,
    revision: Option<String>,
    enabled: Option<bool>,
}

impl HostedResourceStateV1 {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        let object = object(value, "hosted_resource_state payload")?;
        reject_unknown_fields(
            object,
            &[
                "provider",
                "resource_type",
                "resource_id",
                "state",
                "revision",
                "enabled",
            ],
            "hosted_resource_state payload",
        )?;
        let provider = required_string(object, "provider", "hosted_resource_state payload")?;
        let resource_type =
            required_string(object, "resource_type", "hosted_resource_state payload")?;
        let resource_id =
            required_string(object, "resource_id", "hosted_resource_state payload")?;
        let state = required_string(object, "state", "hosted_resource_state payload")?;
        let revision = required_nullable_string(
            object,
            "revision",
            "hosted_resource_state payload",
        )?;
        let enabled = required_nullable_bool(
            object,
            "enabled",
            "hosted_resource_state payload",
        )?;
        validate_text(&provider, "payload.provider", 128)?;
        validate_text(&resource_type, "payload.resource_type", 128)?;
        validate_text(&resource_id, "payload.resource_id", 512)?;
        validate_text(&state, "payload.state", 128)?;
        if let Some(value) = &revision {
            validate_text(value, "payload.revision", 512)?;
        }
        Ok(Self {
            provider,
            resource_type,
            resource_id,
            state,
            revision,
            enabled,
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "provider": self.provider,
            "resource_type": self.resource_type,
            "resource_id": self.resource_id,
            "state": self.state,
            "revision": self.revision,
            "enabled": self.enabled,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionDecision {
    Applied,
    NoChange,
    RolledBack,
    Blocked,
}

impl TransitionDecision {
    fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "APPLIED" => Ok(Self::Applied),
            "NO_CHANGE" => Ok(Self::NoChange),
            "ROLLED_BACK" => Ok(Self::RolledBack),
            "BLOCKED" => Ok(Self::Blocked),
            other => Err(EvidenceError::new(format!(
                "unsupported Release Set transition decision: {other}"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "APPLIED",
            Self::NoChange => "NO_CHANGE",
            Self::RolledBack => "ROLLED_BACK",
            Self::Blocked => "BLOCKED",
        }
    }

    const fn requires_provider_mutation(self) -> bool {
        matches!(self, Self::Applied | Self::RolledBack)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatibilityDecision {
    Compatible,
    Incompatible,
    Unknown,
}

impl CompatibilityDecision {
    fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "COMPATIBLE" => Ok(Self::Compatible),
            "INCOMPATIBLE" => Ok(Self::Incompatible),
            "UNKNOWN" => Ok(Self::Unknown),
            other => Err(EvidenceError::new(format!(
                "unsupported transition compatibility decision: {other}"
            ))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "COMPATIBLE",
            Self::Incompatible => "INCOMPATIBLE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseSetTransitionV1 {
    provider: String,
    profile_id: String,
    previous_release_set_id: Option<String>,
    target_release_set_id: String,
    decision: TransitionDecision,
    compatibility: CompatibilityDecision,
}

impl ReleaseSetTransitionV1 {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        let object = object(value, "release_set_transition payload")?;
        reject_unknown_fields(
            object,
            &[
                "provider",
                "profile_id",
                "previous_release_set_id",
                "target_release_set_id",
                "decision",
                "compatibility",
            ],
            "release_set_transition payload",
        )?;
        let provider = required_string(object, "provider", "release_set_transition payload")?;
        let profile_id = required_string(object, "profile_id", "release_set_transition payload")?;
        let previous_release_set_id = required_nullable_string(
            object,
            "previous_release_set_id",
            "release_set_transition payload",
        )?;
        let target_release_set_id = required_string(
            object,
            "target_release_set_id",
            "release_set_transition payload",
        )?;
        let decision = TransitionDecision::parse(&required_string(
            object,
            "decision",
            "release_set_transition payload",
        )?)?;
        let compatibility = CompatibilityDecision::parse(&required_string(
            object,
            "compatibility",
            "release_set_transition payload",
        )?)?;
        validate_text(&provider, "payload.provider", 128)?;
        validate_text(&profile_id, "payload.profile_id", 256)?;
        validate_text(
            &target_release_set_id,
            "payload.target_release_set_id",
            256,
        )?;
        if let Some(value) = &previous_release_set_id {
            validate_text(value, "payload.previous_release_set_id", 256)?;
        }
        if matches!(
            decision,
            TransitionDecision::Applied | TransitionDecision::RolledBack
        ) && !matches!(compatibility, CompatibilityDecision::Compatible)
        {
            return Err(EvidenceError::new(
                "APPLIED/ROLLED_BACK Release Set evidence requires COMPATIBLE policy decision",
            ));
        }
        Ok(Self {
            provider,
            profile_id,
            previous_release_set_id,
            target_release_set_id,
            decision,
            compatibility,
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "provider": self.provider,
            "profile_id": self.profile_id,
            "previous_release_set_id": self.previous_release_set_id,
            "target_release_set_id": self.target_release_set_id,
            "decision": self.decision.as_str(),
            "compatibility": self.compatibility.as_str(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypedPayload {
    CredentialReadiness(CredentialReadinessV1),
    HostedResourceState(HostedResourceStateV1),
    ReleaseSetTransition(ReleaseSetTransitionV1),
}

impl TypedPayload {
    fn parse(
        evidence_kind: EvidenceKind,
        payload_version: u64,
        value: &Value,
    ) -> Result<Self, EvidenceError> {
        if payload_version != 1 {
            return Err(EvidenceError::new(format!(
                "unsupported payload version {payload_version}"
            )));
        }
        match evidence_kind {
            EvidenceKind::CredentialReadiness => Ok(Self::CredentialReadiness(
                CredentialReadinessV1::parse(value)?,
            )),
            EvidenceKind::HostedResourceState => Ok(Self::HostedResourceState(
                HostedResourceStateV1::parse(value)?,
            )),
            EvidenceKind::ReleaseSetTransition => Ok(Self::ReleaseSetTransition(
                ReleaseSetTransitionV1::parse(value)?,
            )),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Self::CredentialReadiness(value) => value.to_value(),
            Self::HostedResourceState(value) => value.to_value(),
            Self::ReleaseSetTransition(value) => value.to_value(),
        }
    }

    const fn requires_provider_mutation(&self) -> bool {
        match self {
            Self::CredentialReadiness(_) | Self::HostedResourceState(_) => false,
            Self::ReleaseSetTransition(value) => value.decision.requires_provider_mutation(),
        }
    }

    fn observation_only(&self) -> bool {
        matches!(
            self,
            Self::CredentialReadiness(_) | Self::HostedResourceState(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostedEvidenceEnvelopeV1 {
    context: HostedEvidenceContextV1,
    payload: TypedPayload,
}

impl HostedEvidenceEnvelopeV1 {
    fn from_raw(
        context: HostedEvidenceContextV1,
        raw_observation: &Value,
    ) -> Result<Self, EvidenceError> {
        reject_secret_material(raw_observation, "raw_observation")?;
        let payload = TypedPayload::parse(
            context.evidence_kind,
            context.payload_version,
            raw_observation,
        )?;
        validate_effect_policy(&context, &payload)?;
        Ok(Self { context, payload })
    }

    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        reject_secret_material(value, "hosted_evidence")?;
        let root = object(value, "HostedEvidenceEnvelopeV1")?;
        reject_unknown_fields(
            root,
            &[
                "schema_version",
                "evidence_kind",
                "payload_version",
                "repository",
                "source_sha",
                "source_ref",
                "workflow",
                "environment",
                "observed_at",
                "provider_mutation",
                "production_mutation",
                "payload",
            ],
            "HostedEvidenceEnvelopeV1",
        )?;
        let context = parse_context_fields(root)?;
        let payload_value = required(root, "payload", "HostedEvidenceEnvelopeV1")?;
        let payload = TypedPayload::parse(
            context.evidence_kind,
            context.payload_version,
            payload_value,
        )?;
        validate_effect_policy(&context, &payload)?;
        Ok(Self { context, payload })
    }

    fn to_value(&self) -> Value {
        let mut value = self.context.to_value();
        if let Some(root) = value.as_object_mut() {
            root.insert("payload".to_owned(), self.payload.to_value());
        }
        value
    }

    fn canonical_text(&self) -> Result<String, EvidenceError> {
        let canonical = canonical_json(&self.to_value()).map_err(EvidenceError::new)?;
        Ok(format!("{canonical}\n"))
    }
}

pub fn run(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    match request.action {
        EvidenceAction::Build => build(request),
        EvidenceAction::Validate => validate(request),
        EvidenceAction::Inspect => inspect(request),
        EvidenceAction::Verify => verify(request),
    }
}

fn build(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let raw_path = request
        .raw_observation
        .ok_or_else(|| EvidenceError::new("evidence build requires --raw-observation"))?;
    let context_path = request
        .context_json
        .ok_or_else(|| EvidenceError::new("evidence build requires --context-json"))?;
    if request.evidence_json.is_some() {
        return Err(EvidenceError::new(
            "evidence build does not accept --evidence-json",
        ));
    }
    let (_, context_value) = read_json_bounded(context_path, "hosted evidence context")?;
    let (_, raw_value) = read_json_bounded(raw_path, "raw hosted observation")?;
    let context = HostedEvidenceContextV1::parse(&context_value)?;
    HostedEvidenceEnvelopeV1::from_raw(context, &raw_value)?.canonical_text()
}

fn validate(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let evidence_path = evidence_only_path(&request, "validate")?;
    let (_, value) = read_json_bounded(evidence_path, "HostedEvidenceEnvelopeV1")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&value)?;
    canonical_result("VALID", &envelope, None)
}

fn inspect(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let evidence_path = evidence_only_path(&request, "inspect")?;
    let (text, value) = read_json_bounded(evidence_path, "HostedEvidenceEnvelopeV1")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&value)?;
    let canonical = envelope.canonical_text()?;
    let result = json!({
        "decision": "INSPECTED",
        "sha256": sha256_hex(text.as_bytes()),
        "canonical": text == canonical,
        "envelope": envelope.to_value(),
    });
    canonical_line(&result)
}

fn verify(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let evidence_path = request
        .evidence_json
        .ok_or_else(|| EvidenceError::new("evidence verify requires --evidence-json"))?;
    let context_path = request
        .context_json
        .ok_or_else(|| EvidenceError::new("evidence verify requires --context-json"))?;
    if request.raw_observation.is_some() {
        return Err(EvidenceError::new(
            "evidence verify does not accept --raw-observation",
        ));
    }
    let (text, value) = read_json_bounded(evidence_path, "HostedEvidenceEnvelopeV1")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&value)?;
    let canonical = envelope.canonical_text()?;
    if text != canonical {
        return Err(EvidenceError::new(
            "hosted evidence bytes are not the deterministic canonical serialization",
        ));
    }
    let (_, expected_value) = read_json_bounded(context_path, "expected hosted evidence context")?;
    let expected = HostedEvidenceContextV1::parse(&expected_value)?;
    if envelope.context != expected {
        return Err(EvidenceError::new(
            "hosted evidence context does not match the independent expected workflow/run/source binding",
        ));
    }
    canonical_result("VERIFIED", &envelope, Some(sha256_hex(text.as_bytes())))
}

fn evidence_only_path<'a>(
    request: &'a EvidenceRunRequest<'a>,
    action: &str,
) -> Result<&'a Path, EvidenceError> {
    let path = request.evidence_json.ok_or_else(|| {
        EvidenceError::new(format!("evidence {action} requires --evidence-json"))
    })?;
    if request.raw_observation.is_some() || request.context_json.is_some() {
        return Err(EvidenceError::new(format!(
            "evidence {action} accepts only --evidence-json"
        )));
    }
    Ok(path)
}

fn canonical_result(
    decision: &str,
    envelope: &HostedEvidenceEnvelopeV1,
    sha256: Option<String>,
) -> Result<String, EvidenceError> {
    let result = json!({
        "decision": decision,
        "schema_version": HOSTED_EVIDENCE_SCHEMA_VERSION,
        "evidence_kind": envelope.context.evidence_kind.as_str(),
        "payload_version": envelope.context.payload_version,
        "repository": envelope.context.repository,
        "source_sha": envelope.context.source_sha,
        "source_ref": envelope.context.source_ref,
        "workflow_run_id": envelope.context.workflow.run_id,
        "workflow_run_attempt": envelope.context.workflow.run_attempt,
        "environment": envelope.context.environment.as_str(),
        "provider_mutation": envelope.context.provider_mutation,
        "production_mutation": envelope.context.production_mutation,
        "sha256": sha256,
    });
    canonical_line(&result)
}

fn canonical_line(value: &Value) -> Result<String, EvidenceError> {
    let canonical = canonical_json(value).map_err(EvidenceError::new)?;
    Ok(format!("{canonical}\n"))
}

fn parse_context_fields(
    root: &Map<String, Value>,
) -> Result<HostedEvidenceContextV1, EvidenceError> {
    let schema_version = required_u64(root, "schema_version", "hosted evidence context")?;
    if schema_version != HOSTED_EVIDENCE_SCHEMA_VERSION {
        return Err(EvidenceError::new(format!(
            "unsupported HostedEvidenceEnvelope schema_version: {schema_version}"
        )));
    }
    let payload_version = required_u64(root, "payload_version", "hosted evidence context")?;
    let evidence_kind_text =
        required_string(root, "evidence_kind", "hosted evidence context")?;
    let evidence_kind = EvidenceKind::parse(&evidence_kind_text, payload_version)?;
    let repository = required_string(root, "repository", "hosted evidence context")?;
    if repository != EXPECTED_REPOSITORY {
        return Err(EvidenceError::new(format!(
            "hosted evidence repository must be {EXPECTED_REPOSITORY}"
        )));
    }
    let source_sha = required_string(root, "source_sha", "hosted evidence context")?;
    validate_sha(&source_sha, "source_sha")?;
    let source_ref = required_string(root, "source_ref", "hosted evidence context")?;
    if !source_ref.starts_with("refs/") {
        return Err(EvidenceError::new(
            "hosted evidence source_ref must start with refs/",
        ));
    }
    validate_text(&source_ref, "source_ref", 512)?;
    let workflow = WorkflowIdentity::parse(required(
        root,
        "workflow",
        "hosted evidence context",
    )?)?;
    let environment_text = required_string(root, "environment", "hosted evidence context")?;
    let environment = Environment::parse(&environment_text)?;
    let observed_at = required_string(root, "observed_at", "hosted evidence context")?;
    validate_utc_timestamp(&observed_at)?;
    let provider_mutation =
        required_bool(root, "provider_mutation", "hosted evidence context")?;
    let production_mutation =
        required_bool(root, "production_mutation", "hosted evidence context")?;
    validate_context_effects(environment, provider_mutation, production_mutation)?;
    Ok(HostedEvidenceContextV1 {
        evidence_kind,
        payload_version,
        repository,
        source_sha,
        source_ref,
        workflow,
        environment,
        observed_at,
        provider_mutation,
        production_mutation,
    })
}

fn validate_context_effects(
    environment: Environment,
    provider_mutation: bool,
    production_mutation: bool,
) -> Result<(), EvidenceError> {
    if production_mutation && !provider_mutation {
        return Err(EvidenceError::new(
            "production_mutation=true requires provider_mutation=true",
        ));
    }
    if production_mutation && !matches!(environment, Environment::Production) {
        return Err(EvidenceError::new(
            "production_mutation=true is valid only for environment=production",
        ));
    }
    if matches!(environment, Environment::Production)
        && provider_mutation != production_mutation
    {
        return Err(EvidenceError::new(
            "production provider mutation must be represented by production_mutation=true",
        ));
    }
    Ok(())
}

fn validate_effect_policy(
    context: &HostedEvidenceContextV1,
    payload: &TypedPayload,
) -> Result<(), EvidenceError> {
    if payload.observation_only() && context.provider_mutation {
        return Err(EvidenceError::new(
            "observational evidence kinds must have provider_mutation=false",
        ));
    }
    if payload.requires_provider_mutation() != context.provider_mutation {
        return Err(EvidenceError::new(
            "evidence payload decision and provider_mutation flag are inconsistent",
        ));
    }
    Ok(())
}

fn read_json_bounded(path: &Path, label: &str) -> Result<(String, Value), EvidenceError> {
    let metadata = fs::metadata(path).map_err(|error| {
        EvidenceError::new(format!("{label} metadata is unavailable: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(EvidenceError::new(format!("{label} must be a regular file")));
    }
    if metadata.len() > MAX_JSON_BYTES {
        return Err(EvidenceError::new(format!(
            "{label} exceeds the {MAX_JSON_BYTES}-byte policy bound"
        )));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| EvidenceError::new(format!("{label} is unreadable UTF-8: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| EvidenceError::new(format!("{label} is invalid JSON: {error}")))?;
    Ok((text, value))
}

fn reject_secret_material(value: &Value, path: &str) -> Result<(), EvidenceError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                if FORBIDDEN_SECRET_KEYS.contains(&normalized.as_str()) {
                    return Err(EvidenceError::new(format!(
                        "secret-bearing field is forbidden in hosted evidence at {path}.{key}"
                    )));
                }
                reject_secret_material(child, &format!("{path}.{key}"))?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_secret_material(child, &format!("{path}[{index}]"))?;
            }
        }
        Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            if lower.starts_with("bearer ")
                || text.contains("-----BEGIN PRIVATE KEY-----")
                || text.contains("-----BEGIN RSA PRIVATE KEY-----")
            {
                return Err(EvidenceError::new(format!(
                    "plaintext secret material is forbidden in hosted evidence at {path}"
                )));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, EvidenceError> {
    value
        .as_object()
        .ok_or_else(|| EvidenceError::new(format!("{label} must be one JSON object")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), EvidenceError> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(EvidenceError::new(format!(
                "unknown field in {label}: {key}"
            )));
        }
    }
    Ok(())
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a Value, EvidenceError> {
    object
        .get(key)
        .ok_or_else(|| EvidenceError::new(format!("{label} is missing required field {key}")))
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<String, EvidenceError> {
    required(object, key, label)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| EvidenceError::new(format!("{label}.{key} must be a string")))
}

fn required_u64(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u64, EvidenceError> {
    required(object, key, label)?
        .as_u64()
        .ok_or_else(|| EvidenceError::new(format!("{label}.{key} must be an unsigned integer")))
}

fn required_bool(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<bool, EvidenceError> {
    required(object, key, label)?
        .as_bool()
        .ok_or_else(|| EvidenceError::new(format!("{label}.{key} must be a boolean")))
}

fn required_nullable_string(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Option<String>, EvidenceError> {
    match required(object, key, label)? {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.to_owned())),
        _ => Err(EvidenceError::new(format!(
            "{label}.{key} must be a string or null"
        ))),
    }
}

fn required_nullable_bool(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Option<bool>, EvidenceError> {
    match required(object, key, label)? {
        Value::Null => Ok(None),
        Value::Bool(value) => Ok(Some(*value)),
        _ => Err(EvidenceError::new(format!(
            "{label}.{key} must be a boolean or null"
        ))),
    }
}

fn required_string_array(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Vec<String>, EvidenceError> {
    let values = required(object, key, label)?
        .as_array()
        .ok_or_else(|| EvidenceError::new(format!("{label}.{key} must be an array")))?;
    let mut result = Vec::with_capacity(values.len());
    let mut unique = BTreeSet::new();
    for value in values {
        let text = value.as_str().ok_or_else(|| {
            EvidenceError::new(format!("{label}.{key} entries must be strings"))
        })?;
        validate_text(text, &format!("{label}.{key}"), 256)?;
        if !unique.insert(text.to_owned()) {
            return Err(EvidenceError::new(format!(
                "{label}.{key} must not contain duplicate entries"
            )));
        }
        result.push(text.to_owned());
    }
    result.sort();
    Ok(result)
}

fn validate_text(value: &str, label: &str, max_len: usize) -> Result<(), EvidenceError> {
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(EvidenceError::new(format!(
            "{label} must be non-empty, control-free UTF-8 not exceeding {max_len} bytes"
        )));
    }
    Ok(())
}

fn validate_sha(value: &str, label: &str) -> Result<(), EvidenceError> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EvidenceError::new(format!(
            "{label} must be a 40-character lowercase hexadecimal Git SHA"
        )));
    }
    Ok(())
}

fn validate_utc_timestamp(value: &str) -> Result<(), EvidenceError> {
    let bytes = value.as_bytes();
    let shape = bytes.len() == 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && bytes.get(19) == Some(&b'Z')
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        });
    if !shape {
        return Err(EvidenceError::new(
            "observed_at must use canonical UTC seconds format YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let component = |start: usize, end: usize| -> Result<u32, EvidenceError> {
        value[start..end]
            .parse::<u32>()
            .map_err(|error| EvidenceError::new(format!("invalid observed_at component: {error}")))
    };
    let month = component(5, 7)?;
    let day = component(8, 10)?;
    let hour = component(11, 13)?;
    let minute = component(14, 16)?;
    let second = component(17, 19)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(EvidenceError::new(
            "observed_at contains an out-of-range UTC date/time component",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        HostedEvidenceContextV1, HostedEvidenceEnvelopeV1, TypedPayload, canonical_json,
        reject_secret_material,
    };
    use serde_json::{Value, json};

    fn context() -> Value {
        json!({
            "schema_version": 1,
            "evidence_kind": "credential_readiness",
            "payload_version": 1,
            "repository": "iamaman11/part-crm-emai-profile",
            "source_sha": "0123456789abcdef0123456789abcdef01234567",
            "source_ref": "refs/heads/main",
            "workflow": {
                "name": "Hosted probe",
                "workflow_ref": "iamaman11/part-crm-emai-profile/.github/workflows/probe.yml@refs/heads/main",
                "run_id": 42,
                "run_attempt": 1,
                "observation_job": "observe"
            },
            "environment": "staging",
            "observed_at": "2026-08-21T18:00:00Z",
            "provider_mutation": false,
            "production_mutation": false
        })
    }

    fn payload() -> Value {
        json!({
            "provider": "cloudflare",
            "credential_identity": "staging-observe",
            "provider_metadata_identifier": "token-metadata-id",
            "status": "READY",
            "capabilities": ["workers.read", "d1.read"]
        })
    }

    #[test]
    fn typed_build_is_deterministic_and_normalizes_set_like_capabilities() -> Result<(), String> {
        let context = HostedEvidenceContextV1::parse(&context()).map_err(|error| error.to_string())?;
        let envelope = HostedEvidenceEnvelopeV1::from_raw(context, &payload())
            .map_err(|error| error.to_string())?;
        let first = envelope.canonical_text().map_err(|error| error.to_string())?;
        let second = envelope.canonical_text().map_err(|error| error.to_string())?;
        assert_eq!(first, second);
        assert!(first.contains(r#""capabilities":["d1.read","workers.read"]"#));
        Ok(())
    }

    #[test]
    fn unknown_kind_and_version_fail_closed() {
        let mut unknown_kind = context();
        unknown_kind["evidence_kind"] = json!("future_unknown_kind");
        assert!(HostedEvidenceContextV1::parse(&unknown_kind).is_err());

        let mut unknown_version = context();
        unknown_version["payload_version"] = json!(2);
        assert!(HostedEvidenceContextV1::parse(&unknown_version).is_err());
    }

    #[test]
    fn secret_fields_and_bearer_material_are_rejected_recursively() {
        assert!(
            reject_secret_material(&json!({"nested": {"token_value": "secret"}}), "root")
                .is_err()
        );
        assert!(reject_secret_material(&json!({"safe": "Bearer abc"}), "root").is_err());
    }

    #[test]
    fn observational_payload_rejects_provider_mutation() {
        let mut context_value = context();
        context_value["provider_mutation"] = json!(true);
        let parsed = HostedEvidenceContextV1::parse(&context_value);
        assert!(parsed.is_ok());
        if let Ok(context) = parsed {
            assert!(HostedEvidenceEnvelopeV1::from_raw(context, &payload()).is_err());
        }
    }

    #[test]
    fn production_effect_flags_fail_closed() {
        let mut context_value = context();
        context_value["environment"] = json!("production");
        context_value["provider_mutation"] = json!(true);
        context_value["production_mutation"] = json!(false);
        assert!(HostedEvidenceContextV1::parse(&context_value).is_err());
    }

    #[test]
    fn envelope_rejects_unknown_payload_fields() -> Result<(), String> {
        let context = HostedEvidenceContextV1::parse(&context()).map_err(|error| error.to_string())?;
        let mut payload = payload();
        payload["arbitrary"] = json!(true);
        assert!(HostedEvidenceEnvelopeV1::from_raw(context, &payload).is_err());
        Ok(())
    }

    #[test]
    fn release_transition_effect_policy_is_typed() -> Result<(), String> {
        let mut value = context();
        value["evidence_kind"] = json!("release_set_transition");
        value["provider_mutation"] = json!(true);
        let context = HostedEvidenceContextV1::parse(&value).map_err(|error| error.to_string())?;
        let payload = json!({
            "provider": "cloudflare",
            "profile_id": "staging-core-v1",
            "previous_release_set_id": "release-a",
            "target_release_set_id": "release-b",
            "decision": "APPLIED",
            "compatibility": "COMPATIBLE"
        });
        let envelope = HostedEvidenceEnvelopeV1::from_raw(context, &payload)
            .map_err(|error| error.to_string())?;
        assert!(matches!(
            envelope.payload,
            TypedPayload::ReleaseSetTransition(_)
        ));
        Ok(())
    }

    #[test]
    fn canonical_json_dependency_remains_shared_with_release_policy() -> Result<(), String> {
        let value = json!({"z": 1, "a": 2});
        assert_eq!(canonical_json(&value)?, r#"{"a":2,"z":1}"#);
        Ok(())
    }
}
