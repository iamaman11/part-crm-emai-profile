use crate::request_evidence::{audit_event_id, outbox_event_id};
use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_scheduling::MailboxJobDispatch;
use profile_platform_primitives::{
    ActorContext, CorrelationId, IdempotencyKey, TenantScope, UnixMillis,
};
use sha2::{Digest, Sha256};
use worker::{Error, Result};

const MAILBOX_QUEUE_DOMAIN: &[u8] = b"part-crm:mailbox-queue-execution:v1";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;

pub fn actor_and_evidence(
    dispatch: &MailboxJobDispatch,
    now: UnixMillis,
) -> Result<(ActorContext, CommandExecutionEvidence)> {
    let digest = execution_digest(dispatch)?;
    let idempotency_key =
        IdempotencyKey::parse(format!("mailboxq_{digest}")).map_err(identifier_error)?;
    let correlation_id =
        CorrelationId::parse(format!("corr_{digest}")).map_err(identifier_error)?;
    let actor = ActorContext::new(
        TenantScope::new(dispatch.tenant_id().clone()),
        dispatch.actor_id().clone(),
        correlation_id,
    );
    let audit_event_id =
        audit_event_id(dispatch.tenant_id(), dispatch.actor_id(), &idempotency_key)?;
    let outbox_event_id =
        outbox_event_id(dispatch.tenant_id(), dispatch.actor_id(), &idempotency_key)?;
    let expires_at = now
        .value()
        .checked_add(IDEMPOTENCY_TTL_MS)
        .ok_or_else(|| Error::RustError("mailbox queue idempotency expiry overflow".into()))?;
    Ok((
        actor,
        CommandExecutionEvidence::new(
            idempotency_key,
            digest,
            audit_event_id,
            outbox_event_id,
            now,
            UnixMillis::new(expires_at),
        ),
    ))
}

fn execution_digest(dispatch: &MailboxJobDispatch) -> Result<String> {
    let mut hasher = Sha256::new();
    append_field(&mut hasher, MAILBOX_QUEUE_DOMAIN)?;
    append_field(&mut hasher, dispatch.tenant_id().as_str().as_bytes())?;
    append_field(&mut hasher, dispatch.actor_id().as_str().as_bytes())?;
    append_field(&mut hasher, dispatch.binding_id().as_str().as_bytes())?;
    append_field(&mut hasher, dispatch.job_id().as_str().as_bytes())?;
    append_field(
        &mut hasher,
        &dispatch.expected_version().value().to_be_bytes(),
    )?;
    Ok(lowercase_hex(&hasher.finalize()))
}

fn append_field(hasher: &mut Sha256, value: &[u8]) -> Result<()> {
    let length = u64::try_from(value.len())
        .map_err(|_| Error::RustError("mailbox queue evidence field overflow".into()))?;
    hasher.update(length.to_be_bytes());
    hasher.update(value);
    Ok(())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{actor_and_evidence, execution_digest};
    use application_ports::mailbox_scheduling::MailboxJobDispatch;
    use profile_platform_primitives::{
        ActorId, AggregateVersion, MailboxBindingId, MailboxJobId, TenantId, UnixMillis,
    };

    fn dispatch(version: u64) -> Result<MailboxJobDispatch, Box<dyn std::error::Error>> {
        Ok(MailboxJobDispatch::new(
            TenantId::parse("tenant_01JQEVIDENCE")?,
            ActorId::parse("actor_01JQEVIDENCE")?,
            MailboxBindingId::parse("mailbox_01JQEVIDENCE")?,
            MailboxJobId::parse("mailjob_01JQEVIDENCE")?,
            AggregateVersion::new(version)?,
            UnixMillis::new(100),
        ))
    }

    #[test]
    fn duplicate_delivery_reuses_exact_execution_identity() -> Result<(), Box<dyn std::error::Error>>
    {
        let version_four = dispatch(4)?;
        let first = actor_and_evidence(&version_four, UnixMillis::new(100))?;
        let second = actor_and_evidence(&version_four, UnixMillis::new(200))?;
        assert_eq!(first.0, second.0);
        assert_eq!(first.1.idempotency_key(), second.1.idempotency_key());
        assert_eq!(first.1.request_digest(), second.1.request_digest());
        assert_ne!(
            execution_digest(&version_four)?,
            execution_digest(&dispatch(5)?)?
        );
        Ok(())
    }
}
