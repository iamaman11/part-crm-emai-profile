use cloudflare_adapters::profile_coordinator::{
    CoordinatorAdapterError, CoordinatorProjection, StoredCoordinatorCommand,
    StoredCoordinatorDocument, StoredCoordinatorEnvelope, outcome_name,
};
use profile_platform_primitives::{ProfileId, TenantId, UnixMillis};
use serde::{Deserialize, Serialize};
use session_domain::coordinator::{CoordinatorConfig, CoordinatorOutcome};
use worker::{
    Date, DateInit, DurableObject, Env, Error, Method, Request, Response, Result, ScheduledTime,
    State, durable_object,
};

const STORAGE_KEY: &str = "profile-coordinator-v1";
const IDLE_TIMEOUT_MS: u64 = 30_000;
const HARD_TIMEOUT_MS: u64 = 900_000;
const DRAIN_TIMEOUT_MS: u64 = 60_000;

#[durable_object]
pub struct ProfileCoordinator {
    state: State,
    _env: Env,
}

impl DurableObject for ProfileCoordinator {
    fn new(state: State, env: Env) -> Self {
        Self { state, _env: env }
    }

    async fn fetch(&self, mut request: Request) -> Result<Response> {
        match (request.method(), request.path().as_str()) {
            (Method::Post, "/snapshot") => self.snapshot(&mut request).await,
            (Method::Post, "/command") => self.command(&mut request).await,
            _ => Response::error("Not Found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
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

#[derive(Serialize)]
struct CoordinatorErrorResponse {
    code: &'static str,
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
