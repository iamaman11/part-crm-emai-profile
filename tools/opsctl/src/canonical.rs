use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::collections::BTreeSet;
use std::fmt::Formatter;
use std::io::{Read, Result as IoResult};

pub const DEFAULT_MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_JSON_DEPTH: usize = 64;
const MAX_JCS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

const INITIAL_STATE: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Debug, Clone)]
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_len = self.total_len.wrapping_add(input.len() as u64);
        if self.buffer_len != 0 {
            let needed = 64 - self.buffer_len;
            let take = needed.min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                compress(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while input.len() >= 64 {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&input[..64]);
            compress(&mut self.state, &block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            compress(&mut self.state, &block);
            self.buffer = [0; 64];
            self.buffer_len = 0;
        }
        self.buffer[self.buffer_len..56].fill(0);
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        compress(&mut self.state, &block);

        let mut output = [0_u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        output
    }
}

fn compress(state: &mut [u32; 8], chunk: &[u8; 64]) {
    let mut schedule = [0_u32; 64];
    for (index, word) in schedule.iter_mut().take(16).enumerate() {
        let offset = index * 4;
        *word = u32::from_be_bytes([
            chunk[offset],
            chunk[offset + 1],
            chunk[offset + 2],
            chunk[offset + 3],
        ]);
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
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(K[index]);
        let temp1 = temp1.wrapping_add(schedule[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);

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

#[must_use]
pub fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex(&hasher.finalize())
}

pub fn sha256_reader_hex(reader: &mut impl Read) -> IoResult<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn parse_strict_json(input: &str) -> Result<Value, String> {
    parse_strict_json_with_limits(input, DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_JSON_DEPTH)
}

pub fn parse_strict_json_with_limits(
    input: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<Value, String> {
    if input.len() > max_bytes {
        return Err(format!(
            "JSON input exceeds byte budget: observed={} max={max_bytes}",
            input.len()
        ));
    }
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = StrictValueSeed {
        depth: 0,
        max_depth,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| format!("invalid strict JSON: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("invalid trailing JSON data: {error}"))?;
    Ok(value)
}

#[derive(Clone, Copy)]
struct StrictValueSeed {
    depth: usize,
    max_depth: usize,
}

impl StrictValueSeed {
    fn child<E: de::Error>(self) -> Result<Self, E> {
        if self.depth >= self.max_depth {
            return Err(E::custom(format!(
                "JSON nesting exceeds depth budget {}",
                self.max_depth
            )));
        }
        Ok(Self {
            depth: self.depth + 1,
            max_depth: self.max_depth,
        })
    }
}

impl<'de> DeserializeSeed<'de> for StrictValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor { seed: self })
    }
}

struct StrictValueVisitor {
    seed: StrictValueSeed,
}

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("one bounded JSON value with unique object member names")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let max = MAX_JCS_SAFE_INTEGER as i64;
        if value < -max || value > max {
            return Err(E::custom("integer exceeds RFC 8785/I-JSON safe range"));
        }
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value > MAX_JCS_SAFE_INTEGER {
            return Err(E::custom("integer exceeds RFC 8785/I-JSON safe range"));
        }
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value.is_finite() && value.fract() == 0.0 && value.abs() > MAX_JCS_SAFE_INTEGER as f64 {
            return Err(E::custom(
                "integer-valued number exceeds RFC 8785/I-JSON safe range",
            ));
        }
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number is forbidden"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let child = self.seed.child::<A::Error>()?;
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(child)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let child = self.seed.child::<A::Error>()?;
        let mut keys = BTreeSet::new();
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom(format!(
                    "duplicate JSON object member: {key}"
                )));
            }
            let value = object.next_value_seed(child)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub fn canonical_json(value: &Value) -> Result<String, String> {
    let mut output = String::new();
    write_canonical(value, &mut output)?;
    Ok(output)
}

pub fn canonical_pretty_json(value: &Value) -> Result<String, String> {
    let compact = canonical_json(value)?;
    let canonical: Value = serde_json::from_str(&compact)
        .map_err(|error| format!("cannot parse canonical JSON for pretty projection: {error}"))?;
    let mut output = serde_json::to_string_pretty(&canonical)
        .map_err(|error| format!("cannot render canonical pretty JSON: {error}"))?;
    output.push('\n');
    Ok(output)
}

fn write_canonical(value: &Value, output: &mut String) -> Result<(), String> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&value.to_string()),
        Value::String(value) => {
            output.push_str(
                &serde_json::to_string(value)
                    .map_err(|error| format!("cannot canonicalize string: {error}"))?,
            );
        }
        Value::Array(values) => {
            output.push('[');
            for (index, child) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical(child, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for (index, key) in keys.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key)
                        .map_err(|error| format!("cannot canonicalize key: {error}"))?,
                );
                output.push(':');
                let child = values
                    .get(*key)
                    .ok_or_else(|| "canonical JSON key disappeared".to_owned())?;
                write_canonical(child, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_json, canonical_pretty_json, parse_strict_json, parse_strict_json_with_limits,
        sha256_hex, sha256_reader_hex,
    };
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn streaming_sha256_matches_one_shot_across_block_boundaries() -> std::io::Result<()> {
        let bytes = vec![0x5a; 64 * 1024 + 137];
        let expected = sha256_hex(&bytes);
        let mut reader = Cursor::new(bytes);
        assert_eq!(sha256_reader_hex(&mut reader)?, expected);
        Ok(())
    }

    #[test]
    fn canonical_json_sorts_object_keys_recursively() -> Result<(), String> {
        let value = json!({"z": 1, "a": {"d": 4, "b": 2}, "m": [3, {"y": 2, "x": 1}]});
        assert_eq!(
            canonical_json(&value)?,
            r#"{"a":{"b":2,"d":4},"m":[3,{"x":1,"y":2}],"z":1}"#
        );
        Ok(())
    }

    #[test]
    fn strict_json_rejects_duplicate_members_at_any_depth() {
        assert!(parse_strict_json(r#"{"a":1,"a":2}"#).is_err());
        assert!(parse_strict_json(r#"{"outer":{"a":1,"a":2}}"#).is_err());
    }

    #[test]
    fn strict_json_rejects_unsafe_integer_numbers() {
        assert!(parse_strict_json(r#"{"n":9007199254740991}"#).is_ok());
        assert!(parse_strict_json(r#"{"n":-9007199254740991}"#).is_ok());
        assert!(parse_strict_json(r#"{"n":9007199254740992}"#).is_err());
        assert!(parse_strict_json(r#"{"n":-9007199254740992}"#).is_err());
        assert!(parse_strict_json(r#"{"n":9007199254740992.0}"#).is_err());
    }

    #[test]
    fn strict_json_enforces_byte_and_depth_budgets() {
        assert!(parse_strict_json_with_limits(r#"{"a":1}"#, 4, 64).is_err());
        assert!(parse_strict_json_with_limits(r#"[[[0]]]"#, 1024, 2).is_err());
        assert!(parse_strict_json_with_limits(r#"[[0]]"#, 1024, 2).is_ok());
    }

    #[test]
    fn pretty_projection_reuses_canonical_key_order() -> Result<(), String> {
        let value = json!({"z": 1, "a": {"d": 4, "b": 2}});
        assert_eq!(
            canonical_pretty_json(&value)?,
            "{\n  \"a\": {\n    \"b\": 2,\n    \"d\": 4\n  },\n  \"z\": 1\n}\n"
        );
        Ok(())
    }
}
