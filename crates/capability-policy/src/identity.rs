use crate::{CapabilityProfileDefinition, PolicyError, ProfileId, profile_definition};
use std::fmt::{Display, Formatter, Write};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProfileDigest([u8; 32]);

impl ProfileDigest {
    pub fn parse_hex(value: &str) -> Result<Self, PolicyError> {
        if value.len() != 64 {
            return Err(PolicyError::InvalidDigest);
        }
        let bytes = value.as_bytes();
        let mut digest = [0_u8; 32];
        let mut index = 0_usize;
        while index < digest.len() {
            let high = hex_nibble(bytes[index * 2])?;
            let low = hex_nibble(bytes[index * 2 + 1])?;
            digest[index] = (high << 4) | low;
            index += 1;
        }
        Ok(Self(digest))
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            let _ = write!(&mut output, "{byte:02x}");
        }
        output
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl Display for ProfileDigest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

const fn hex_nibble(value: u8) -> Result<u8, PolicyError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(PolicyError::InvalidDigest),
    }
}

#[must_use]
pub fn profile_digest(profile_id: ProfileId) -> ProfileDigest {
    semantic_digest_v1(profile_definition(profile_id))
}

pub(crate) fn semantic_digest_v1(definition: CapabilityProfileDefinition) -> ProfileDigest {
    ProfileDigest(sha256(&semantic_identity_v1_bytes(definition)))
}

pub(crate) fn semantic_identity_v1_bytes(definition: CapabilityProfileDefinition) -> Vec<u8> {
    let mut output = String::new();
    output.push_str("{\"allowed_environments\":[");
    push_string_list(
        &mut output,
        definition
            .allowed_environments
            .iter()
            .map(|environment| environment.id()),
    );
    output.push_str("],\"disabled_activation_units\":[");
    push_string_list(
        &mut output,
        definition
            .disabled_activation_units
            .iter()
            .map(|unit| unit.id()),
    );
    output.push_str("],\"enabled_activation_units\":[");
    push_string_list(
        &mut output,
        definition
            .enabled_activation_units
            .iter()
            .map(|unit| unit.id()),
    );
    output.push(']');
    if let Some(parent) = definition.extends {
        output.push_str(",\"extends\":\"");
        output.push_str(parent.id());
        output.push('"');
    }
    output.push_str(",\"profile_id\":\"");
    output.push_str(definition.id.id());
    output.push_str("\",\"profile_version\":");
    let _ = write!(&mut output, "{}", definition.version);
    output.push('}');
    output.into_bytes()
}

fn push_string_list<'a>(output: &mut String, values: impl Iterator<Item = &'a str>) {
    let mut first = true;
    for value in values {
        if !first {
            output.push(',');
        }
        first = false;
        output.push('"');
        output.push_str(value);
        output.push('"');
    }
}

#[allow(clippy::many_single_char_names)]
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0_u32; 64];
        for (index, word) in chunk.chunks_exact(4).take(16).enumerate() {
            schedule[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
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
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
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

    let mut output = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{ProfileDigest, semantic_digest_v1, semantic_identity_v1_bytes, sha256};
    use crate::{ALL_PROFILE_IDS, ProfileId, profile_definition};

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            ProfileDigest(sha256(b"")).to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            ProfileDigest(sha256(b"abc")).to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn historical_v1_profile_digests_remain_byte_exact() {
        let expected = [
            (
                ProfileId::ProductionCoreV1,
                "92ccb88e7b74c89e4f39a5349eb5bf0da6a2d6f9ccc4a89d72ab462cb08e0868",
            ),
            (
                ProfileId::RehearsalCoreV1,
                "40ebe3bc1d890757f00433d0ff814720be1ffcd691fff35aea5244a05fc1f45a",
            ),
            (
                ProfileId::ProductionMailboxAdminV1,
                "ede6abdcdeb98738855e7fc2309788625ecbce03531b71644413ba69dceaf939",
            ),
            (
                ProfileId::ProductionMailboxJobsV1,
                "a95ad429c73bc3415d8991ec15d390c4be94daec88a7700f473e2325fa3470ef",
            ),
            (
                ProfileId::ProductionOutboundMailV1,
                "da2a883eba9fa706d6e9adfe87265d3b56a842cd7636877874dce3e86d3bf014",
            ),
        ];
        for (profile_id, expected_digest) in expected {
            assert_eq!(
                semantic_digest_v1(profile_definition(profile_id)).to_hex(),
                expected_digest
            );
        }
        assert_eq!(expected.len(), 5);
    }

    #[test]
    fn core_v2_profile_digests_are_frozen_separately() {
        let expected = [
            (
                ProfileId::ProductionCoreV2,
                "41288578863d5e2e9a96b8ea609bb55154f261b3e8b13c9420e3188fdb317830",
            ),
            (
                ProfileId::RehearsalCoreV2,
                "22be80b5718a3cb80f35c474d3f52421a98ffeb663cc8701c1acd6f2c47759e2",
            ),
        ];
        for (profile_id, expected_digest) in expected {
            assert_eq!(
                semantic_digest_v1(profile_definition(profile_id)).to_hex(),
                expected_digest
            );
        }
        assert_eq!(ALL_PROFILE_IDS.len(), 7);
    }

    #[test]
    fn v1_bytes_match_the_historical_stable_json_scope() {
        let bytes = semantic_identity_v1_bytes(profile_definition(ProfileId::ProductionCoreV1));
        let text = String::from_utf8(bytes);
        assert!(text.is_ok());
        if let Ok(text) = text {
            assert!(text.starts_with("{\"allowed_environments\":[\"production\"]"));
            assert!(text.ends_with("\"profile_version\":1}"));
            assert!(!text.contains("activation_gate"));
        }
    }

    #[test]
    fn digest_parser_round_trips() {
        for profile_id in ALL_PROFILE_IDS {
            let digest = semantic_digest_v1(profile_definition(profile_id));
            assert_eq!(ProfileDigest::parse_hex(&digest.to_hex()), Ok(digest));
        }
    }
}
