//! Offline typed policy for Hosted Operational Evidence.
//!
//! Provider-native tools and GitHub Actions own observation, mutation, credentials,
//! orchestration and signing. This module only validates saved JSON, canonicalizes it,
//! and verifies an evidence artifact against an independently supplied run context.

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
pub struct EvidenceError(String);

impl EvidenceError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for EvidenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
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
    fn parse(kind: &str, version: u64) -> Result<Self, EvidenceError> {
        if version != 1 {
            return Err(EvidenceError::new(format!(
                "unsupported payload version {version} for evidence kind {kind}"
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
        let object = as_object(value, "workflow")?;
        strict_fields(
            object,
            &["name", "workflow_ref", "run_id", "run_attempt", "observation_job"],
            "workflow",
        )?;
        let result = Self {
            name: required_string(object, "name", "workflow")?,
            workflow_ref: required_string(object, "workflow_ref", "workflow")?,
            run_id: required_u64(object, "run_id", "workflow")?,
            run_attempt: required_u64(object, "run_attempt", "workflow")?,
            observation_job: required_string(object, "observation_job", "workflow")?,
        };
        validate_text(&result.name, "workflow.name", 256)?;
        validate_text(&result.workflow_ref, "workflow.workflow_ref", 512)?;
        validate_text(&result.observation_job, "workflow.observation_job", 256)?;
        if result.run_id == 0 || result.run_attempt == 0 {
            return Err(EvidenceError::new(
                "workflow run_id and run_attempt must be positive integers",
            ));
        }
        Ok(result)
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
        let object = as_object(value, "hosted evidence context")?;
        strict_fields(object, CONTEXT_FIELDS, "hosted evidence context")?;
        Self::from_fields(object)
    }

    fn from_fields(object: &Map<String, Value>) -> Result<Self, EvidenceError> {
        let schema = required_u64(object, "schema_version", "hosted evidence context")?;
        if schema != HOSTED_EVIDENCE_CONTEXT_VERSION {
            return Err(EvidenceError::new(format!(
                "unsupported HostedEvidenceEnvelope schema_version: {schema}"
            )));
        }
        let payload_version =
            required_u64(object, "payload_version", "hosted evidence context")?;
        let kind = required_string(object, "evidence_kind", "hosted evidence context")?;
        let evidence_kind = EvidenceKind::parse(&kind, payload_version)?;
        let repository = required_string(object, "repository", "hosted evidence context")?;
        if repository != EXPECTED_REPOSITORY {
            return Err(EvidenceError::new(format!(
                "hosted evidence repository must be {EXPECTED_REPOSITORY}"
            )));
        }
        let source_sha = required_string(object, "source_sha", "hosted evidence context")?;
        validate_sha(&source_sha)?;
        let source_ref = required_string(object, "source_ref", "hosted evidence context")?;
        if !source_ref.starts_with("refs/") {
            return Err(EvidenceError::new(
                "hosted evidence source_ref must start with refs/",
            ));
        }
        validate_text(&source_ref, "source_ref", 512)?;
        let workflow = WorkflowIdentity::parse(required(
            object,
            "workflow",
            "hosted evidence context",
        )?)?;
        let environment = Environment::parse(&required_string(
            object,
            "environment",
            "hosted evidence context",
        )?)?;
        let observed_at = required_string(object, "observed_at", "hosted evidence context")?;
        validate_utc_timestamp(&observed_at)?;
        let provider_mutation =
            required_bool(object, "provider_mutation", "hosted evidence context")?;
        let production_mutation =
            required_bool(object, "production_mutation", "hosted evidence context")?;
        validate_context_effects(environment, provider_mutation, production_mutation)?;
        Ok(Self {
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

    fn to_value(&self) -> Value {
        json!({
            "schema_version": HOSTED_EVIDENCE_SCHEMA_VERSION,
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

const CONTEXT_FIELDS: &[&str] = &[
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
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadinessStatus {
    Ready,
    NotReady,
}

impl ReadinessStatus {
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
    status: ReadinessStatus,
    capabilities: Vec<String>,
}

impl CredentialReadinessV1 {
    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        let object = as_object(value, "credential_readiness payload")?;
        strict_fields(
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
        let status = ReadinessStatus::parse(&required_string(
            object,
            "status",
            "credential_readiness payload",
        )?)?;
        let capabilities =
            required_string_set(object, "capabilities", "credential_readiness payload")?;
        validate_text(&provider, "payload.provider", 128)?;
        validate_text(&credential_identity, "payload.credential_identity", 256)?;
        validate_text(
            &provider_metadata_identifier,
            "payload.provider_metadata_identifier",
            256,
        )?;
        if status == ReadinessStatus::Ready && capabilities.is_empty() {
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
        let object = as_object(value, "hosted_resource_state payload")?;
        strict_fields(
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
        let result = Self {
            provider: required_string(object, "provider", "hosted_resource_state payload")?,
            resource_type: required_string(
                object,
                "resource_type",
                "hosted_resource_state payload",
            )?,
            resource_id: required_string(
                object,
                "resource_id",
                "hosted_resource_state payload",
            )?,
            state: required_string(object, "state", "hosted_resource_state payload")?,
            revision: nullable_string(object, "revision", "hosted_resource_state payload")?,
            enabled: nullable_bool(object, "enabled", "hosted_resource_state payload")?,
        };
        validate_text(&result.provider, "payload.provider", 128)?;
        validate_text(&result.resource_type, "payload.resource_type", 128)?;
        validate_text(&result.resource_id, "payload.resource_id", 512)?;
        validate_text(&result.state, "payload.state", 128)?;
        if let Some(revision) = &result.revision {
            validate_text(revision, "payload.revision", 512)?;
        }
        Ok(result)
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

    const fn mutates_provider(self) -> bool {
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
        let object = as_object(value, "release_set_transition payload")?;
        strict_fields(
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
        let result = Self {
            provider: required_string(object, "provider", "release_set_transition payload")?,
            profile_id: required_string(object, "profile_id", "release_set_transition payload")?,
            previous_release_set_id: nullable_string(
                object,
                "previous_release_set_id",
                "release_set_transition payload",
            )?,
            target_release_set_id: required_string(
                object,
                "target_release_set_id",
                "release_set_transition payload",
            )?,
            decision: TransitionDecision::parse(&required_string(
                object,
                "decision",
                "release_set_transition payload",
            )?)?,
            compatibility: CompatibilityDecision::parse(&required_string(
                object,
                "compatibility",
                "release_set_transition payload",
            )?)?,
        };
        validate_text(&result.provider, "payload.provider", 128)?;
        validate_text(&result.profile_id, "payload.profile_id", 256)?;
        validate_text(
            &result.target_release_set_id,
            "payload.target_release_set_id",
            256,
        )?;
        if let Some(previous) = &result.previous_release_set_id {
            validate_text(previous, "payload.previous_release_set_id", 256)?;
        }
        if result.decision.mutates_provider()
            && result.compatibility != CompatibilityDecision::Compatible
        {
            return Err(EvidenceError::new(
                "APPLIED/ROLLED_BACK Release Set evidence requires COMPATIBLE policy decision",
            ));
        }
        Ok(result)
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
    fn parse(kind: EvidenceKind, version: u64, value: &Value) -> Result<Self, EvidenceError> {
        if version != 1 {
            return Err(EvidenceError::new(format!(
                "unsupported payload version {version}"
            )));
        }
        match kind {
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

    fn provider_mutation(&self) -> bool {
        match self {
            Self::CredentialReadiness(_) | Self::HostedResourceState(_) => false,
            Self::ReleaseSetTransition(value) => value.decision.mutates_provider(),
        }
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
        validate_payload_effect(&context, &payload)?;
        Ok(Self { context, payload })
    }

    fn parse(value: &Value) -> Result<Self, EvidenceError> {
        reject_secret_material(value, "hosted_evidence")?;
        let object = as_object(value, "HostedEvidenceEnvelopeV1")?;
        let mut envelope_fields = CONTEXT_FIELDS.to_vec();
        envelope_fields.push("payload");
        strict_fields(object, &envelope_fields, "HostedEvidenceEnvelopeV1")?;
        let context = HostedEvidenceContextV1::from_fields(object)?;
        let payload = TypedPayload::parse(
            context.evidence_kind,
            context.payload_version,
            required(object, "payload", "HostedEvidenceEnvelopeV1")?,
        )?;
        validate_payload_effect(&context, &payload)?;
        Ok(Self { context, payload })
    }

    fn to_value(&self) -> Value {
        let mut value = self.context.to_value();
        value
            .as_object_mut()
            .expect("context serialization is always an object")
            .insert("payload".to_owned(), self.payload.to_value());
        value
    }

    fn canonical_text(&self) -> Result<String, EvidenceError> {
        canonical_line(&self.to_value())
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
    let context_value = read_json_bounded(context_path, "hosted evidence context")?.1;
    let raw_value = read_json_bounded(raw_path, "raw hosted observation")?.1;
    let context = HostedEvidenceContextV1::parse(&context_value)?;
    HostedEvidenceEnvelopeV1::from_raw(context, &raw_value)?.canonical_text()
}

fn validate(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let path = evidence_only_path(&request, "validate")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&read_json_bounded(path, "hosted evidence")?.1)?;
    summary("VALID", &envelope, None)
}

fn inspect(request: EvidenceRunRequest<'_>) -> Result<String, EvidenceError> {
    let path = evidence_only_path(&request, "inspect")?;
    let (text, value) = read_json_bounded(path, "hosted evidence")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&value)?;
    canonical_line(&json!({
        "decision": "INSPECTED",
        "sha256": sha256_hex(text.as_bytes()),
        "canonical": text == envelope.canonical_text()?,
        "envelope": envelope.to_value(),
    }))
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
    let (text, value) = read_json_bounded(evidence_path, "hosted evidence")?;
    let envelope = HostedEvidenceEnvelopeV1::parse(&value)?;
    if text != envelope.canonical_text()? {
        return Err(EvidenceError::new(
            "hosted evidence bytes are not the deterministic canonical serialization",
        ));
    }
    let expected_value = read_json_bounded(context_path, "expected hosted evidence context")?.1;
    let expected = HostedEvidenceContextV1::parse(&expected_value)?;
    if envelope.context != expected {
        return Err(EvidenceError::new(
            "hosted evidence context does not match the independent expected workflow/run/source binding",
        ));
    }
    summary("VERIFIED", &envelope, Some(sha256_hex(text.as_bytes())))
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

fn summary(
    decision: &str,
    envelope: &HostedEvidenceEnvelopeV1,
    digest: Option<String>,
) -> Result<String, EvidenceError> {
    canonical_line(&json!({
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
        "sha256": digest,
    }))
}

fn validate_context_effects(
    environment: Environment,
    provider_mutation: bool,
    production_mutation: bool,
) -> Result<(), EvidenceError> {
    if production_mutation && (!provider_mutation || environment != Environment::Production) {
        return Err(EvidenceError::new(
            "production_mutation=true requires provider mutation in production",
        ));
    }
    if environment == Environment::Production && provider_mutation != production_mutation {
        return Err(EvidenceError::new(
            "production provider mutation must be represented by production_mutation=true",
        ));
    }
    Ok(())
}

fn validate_payload_effect(
    context: &HostedEvidenceContextV1,
    payload: &TypedPayload,
) -> Result<(), EvidenceError> {
    if context.provider_mutation != payload.provider_mutation() {
        return Err(EvidenceError::new(
            "evidence payload decision and provider_mutation flag are inconsistent",
        ));
    }
    Ok(())
}

fn read_json_bounded(path: &Path, label: &str) -> Result<(String, Value), EvidenceError> {
    let metadata = fs::metadata(path)
        .map_err(|error| EvidenceError::new(format!("{label} metadata unavailable: {error}")))?;
    if !metadata.is_file() || metadata.len() > MAX_JSON_BYTES {
        return Err(EvidenceError::new(format!(
            "{label} must be a regular file not exceeding {MAX_JSON_BYTES} bytes"
        )));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| EvidenceError::new(format!("{label} is unreadable UTF-8: {error}")))?;
    let value = serde_json::from_str(&text)
        .map_err(|error| EvidenceError::new(format!("{label} is invalid JSON: {error}")))?;
    Ok((text, value))
}

fn reject_secret_material(value: &Value, path: &str) -> Result<(), EvidenceError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if forbidden_secret_key(key) {
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
        Value::String(text) => reject_secret_text(text, path)?,
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn forbidden_secret_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "authorization"
            | "credential_value"
            | "key_material"
            | "password"
            | "plaintext"
            | "plaintext_value"
            | "private_key"
            | "raw_handle"
            | "value"
    ) || normalized == "token"
        || normalized == "secret"
        || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
}

fn reject_secret_text(text: &str, path: &str) -> Result<(), EvidenceError> {
    let lower = text.to_ascii_lowercase();
    let generic_private = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
    let rsa_private = ["-----BEGIN RSA ", "PRIVATE KEY-----"].concat();
    let ec_private = ["-----BEGIN EC ", "PRIVATE KEY-----"].concat();
    let openssh_private = ["-----BEGIN OPENSSH ", "PRIVATE KEY-----"].concat();
    if lower.starts_with("bearer ")
        || text.contains(&generic_private)
        || text.contains(&rsa_private)
        || text.contains(&ec_private)
        || text.contains(&openssh_private)
    {
        return Err(EvidenceError::new(format!(
            "plaintext secret material is forbidden in hosted evidence at {path}"
        )));
    }
    Ok(())
}

fn as_object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, EvidenceError> {
    value
        .as_object()
        .ok_or_else(|| EvidenceError::new(format!("{label} must be one JSON object")))
}

fn strict_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), EvidenceError> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(EvidenceError::new(format!("unknown field in {label}: {key}")));
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

fn nullable_string(
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

fn nullable_bool(
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

fn required_string_set(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Vec<String>, EvidenceError> {
    let values = required(object, key, label)?
        .as_array()
        .ok_or_else(|| EvidenceError::new(format!("{label}.{key} must be an array")))?;
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
    }
    Ok(unique.into_iter().collect())
}

fn validate_text(value: &str, label: &str, max_len: usize) -> Result<(), EvidenceError> {
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(EvidenceError::new(format!(
            "{label} must be non-empty, control-free UTF-8 not exceeding {max_len} bytes"
        )));
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<(), EvidenceError> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EvidenceError::new(
            "source_sha must be a 40-character lowercase hexadecimal Git SHA",
        ));
    }
    Ok(())
}

fn validate_utc_timestamp(value: &str) -> Result<(), EvidenceError> {
    let bytes = value.as_bytes();
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':'), (19, b'Z')];
    if bytes.len() != 20
        || separators
            .iter()
            .any(|(index, expected)| bytes.get(*index) != Some(expected))
        || bytes.iter().enumerate().any(|(index, byte)| {
            !separators.iter().any(|(separator, _)| *separator == index) && !byte.is_ascii_digit()
        })
    {
        return Err(EvidenceError::new(
            "observed_at must use canonical UTC seconds format YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let number = |start: usize, end: usize| -> Result<u32, EvidenceError> {
        value[start..end]
            .parse::<u32>()
            .map_err(|_| EvidenceError::new("observed_at contains an invalid numeric component"))
    };
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
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

fn canonical_line(value: &Value) -> Result<String, EvidenceError> {
    canonical_json(value)
        .map(|text| format!("{text}\n"))
        .map_err(EvidenceError::new)
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
    fn typed_build_is_deterministic_and_normalizes_capabilities() -> Result<(), String> {
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
    fn secret_fields_and_material_are_rejected_recursively() {
        let secret_field = ["api", "token"].join("_");
        let value = json!({"nested": {secret_field: "redacted-for-test"}});
        assert!(reject_secret_material(&value, "root").is_err());
        assert!(reject_secret_material(&json!({"safe": "Bearer redacted"}), "root").is_err());
        let marker = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
        assert!(reject_secret_material(&json!({"safe": marker}), "root").is_err());
    }

    #[test]
    fn observational_payload_rejects_provider_mutation() -> Result<(), String> {
        let mut value = context();
        value["provider_mutation"] = json!(true);
        let context = HostedEvidenceContextV1::parse(&value).map_err(|error| error.to_string())?;
        assert!(HostedEvidenceEnvelopeV1::from_raw(context, &payload()).is_err());
        Ok(())
    }

    #[test]
    fn production_effect_flags_fail_closed() {
        let mut value = context();
        value["environment"] = json!("production");
        value["provider_mutation"] = json!(true);
        assert!(HostedEvidenceContextV1::parse(&value).is_err());
    }

    #[test]
    fn unknown_payload_fields_fail_closed() -> Result<(), String> {
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
        assert!(matches!(envelope.payload, TypedPayload::ReleaseSetTransition(_)));
        Ok(())
    }

    #[test]
    fn canonicalization_remains_shared_with_release_policy() -> Result<(), String> {
        assert_eq!(canonical_json(&json!({"z": 1, "a": 2}))?, r#"{"a":2,"z":1}"#);
        Ok(())
    }
}
