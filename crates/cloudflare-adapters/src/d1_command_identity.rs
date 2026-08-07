use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};
use sha2::{Digest, Sha256};
use worker::{Error, Result};

const COMMAND_DOMAIN: &[u8] = b"part-crm:d1-command-journal:v1";

pub(crate) fn command_journal_id(
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<String> {
    let mut hasher = Sha256::new();
    append_field(&mut hasher, COMMAND_DOMAIN)?;
    append_field(&mut hasher, tenant_id.as_str().as_bytes())?;
    append_field(&mut hasher, actor_id.as_str().as_bytes())?;
    append_field(&mut hasher, idempotency_key.as_str().as_bytes())?;
    let digest = hasher.finalize();
    Ok(format!("command_{}", lowercase_hex(&digest)))
}

fn append_field(hasher: &mut Sha256, value: &[u8]) -> Result<()> {
    let length = u64::try_from(value.len())
        .map_err(|_| Error::RustError("command identity field length overflow".to_owned()))?;
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

#[cfg(test)]
mod tests {
    use super::command_journal_id;
    use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};

    #[test]
    fn command_ids_are_actor_bound_and_include_the_full_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JCOMMAND")?;
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
        assert!(actor_a_key_a.len() <= 96);
        Ok(())
    }
}
