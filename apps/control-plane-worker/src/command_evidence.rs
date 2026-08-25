use crate::request_evidence::{audit_event_id, outbox_event_id};
use application_ports::CommandExecutionEvidence;
use profile_platform_primitives::{
    ActorContext, IdempotencyKey, MailboxOnboardingId, PayloadFingerprint, UnixMillis,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use worker::{Date, Error, Request, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;
const FINGERPRINT_DOMAIN: &[u8] = b"part-crm:payload-fingerprint:v1";
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Builds command execution evidence only after a request has been decoded into its typed DTO.
///
/// The fingerprint is server-owned and covers the normalized method + concrete route + typed payload,
/// so reusing one idempotency key for a different resource or payload conflicts instead of replaying.
pub fn from_request<T: Serialize>(
    request: &Request,
    actor: &ActorContext,
    payload: &T,
) -> Result<CommandExecutionEvidence> {
    let idempotency_key = request_idempotency_key(request)?;
    let payload_fingerprint = fingerprint_typed_request(request, payload)?;
    evidence(actor, idempotency_key, payload_fingerprint)
}

#[allow(clippy::too_many_arguments)]
pub fn from_standards_password_onboarding(
    request: &Request,
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    expected_version: u64,
    imap_host: &str,
    imap_port: u16,
    imap_transport: &str,
    imap_username: &str,
    smtp_host: &str,
    smtp_port: u16,
    smtp_transport: &str,
    smtp_username: &str,
) -> Result<CommandExecutionEvidence> {
    let material = format!(
        "standards-password\n{}\n{}\n{}\n{}\n{imap_host}\n{imap_port}\n{imap_transport}\n{imap_username}\n{smtp_host}\n{smtp_port}\n{smtp_transport}\n{smtp_username}",
        actor.tenant_scope().tenant_id().as_str(),
        actor.actor_id().as_str(),
        onboarding_id.as_str(),
        expected_version,
    );
    evidence(
        actor,
        request_idempotency_key(request)?,
        payload_fingerprint(material.as_bytes())?,
    )
}

pub fn from_oauth_callback(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    state: &str,
) -> Result<CommandExecutionEvidence> {
    oauth_callback_evidence(actor, onboarding_id, state, "gmail-oauth-callback")
}

pub fn from_standards_oauth_callback(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    state: &str,
) -> Result<CommandExecutionEvidence> {
    oauth_callback_evidence(
        actor,
        onboarding_id,
        state,
        "microsoft-standards-oauth-callback",
    )
}

fn oauth_callback_evidence(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    state: &str,
    namespace: &str,
) -> Result<CommandExecutionEvidence> {
    let material = format!(
        "{namespace}\n{}\n{}\n{}\n{}",
        actor.tenant_scope().tenant_id().as_str(),
        actor.actor_id().as_str(),
        onboarding_id.as_str(),
        state,
    );
    let digest = hex_digest(material.as_bytes());
    let idempotency_key = IdempotencyKey::parse(format!("oauthcb_{digest}"))
        .map_err(|error| Error::RustError(error.to_string()))?;
    evidence(actor, idempotency_key, payload_fingerprint(material.as_bytes())?)
}

fn request_idempotency_key(request: &Request) -> Result<IdempotencyKey> {
    let key = request
        .headers()
        .get(IDEMPOTENCY_HEADER)?
        .ok_or_else(|| Error::RustError("idempotency key missing".to_owned()))?;
    IdempotencyKey::parse(key).map_err(|error| Error::RustError(error.to_string()))
}

fn fingerprint_typed_request<T: Serialize>(request: &Request, payload: &T) -> Result<PayloadFingerprint> {
    let payload_bytes = serde_json::to_vec(payload)
        .map_err(|error| Error::RustError(format!("typed command serialization failed: {error}")))?;
    let method = request.method().as_ref().as_bytes();
    let path = request.path();
    let mut material = Vec::with_capacity(
        FINGERPRINT_DOMAIN.len() + method.len() + path.len() + payload_bytes.len() + 32,
    );
    append_field(&mut material, FINGERPRINT_DOMAIN)?;
    append_field(&mut material, method)?;
    append_field(&mut material, path.as_bytes())?;
    append_field(&mut material, &payload_bytes)?;
    payload_fingerprint(&material)
}

fn append_field(material: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = u64::try_from(value.len())
        .map_err(|_| Error::RustError("payload fingerprint field length overflow".to_owned()))?;
    material.extend_from_slice(&length.to_be_bytes());
    material.extend_from_slice(value);
    Ok(())
}

fn payload_fingerprint(material: &[u8]) -> Result<PayloadFingerprint> {
    PayloadFingerprint::parse(hex_digest(material))
        .map_err(|error| Error::RustError(error.to_string()))
}

fn hex_digest(material: &[u8]) -> String {
    let digest_bytes = Sha256::digest(material);
    let mut digest = String::with_capacity(digest_bytes.len() * 2);
    for byte in digest_bytes {
        digest.push(HEX[usize::from(byte >> 4)] as char);
        digest.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    digest
}

fn evidence(
    actor: &ActorContext,
    idempotency_key: IdempotencyKey,
    payload_fingerprint: PayloadFingerprint,
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
        payload_fingerprint,
        audit_event_id,
        outbox_event_id,
        UnixMillis::new(now),
        UnixMillis::new(expires_at),
    ))
}

#[cfg(test)]
mod tests {
    use super::{hex_digest, payload_fingerprint};

    #[test]
    fn fingerprint_hash_is_stable_and_strongly_typed() -> Result<(), Box<dyn std::error::Error>> {
        let fingerprint = payload_fingerprint(b"stable")?;
        assert_eq!(fingerprint.as_str(), hex_digest(b"stable"));
        assert_eq!(fingerprint.as_str().len(), 64);
        Ok(())
    }

    #[test]
    fn callback_namespaces_do_not_collide() -> Result<(), Box<dyn std::error::Error>> {
        let gmail = payload_fingerprint(b"gmail-oauth-callback")?;
        let standards = payload_fingerprint(b"microsoft-standards-oauth-callback")?;
        assert_ne!(gmail, standards);
        Ok(())
    }
}
