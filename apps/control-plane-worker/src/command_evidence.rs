use crate::request_evidence::{audit_event_id, outbox_event_id};
use application_ports::CommandExecutionEvidence;
use profile_platform_primitives::{ActorContext, IdempotencyKey, MailboxOnboardingId, UnixMillis};
use sha2::{Digest, Sha256};
use worker::{Date, Error, Request, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;
const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn from_request(
    request: &Request,
    actor: &ActorContext,
    request_digest: String,
) -> Result<CommandExecutionEvidence> {
    if !(16..=256).contains(&request_digest.len()) {
        return Err(Error::RustError(
            "request digest length is invalid".to_owned(),
        ));
    }
    let key = request
        .headers()
        .get(IDEMPOTENCY_HEADER)?
        .ok_or_else(|| Error::RustError("idempotency key missing".to_owned()))?;
    let idempotency_key =
        IdempotencyKey::parse(key).map_err(|error| Error::RustError(error.to_string()))?;
    evidence(actor, idempotency_key, request_digest)
}

pub fn from_oauth_callback(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    state: &str,
) -> Result<CommandExecutionEvidence> {
    let material = format!(
        "gmail-oauth-callback\n{}\n{}\n{}\n{}",
        actor.tenant_scope().tenant_id().as_str(),
        actor.actor_id().as_str(),
        onboarding_id.as_str(),
        state,
    );
    let digest_bytes = Sha256::digest(material.as_bytes());
    let mut digest = String::with_capacity(digest_bytes.len() * 2);
    for byte in digest_bytes {
        digest.push(HEX[usize::from(byte >> 4)] as char);
        digest.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    let idempotency_key = IdempotencyKey::parse(format!("oauthcb_{digest}"))
        .map_err(|error| Error::RustError(error.to_string()))?;
    evidence(actor, idempotency_key, digest)
}

fn evidence(
    actor: &ActorContext,
    idempotency_key: IdempotencyKey,
    request_digest: String,
) -> Result<CommandExecutionEvidence> {
    let audit_event_id = audit_event_id(
        actor.tenant_scope().tenant_id(),
        actor.actor_id(),
        &idempotency_key,
    )?;
    let outbox_event_id = outbox_event_id(
        actor.tenant_scope().tenant_id(),
        actor.actor_id(),
        &idempotency_key,
    )?;
    let now = Date::now().as_millis();
    let expires_at = now
        .checked_add(IDEMPOTENCY_TTL_MS)
        .ok_or_else(|| Error::RustError("idempotency expiry overflow".to_owned()))?;
    Ok(CommandExecutionEvidence::new(
        idempotency_key,
        request_digest,
        audit_event_id,
        outbox_event_id,
        UnixMillis::new(now),
        UnixMillis::new(expires_at),
    ))
}
