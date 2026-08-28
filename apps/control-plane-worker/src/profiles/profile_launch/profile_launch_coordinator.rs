use application_ports::ClockPort;
use application_ports::coordinator_ingress::CoordinatorRuntimeOutcome;
use cloudflare_adapters::coordinator_ingress::{
    CloudflareCoordinatorClock, CloudflareCoordinatorIngressApplication,
};
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, DeviceId, IdempotencyKey, LaunchIntentId, ProfileId};
use sha2::{Digest, Sha256};
use use_cases::coordinator_ingress::{
    CoordinatorCommandInput, CoordinatorIngressOperationError, CoordinatorIngressRequest,
    ExecuteCoordinatorCommand, execute_prepared_coordinator_ingress, prepare_coordinator_ingress,
};
use use_cases::{ApplicationError, ProblemCode};
use worker::Env;

const BRIDGE_LAUNCH_INTENT_TTL_MS: u64 = 30_000;
const INTENT_DOMAIN: &str = "part-crm:bridge-launch-intent:v1";
const IDEMPOTENCY_DOMAIN: &str = "part-crm:bridge-launch-intent-idempotency:v1";

pub(super) async fn ensure_bridge_launch_intent(
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    device_id: &DeviceId,
    claim_code: &str,
) -> Result<LaunchIntentId, ApplicationError> {
    let launch_intent_id = LaunchIntentId::parse(format!(
        "launch_{}",
        digest_domain(INTENT_DOMAIN, claim_code)
    ))
    .map_err(|_| ApplicationError::new(ProblemCode::IntegrityFailure))?;
    let idempotency_key = IdempotencyKey::parse(format!(
        "idem_{}",
        digest_domain(IDEMPOTENCY_DOMAIN, claim_code)
    ))
    .map_err(|_| ApplicationError::new(ProblemCode::IntegrityFailure))?;

    let application = CloudflareCoordinatorIngressApplication::new(
        env,
        D1_CATALOG_BINDING,
        PROFILE_COORDINATOR_BINDING,
    );
    let clock = CloudflareCoordinatorClock;
    let access = prepare_coordinator_ingress(actor, role, profile_id, &application)
        .await
        .map_err(map_coordinator_error)?;
    let snapshot = execute_prepared_coordinator_ingress(
        actor,
        role,
        &access,
        &application,
        &clock,
        CoordinatorIngressRequest::Snapshot,
    )
    .await
    .map_err(map_coordinator_error)?;

    let projection = snapshot.projection();
    if projection.active_session_id().is_some() {
        return Err(ApplicationError::new(ProblemCode::LeaseConflict));
    }
    if let Some(existing) = projection.pending_launch_intent_id() {
        let still_live = projection
            .pending_intent_expires_at()
            .is_some_and(|expires_at| expires_at > clock.now());
        if existing == &launch_intent_id && still_live {
            return Ok(launch_intent_id);
        }
        return Err(ApplicationError::new(ProblemCode::LeaseConflict));
    }

    let sequence = snapshot
        .sequence()
        .checked_add(1)
        .ok_or_else(|| ApplicationError::new(ProblemCode::IntegrityFailure))?;
    let command = ExecuteCoordinatorCommand::new(
        idempotency_key,
        sequence,
        snapshot.version(),
        CoordinatorCommandInput::IssueLaunchIntent {
            launch_intent_id: launch_intent_id.clone(),
            device_id: device_id.clone(),
            expires_in_ms: BRIDGE_LAUNCH_INTENT_TTL_MS,
        },
    );
    let issued = execute_prepared_coordinator_ingress(
        actor,
        role,
        &access,
        &application,
        &clock,
        CoordinatorIngressRequest::Command(command),
    )
    .await
    .map_err(map_coordinator_error)?;
    if issued.outcome() != CoordinatorRuntimeOutcome::LaunchIntentIssued
        || issued.projection().pending_launch_intent_id() != Some(&launch_intent_id)
    {
        return Err(ApplicationError::new(ProblemCode::IntegrityFailure));
    }
    Ok(launch_intent_id)
}

fn digest_domain(domain: &str, claim_code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\n");
    hasher.update(claim_code.as_bytes());
    hex_encode(hasher.finalize().as_slice())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn map_coordinator_error(error: CoordinatorIngressOperationError) -> ApplicationError {
    ApplicationError::new(match error {
        CoordinatorIngressOperationError::InvalidRequest => ProblemCode::InvalidRequest,
        CoordinatorIngressOperationError::NotFound => ProblemCode::NotFound,
        CoordinatorIngressOperationError::Conflict => ProblemCode::LeaseConflict,
        CoordinatorIngressOperationError::IntegrityFailure => ProblemCode::IntegrityFailure,
        CoordinatorIngressOperationError::InternalFailure => ProblemCode::InternalFailure,
        CoordinatorIngressOperationError::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::{BRIDGE_LAUNCH_INTENT_TTL_MS, IDEMPOTENCY_DOMAIN, INTENT_DOMAIN, digest_domain};

    #[test]
    fn coordinator_continuation_is_short_lived_and_domain_separated() {
        assert_eq!(BRIDGE_LAUNCH_INTENT_TTL_MS, 30_000);
        assert_ne!(INTENT_DOMAIN, IDEMPOTENCY_DOMAIN);
        let claim = "a".repeat(64);
        assert_ne!(
            digest_domain(INTENT_DOMAIN, &claim),
            digest_domain(IDEMPOTENCY_DOMAIN, &claim)
        );
        assert!(!digest_domain(INTENT_DOMAIN, &claim).contains(&claim));
    }
}
