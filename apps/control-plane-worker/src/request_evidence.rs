use profile_platform_primitives::{
    ActorId, AuditEventId, IdempotencyKey, OutboxEventId, TenantId,
};
use sha2::{Digest, Sha256};
use worker::{Error, Result};

const AUDIT_DOMAIN: &[u8] = b"part-crm:audit-event:v1";
const OUTBOX_DOMAIN: &[u8] = b"part-crm:outbox-event:v1";

pub fn audit_event_id(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<AuditEventId> {
    AuditEventId::parse(derived_id(
        "audit",
        AUDIT_DOMAIN,
        tenant_id,
        actor_id,
        idempotency_key,
    ))
    .map_err(identifier_error)
}

pub fn outbox_event_id(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<OutboxEventId> {
    OutboxEventId::parse(derived_id(
        "outbox",
        OUTBOX_DOMAIN,
        tenant_id,
        actor_id,
        idempotency_key,
    ))
    .map_err(identifier_error)
}

fn derived_id(
    prefix: &str,
    domain: &[u8],
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    update_field(&mut digest, tenant_id.as_str().as_bytes());
    update_field(&mut digest, actor_id.as_str().as_bytes());
    update_field(&mut digest, idempotency_key.as_str().as_bytes());
    format!("{prefix}_{:x}", digest.finalize())
}

fn update_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{audit_event_id, outbox_event_id};
    use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};

    #[test]
    fn evidence_ids_are_stable_bounded_and_domain_separated()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JEVIDENCE")?;
        let actor = ActorId::parse("actor_01JEVIDENCE")?;
        let key = IdempotencyKey::parse("idempotency_01JEVIDENCE")?;
        let audit = audit_event_id(&tenant, &actor, &key)?;
        let outbox = outbox_event_id(&tenant, &actor, &key)?;
        assert_eq!(audit, audit_event_id(&tenant, &actor, &key)?);
        assert_eq!(outbox, outbox_event_id(&tenant, &actor, &key)?);
        assert_ne!(audit.as_str(), outbox.as_str());
        assert!(audit.as_str().len() <= 96);
        assert!(outbox.as_str().len() <= 96);
        Ok(())
    }

    #[test]
    fn actors_and_long_key_suffixes_cannot_collide()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JEVIDENCE")?;
        let actor_a = ActorId::parse("actor_01JEVIDENCE_A")?;
        let actor_b = ActorId::parse("actor_01JEVIDENCE_B")?;
        let shared_prefix = "x".repeat(90);
        let key_a = IdempotencyKey::parse(format!("{shared_prefix}AAAAAA"))?;
        let key_b = IdempotencyKey::parse(format!("{shared_prefix}BBBBBB"))?;
        assert_ne!(
            audit_event_id(&tenant, &actor_a, &key_a)?,
            audit_event_id(&tenant, &actor_b, &key_a)?
        );
        assert_ne!(
            audit_event_id(&tenant, &actor_a, &key_a)?,
            audit_event_id(&tenant, &actor_a, &key_b)?
        );
        Ok(())
    }
}
