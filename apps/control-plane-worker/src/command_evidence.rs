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
    from_request(request, actor, hex_digest(material.as_bytes()))
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
    let digest = oauth_callback_digest(actor, onboarding_id, state, namespace);
    let idempotency_key = IdempotencyKey::parse(format!("oauthcb_{digest}"))
        .map_err(|error| Error::RustError(error.to_string()))?;
    evidence(actor, idempotency_key, digest)
}

fn oauth_callback_digest(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    state: &str,
    namespace: &str,
) -> String {
    let material = format!(
        "{namespace}\n{}\n{}\n{}\n{}",
        actor.tenant_scope().tenant_id().as_str(),
        actor.actor_id().as_str(),
        onboarding_id.as_str(),
        state,
    );
    hex_digest(material.as_bytes())
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

#[cfg(test)]
mod tests {
    use super::{hex_digest, oauth_callback_digest};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, MailboxOnboardingId, TenantId, TenantScope,
    };

    #[test]
    fn callback_namespaces_do_not_collide() -> Result<(), Box<dyn std::error::Error>> {
        let actor = ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_C3_evidence")?),
            ActorId::parse("actor_C3_evidence")?,
            CorrelationId::parse("corr_C3_evidence")?,
        );
        let onboarding_id = MailboxOnboardingId::parse("onboarding_C3_evidence")?;
        let gmail = oauth_callback_digest(
            &actor,
            &onboarding_id,
            "opaque-state",
            "gmail-oauth-callback",
        );
        let standards = oauth_callback_digest(
            &actor,
            &onboarding_id,
            "opaque-state",
            "microsoft-standards-oauth-callback",
        );
        assert_ne!(gmail, standards);
        assert_eq!(gmail.len(), 64);
        assert_eq!(standards.len(), 64);
        assert_eq!(hex_digest(b"stable").len(), 64);
        Ok(())
    }
}
