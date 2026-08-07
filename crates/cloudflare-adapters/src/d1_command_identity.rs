use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};
use worker::Result;

const COMMAND_DOMAIN: &str = "part-crm:d1-command-journal:v1";

pub(crate) fn command_journal_id(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<String> {
    Ok(canonical_component(
        COMMAND_DOMAIN,
        tenant_id.as_str(),
        actor_id.as_str(),
        idempotency_key.as_str(),
    ))
}

fn canonical_component(domain: &str, tenant_id: &str, actor_id: &str, key: &str) -> String {
    format!(
        "{domain}:{}:{tenant_id}:{}:{actor_id}:{}:{key}",
        tenant_id.len(),
        actor_id.len(),
        key.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::command_journal_id;
    use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};

    #[test]
    fn command_ids_are_actor_bound_and_include_the_full_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JCOMMAND")?;
        let other_tenant = TenantId::parse("tenant_01JCOMMAND_OTHER")?;
        let actor_a = ActorId::parse("actor_01JCOMMAND_A")?;
        let actor_b = ActorId::parse("actor_01JCOMMAND_B")?;
        let shared_prefix = "x".repeat(90);
        let key_a = IdempotencyKey::parse(format!("{shared_prefix}AAAAAA"))?;
        let key_b = IdempotencyKey::parse(format!("{shared_prefix}BBBBBB"))?;

        let actor_a_key_a = command_journal_id(&tenant, &actor_a, &key_a)?;
        assert_eq!(
            actor_a_key_a,
            command_journal_id(&tenant, &actor_a, &key_a)?
        );
        assert_ne!(
            actor_a_key_a,
            command_journal_id(&tenant, &actor_b, &key_a)?
        );
        assert_ne!(
            actor_a_key_a,
            command_journal_id(&tenant, &actor_a, &key_b)?
        );
        assert_ne!(
            actor_a_key_a,
            command_journal_id(&other_tenant, &actor_a, &key_a)?
        );
        assert!(actor_a_key_a.ends_with(key_a.as_str()));
        Ok(())
    }
}
