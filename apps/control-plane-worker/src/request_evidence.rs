use profile_platform_primitives::{
    ActorId, AuditEventId, IdempotencyKey, OutboxEventId, TenantId,
};
use worker::{Error, Result};

const EVIDENCE_DOMAIN: &[u8] = b"part-crm:evidence-id:v1";
const AUDIT_DOMAIN: &[u8] = b"audit";
const OUTBOX_DOMAIN: &[u8] = b"outbox";
const SHA256_BLOCK_BYTES: usize = 64;
const SHA256_LENGTH_BYTES: usize = 8;

const SHA256_INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

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
    )?)
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
    )?)
    .map_err(identifier_error)
}

fn derived_id(
    prefix: &str,
    domain: &[u8],
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<String> {
    let material = canonical_material(domain, tenant_id, actor_id, idempotency_key)?;
    Ok(format!("{prefix}_{}", lowercase_hex(&sha256(&material)?)))
}

fn canonical_material(
    domain: &[u8],
    tenant_id: &TenantId,
    actor_id: &ActorId,
    idempotency_key: &IdempotencyKey,
) -> Result<Vec<u8>> {
    let mut material = Vec::with_capacity(
        EVIDENCE_DOMAIN.len()
            + domain.len()
            + tenant_id.as_str().len()
            + actor_id.as_str().len()
            + idempotency_key.as_str().len()
            + (5 * SHA256_LENGTH_BYTES),
    );
    append_field(&mut material, EVIDENCE_DOMAIN)?;
    append_field(&mut material, domain)?;
    append_field(&mut material, tenant_id.as_str().as_bytes())?;
    append_field(&mut material, actor_id.as_str().as_bytes())?;
    append_field(&mut material, idempotency_key.as_str().as_bytes())?;
    Ok(material)
}

fn append_field(material: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = u64::try_from(value.len())
        .map_err(|_| Error::RustError("evidence field length overflow".to_owned()))?;
    material.extend_from_slice(&length.to_be_bytes());
    material.extend_from_slice(value);
    Ok(())
}

fn sha256(input: &[u8]) -> Result<[u8; 32]> {
    let byte_length = u64::try_from(input.len())
        .map_err(|_| Error::RustError("evidence material length overflow".to_owned()))?;
    let bit_length = byte_length
        .checked_mul(8)
        .ok_or_else(|| Error::RustError("evidence bit length overflow".to_owned()))?;

    let mut padded = input.to_vec();
    padded.push(0x80);
    while (padded.len() + SHA256_LENGTH_BYTES) % SHA256_BLOCK_BYTES != 0 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut state = SHA256_INITIAL_STATE;
    for block in padded.chunks_exact(SHA256_BLOCK_BYTES) {
        let mut schedule = [0_u32; 64];
        for (index, word) in block.chunks_exact(4).take(16).enumerate() {
            let [first, second, third, fourth] = word else {
                return Err(Error::RustError("invalid SHA-256 word".to_owned()));
            };
            schedule[index] = u32::from_be_bytes([*first, *second, *third, *fourth]);
        }
        for index in 16..64 {
            let sigma_zero = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let sigma_one = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(sigma_zero)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(sigma_one);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for index in 0..64 {
            let sum_one = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temporary_one = h
                .wrapping_add(sum_one)
                .wrapping_add(choice)
                .wrapping_add(SHA256_ROUND_CONSTANTS[index])
                .wrapping_add(schedule[index]);
            let sum_zero = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary_two = sum_zero.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary_one);
            d = c;
            c = b;
            b = a;
            a = temporary_one.wrapping_add(temporary_two);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut digest = [0_u8; 32];
    for (output, value) in digest.chunks_exact_mut(4).zip(state) {
        output.copy_from_slice(&value.to_be_bytes());
    }
    Ok(digest)
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
    use super::{audit_event_id, lowercase_hex, outbox_event_id, sha256};
    use profile_platform_primitives::{ActorId, IdempotencyKey, TenantId};

    #[test]
    fn sha256_matches_standard_vectors() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            lowercase_hex(&sha256(b"")?),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            lowercase_hex(&sha256(b"abc")?),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }

    #[test]
    fn evidence_ids_are_stable_bounded_and_domain_separated()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JEVIDENCE")?;
        let actor = ActorId::parse("actor_01JEVIDENCE")?;
        let key = IdempotencyKey::parse("idempotency_01JEVIDENCE")?;
        let audit = audit_event_id(&tenant, &actor, &key)?;
        let outbox = outbox_event_id(&tenant, &actor, &key)?;
        assert_eq!(
            audit.as_str(),
            "audit_321c7114771f4f170aaa54fae3062c7244932f1315e639eed743bc3f87a6fbd2"
        );
        assert_eq!(
            outbox.as_str(),
            "outbox_d32b31bc248b7df577b3a42604291461baff2ff43da595568022167b45ae0dab"
        );
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
