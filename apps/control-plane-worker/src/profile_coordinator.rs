use application_ports::device_generation_commit::{
    DeviceGenerationCommitError, DeviceGenerationCommitErrorClass, DeviceGenerationCommitRequest,
};
use cloudflare_adapters::d1_device_generation_commit::{
    D1DeviceGenerationCommitJournal, DeviceGenerationCommitJournalOutcome,
};
use cloudflare_adapters::device_generation_commit_runtime::{
    DEVICE_GENERATION_COMMIT_PATH, DeviceGenerationCommitInternalErrorClass,
    DeviceGenerationCommitInternalErrorResponse, DeviceGenerationCommitInternalOutcome,
    DeviceGenerationCommitInternalRequest, DeviceGenerationCommitInternalResponse,
};
use cloudflare_adapters::profile_coordinator::{
    CoordinatorAdapterError, CoordinatorProjection, StoredCoordinatorCommand,
    StoredCoordinatorDocument, StoredCoordinatorEnvelope, outcome_name,
};
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::{ProfileId, TenantId, UnixMillis};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{
    CoordinatorConfig, CoordinatorOutcome, CoordinatorStatus, ProfileCoordinatorState,
};
use worker::{
    Date, DateInit, DurableObject, Env, Error, Method, Request, Response, Result, ScheduledTime,
    State, durable_object,
};

const STORAGE_KEY: &str = "profile-coordinator-v1";
const GENERATION_COMMIT_GATE_KEY: &str = "profile-generation-commit-gate-v1";
const GATE_ALARM_RETRY_MS: u64 = 1_000;
const IDLE_TIMEOUT_MS: u64 = 30_000;
const HARD_TIMEOUT_MS: u64 = 900_000;
const DRAIN_TIMEOUT_MS: u64 = 60_000;

#[durable_object]
pub struct ProfileCoordinator {
    state: State,
    env: Env,
}

impl DurableObject for ProfileCoordinator {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, mut request: Request) -> Result<Response> {
        match (request.method(), request.path().as_str()) {
            (Method::Post, "/snapshot") => self.snapshot(&mut request).await,
            (Method::Post, "/command") => self.command(&mut request).await,
            (Method::Post, DEVICE_GENERATION_COMMIT_PATH) => {
                self.generation_commit(&mut request).await
            }
            _ => Response::error("Not Found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        if self.load_generation_commit_gate().await?.is_some() {
            schedule_gate_retry_alarm(&self.state).await?;
            return Response::ok("generation commit reserved");
        }
        let Some(mut document) = self
            .state
            .storage()
            .get::<StoredCoordinatorDocument>(STORAGE_KEY)
            .await?
        else {
            return Response::ok("no coordinator state");
        };
        let state = document.replay().map_err(adapter_error)?;
        let sequence = state
            .last_sequence()
            .checked_add(1)
            .ok_or_else(|| Error::RustError("coordinator sequence overflow".to_owned()))?;
        let envelope = StoredCoordinatorEnvelope::new(
            format!("alarm_{sequence:020}"),
            sequence,
            state.version().value(),
            StoredCoordinatorCommand::Tick {
                now_ms: Date::now().as_millis(),
            },
        );
        let applied = document.apply(envelope).map_err(adapter_error)?;
        if applied.appended() {
            self.state.storage().put(STORAGE_KEY, &document).await?;
        }
        schedule_alarm(&self.state, applied.next_alarm_at()).await?;
        Response::from_json(&CoordinatorObjectResponse::from_applied(&applied))
    }
}

impl ProfileCoordinator {
    async fn snapshot(&self, request: &mut Request) -> Result<Response> {
        let body = request.json::<CoordinatorSnapshotOwned>().await?;
        let tenant_id = TenantId::parse(body.tenant_id).map_err(identifier_error)?;
        let profile_id = ProfileId::parse(body.profile_id).map_err(identifier_error)?;
        let document = self.load_document(&tenant_id, &profile_id).await?;
        let projection = document.projection().map_err(adapter_error)?;
        Response::from_json(&CoordinatorObjectResponse::from_snapshot(projection))
    }

    async fn command(&self, request: &mut Request) -> Result<Response> {
        if self.load_generation_commit_gate().await?.is_some() {
            return coordinator_busy();
        }
        let body = request.json::<CoordinatorInternalCommandOwned>().await?;
        let tenant_id = TenantId::parse(body.tenant_id).map_err(identifier_error)?;
        let profile_id = ProfileId::parse(body.profile_id).map_err(identifier_error)?;
        let mut document = self.load_document(&tenant_id, &profile_id).await?;
        let applied = match document.apply(body.envelope) {
            Ok(value) => value,
            Err(error) => return coordinator_conflict(error),
        };
        if applied.appended() {
            self.state.storage().put(STORAGE_KEY, &document).await?;
        }
        schedule_alarm(&self.state, applied.next_alarm_at()).await?;
        Response::from_json(&CoordinatorObjectResponse::from_applied(&applied))
    }

    async fn generation_commit(&self, request: &mut Request) -> Result<Response> {
        let internal = match request
            .json::<DeviceGenerationCommitInternalRequest>()
            .await
        {
            Ok(value) => value,
            Err(_) => return generation_commit_error(integrity_failure()),
        };
        let authority_digest = internal.authority_digest();
        let existing_gate = self.load_generation_commit_gate().await?;
        let observed_at = existing_gate.as_ref().map_or_else(
            || UnixMillis::new(Date::now().as_millis()),
            |gate| UnixMillis::new(gate.authorized_at_ms),
        );
        let (actor, commit) = match internal.into_domain(observed_at) {
            Ok(value) => value,
            Err(error) => return generation_commit_error(error),
        };

        let document = self
            .load_document(actor.tenant_scope().tenant_id(), commit.profile_id())
            .await?;
        let coordinator = match document.replay() {
            Ok(value) => value,
            Err(error) => return generation_commit_error(adapter_integrity(error)),
        };

        match existing_gate {
            Some(gate) => {
                if !gate.matches(&authority_digest, &commit) {
                    return generation_commit_error(version_conflict());
                }
                if let Err(error) = validate_generation_commit_authority(
                    &coordinator,
                    &commit,
                    UnixMillis::new(gate.authorized_at_ms),
                ) {
                    return generation_commit_error(error);
                }
            }
            None => {
                if let Err(error) =
                    validate_generation_commit_authority(&coordinator, &commit, observed_at)
                {
                    return generation_commit_error(error);
                }
                let gate = StoredGenerationCommitGate::new(authority_digest, &commit, observed_at);
                self.state
                    .storage()
                    .put(GENERATION_COMMIT_GATE_KEY, &gate)
                    .await?;
            }
        }

        let journal = D1DeviceGenerationCommitJournal::new(self.env.d1(D1_CATALOG_BINDING)?);
        let outcome = journal.apply(&actor, &commit).await;
        match outcome {
            Ok(DeviceGenerationCommitJournalOutcome::Applied) => {
                self.clear_generation_commit_gate().await?;
                Response::from_json(&DeviceGenerationCommitInternalResponse {
                    outcome: DeviceGenerationCommitInternalOutcome::Activated,
                })
            }
            Ok(DeviceGenerationCommitJournalOutcome::ExactReplay) => {
                self.clear_generation_commit_gate().await?;
                Response::from_json(&DeviceGenerationCommitInternalResponse {
                    outcome: DeviceGenerationCommitInternalOutcome::AlreadyActive,
                })
            }
            Err(error) => {
                if generation_commit_failure_releases_gate(error.class()) {
                    self.clear_generation_commit_gate().await?;
                } else {
                    schedule_gate_retry_alarm(&self.state).await?;
                }
                generation_commit_error(error)
            }
        }
    }

    async fn load_generation_commit_gate(&self) -> Result<Option<StoredGenerationCommitGate>> {
        self.state
            .storage()
            .get::<StoredGenerationCommitGate>(GENERATION_COMMIT_GATE_KEY)
            .await
    }

    async fn clear_generation_commit_gate(&self) -> Result<()> {
        let _ = self
            .state
            .storage()
            .delete(GENERATION_COMMIT_GATE_KEY)
            .await?;
        Ok(())
    }

    async fn load_document(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
    ) -> Result<StoredCoordinatorDocument> {
        if let Some(document) = self
            .state
            .storage()
            .get::<StoredCoordinatorDocument>(STORAGE_KEY)
            .await?
        {
            document
                .ensure_identity(tenant_id, profile_id)
                .map_err(adapter_error)?;
            return Ok(document);
        }
        Ok(StoredCoordinatorDocument::new(
            tenant_id,
            profile_id,
            CoordinatorConfig::new(IDLE_TIMEOUT_MS, HARD_TIMEOUT_MS, DRAIN_TIMEOUT_MS)
                .map_err(domain_error)?,
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredGenerationCommitGate {
    authority_digest: String,
    job_id: String,
    generation_id: String,
    authorized_at_ms: u64,
}

impl StoredGenerationCommitGate {
    fn new(
        authority_digest: String,
        request: &DeviceGenerationCommitRequest,
        authorized_at: UnixMillis,
    ) -> Self {
        Self {
            authority_digest,
            job_id: request.job_id().as_str().to_owned(),
            generation_id: request.object().generation_id().as_str().to_owned(),
            authorized_at_ms: authorized_at.value(),
        }
    }

    fn matches(&self, authority_digest: &str, request: &DeviceGenerationCommitRequest) -> bool {
        self.authority_digest == authority_digest
            && self.job_id == request.job_id().as_str()
            && self.generation_id == request.object().generation_id().as_str()
    }
}

fn validate_generation_commit_authority(
    coordinator: &ProfileCoordinatorState,
    request: &DeviceGenerationCommitRequest,
    authorized_at: UnixMillis,
) -> core::result::Result<(), DeviceGenerationCommitError> {
    if !matches!(
        coordinator.status(),
        CoordinatorStatus::Active | CoordinatorStatus::Draining
    ) || coordinator.version().value() != request.coordinator().coordinator_version()
        || coordinator.last_sequence() != request.coordinator().coordinator_sequence()
        || authorized_at < coordinator.last_observed_at()
    {
        return Err(stale_authority());
    }
    let Some(lease) = coordinator.active_lease() else {
        return Err(stale_authority());
    };
    if lease.device_id() != request.device_id()
        || !lease.accepts_writer(
            request.coordinator().session_id(),
            request.coordinator().epoch(),
            request.coordinator().fencing_token(),
        )
        || authorized_at >= lease.idle_expires_at()
        || authorized_at >= lease.hard_expires_at()
    {
        return Err(stale_authority());
    }
    if coordinator.status() == CoordinatorStatus::Draining
        && coordinator
            .drain_deadline()
            .is_none_or(|deadline| authorized_at >= deadline)
    {
        return Err(stale_authority());
    }
    Ok(())
}

#[derive(Deserialize)]
struct CoordinatorSnapshotOwned {
    tenant_id: String,
    profile_id: String,
}

#[derive(Deserialize)]
struct CoordinatorInternalCommandOwned {
    tenant_id: String,
    profile_id: String,
    envelope: StoredCoordinatorEnvelope,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CoordinatorObjectResponse {
    outcome: String,
    version: u64,
    sequence: u64,
    replayed: bool,
    fencing_token: Option<String>,
    epoch: Option<u64>,
    projection: CoordinatorProjection,
}

impl CoordinatorObjectResponse {
    fn from_snapshot(projection: CoordinatorProjection) -> Self {
        Self {
            outcome: "snapshot".to_owned(),
            version: projection.version,
            sequence: projection.sequence,
            replayed: true,
            fencing_token: None,
            epoch: projection.active_epoch,
            projection,
        }
    }

    fn from_applied(
        applied: &cloudflare_adapters::profile_coordinator::CoordinatorApplied,
    ) -> Self {
        let (fencing_token, epoch) = match applied.decision().outcome() {
            CoordinatorOutcome::LeaseClaimed { lease } => (
                Some(lease.fencing_token().as_str().to_owned()),
                Some(lease.epoch()),
            ),
            _ => (None, None),
        };
        Self {
            outcome: outcome_name(applied.decision().outcome()).to_owned(),
            version: applied.decision().version().value(),
            sequence: applied.decision().sequence(),
            replayed: !applied.appended(),
            fencing_token,
            epoch,
            projection: applied.projection().clone(),
        }
    }
}

async fn schedule_alarm(state: &State, deadline: Option<UnixMillis>) -> Result<()> {
    match deadline {
        Some(deadline) => {
            let date = Date::new(DateInit::Millis(deadline.value()));
            state
                .storage()
                .set_alarm(ScheduledTime::new(date.into()))
                .await
        }
        None => state.storage().delete_alarm().await,
    }
}

async fn schedule_gate_retry_alarm(state: &State) -> Result<()> {
    let retry_at = Date::now()
        .as_millis()
        .checked_add(GATE_ALARM_RETRY_MS)
        .ok_or_else(|| Error::RustError("generation commit alarm overflow".to_owned()))?;
    let date = Date::new(DateInit::Millis(retry_at));
    state
        .storage()
        .set_alarm(ScheduledTime::new(date.into()))
        .await
}

fn coordinator_busy() -> Result<Response> {
    Response::from_json(&CoordinatorErrorResponse { code: "conflict" })
        .map(|response| response.with_status(409))
}

fn coordinator_conflict(error: CoordinatorAdapterError) -> Result<Response> {
    let code = match error {
        CoordinatorAdapterError::TenantMismatch | CoordinatorAdapterError::ProfileMismatch => 404,
        CoordinatorAdapterError::Identifier(_) | CoordinatorAdapterError::ZeroVersion(_) => 400,
        CoordinatorAdapterError::Domain(_) | CoordinatorAdapterError::JournalCapacityExceeded => {
            409
        }
    };
    Response::from_json(&CoordinatorErrorResponse {
        code: if code == 404 {
            "not_found"
        } else if code == 400 {
            "invalid_request"
        } else {
            "conflict"
        },
    })
    .map(|response| response.with_status(code))
}

fn generation_commit_failure_releases_gate(class: DeviceGenerationCommitErrorClass) -> bool {
    matches!(
        class,
        DeviceGenerationCommitErrorClass::StaleAuthority
            | DeviceGenerationCommitErrorClass::VersionConflict
    )
}

fn generation_commit_error(error: DeviceGenerationCommitError) -> Result<Response> {
    let (status, class) = match error.class() {
        DeviceGenerationCommitErrorClass::StaleAuthority => (
            409,
            DeviceGenerationCommitInternalErrorClass::StaleAuthority,
        ),
        DeviceGenerationCommitErrorClass::VersionConflict => (
            409,
            DeviceGenerationCommitInternalErrorClass::VersionConflict,
        ),
        DeviceGenerationCommitErrorClass::IntegrityFailure => (
            500,
            DeviceGenerationCommitInternalErrorClass::IntegrityFailure,
        ),
        DeviceGenerationCommitErrorClass::DependencyUnavailable => (
            503,
            DeviceGenerationCommitInternalErrorClass::DependencyUnavailable,
        ),
    };
    Response::from_json(&DeviceGenerationCommitInternalErrorResponse { class })
        .map(|response| response.with_status(status))
}

#[derive(Serialize)]
struct CoordinatorErrorResponse {
    code: &'static str,
}

fn adapter_integrity(_error: CoordinatorAdapterError) -> DeviceGenerationCommitError {
    integrity_failure()
}

fn stale_authority() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::StaleAuthority)
}

fn version_conflict() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::VersionConflict)
}

fn integrity_failure() -> DeviceGenerationCommitError {
    DeviceGenerationCommitError::new(DeviceGenerationCommitErrorClass::IntegrityFailure)
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

fn adapter_error(error: CoordinatorAdapterError) -> Error {
    Error::RustError(error.to_string())
}

fn domain_error(error: session_domain::coordinator::CoordinatorError) -> Error {
    Error::RustError(error.to_string())
}
