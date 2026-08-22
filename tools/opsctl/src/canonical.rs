use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Formatter;
use std::io::{Read, Result as IoResult};

pub const DEFAULT_MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_JSON_DEPTH: usize = 64;
const MAX_JCS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[must_use]
pub fn sha256_hex(input: &[u8]) -> String {
    hex(&Sha256::digest(input))
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
    serde_json_canonicalizer::to_string(value)
        .map_err(|error| format!("RFC 8785 canonicalization failed: {error}"))
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
    fn canonical_json_uses_rfc8785_number_and_key_rules() -> Result<(), String> {
        let value = json!({
            "numbers": [333_333_333.333_333_3_f64, 1e30_f64, 4.50_f64, 2e-3_f64, 1e-27_f64],
            "literals": [null, true, false]
        });
        assert_eq!(
            canonical_json(&value)?,
            r#"{"literals":[null,true,false],"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27]}"#
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
