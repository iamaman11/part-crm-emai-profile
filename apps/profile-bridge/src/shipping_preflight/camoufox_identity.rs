use browser_execution_domain::browser_visible_observation::{
    BrowserKeyValueObservation, BrowserValue, BrowserVisibleSha256Port, SpeechVoiceObservation,
    canonical_audio_seed_sha256, canonical_canvas_seed_sha256, canonical_context_attributes_sha256,
    canonical_font_set_sha256, canonical_font_spacing_seed_sha256,
    canonical_navigator_languages_sha256, canonical_shader_precision_sha256,
    canonical_speech_voices_sha256, canonical_webgl2_parameters_sha256,
    canonical_webgl_extensions_sha256, canonical_webgl_parameters_sha256,
};
use browser_execution_domain::{
    BrowserIdentityManifest, BrowserOsIdentity, DisplayIdentity, FontIdentity, GraphicsIdentity,
    HardwareCapabilityIdentity, LocaleIdentity, OriginDeterminismMode,
    OriginDeterministicIdentity, ProfileStableIdentity,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const CONFIG_NAME: &str = "camoufox-config.json";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_JSON_DEPTH: usize = 32;
const PROFILE_STABLE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CamoufoxIdentityProjectionError {
    InvalidMaterialization,
    IdentityMismatch,
}

impl core::fmt::Display for CamoufoxIdentityProjectionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMaterialization => "materialized Camoufox identity projection is invalid",
            Self::IdentityMismatch => {
                "materialized Camoufox identity does not match generation identity"
            }
        })
    }
}

impl std::error::Error for CamoufoxIdentityProjectionError {}

/// Verify one materialized Camoufox config against the generation-owned browser identity.
///
/// The adapter owns only Camoufox JSON field mapping and filesystem effects. All semantic digest
/// canonicalization is delegated to `browser-execution-domain`; raw config SHA remains integrity
/// evidence and is never used as the Profile-Stable semantic identity.
pub fn verify_materialized_camoufox_identity(
    profile_root: &Path,
    expected: &BrowserIdentityManifest,
) -> Result<(), CamoufoxIdentityProjectionError> {
    let raw = read_regular_bounded(&profile_root.join(CONFIG_NAME))?;
    if sha256_hex(&raw) != expected.fingerprint_config_sha256() {
        return Err(CamoufoxIdentityProjectionError::IdentityMismatch);
    }
    let projected = profile_stable_identity_from_config_bytes(&raw)?;
    if &projected != expected.profile_stable_identity() {
        return Err(CamoufoxIdentityProjectionError::IdentityMismatch);
    }
    Ok(())
}

/// Parse a canonical Camoufox config projection into the typed domain identity.
///
/// This is public for exact-runtime acceptance tests; shipping calls
/// `verify_materialized_camoufox_identity`, so tests no longer own a parallel hash convention.
pub fn profile_stable_identity_from_config_bytes(
    raw: &[u8],
) -> Result<ProfileStableIdentity, CamoufoxIdentityProjectionError> {
    if raw.is_empty() || u64::try_from(raw.len()).ok().is_none_or(|len| len > MAX_CONFIG_BYTES) {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    let config: Value = serde_json::from_slice(raw)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    let object = config
        .as_object()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    let mut digester = Sha256Adapter;

    let user_agent = required_string(object, "navigator.userAgent")?;
    let webgl_extensions = required_string_list(object, "webGl:supportedExtensions")?;
    let webgl2_extensions = required_string_list(object, "webGl2:supportedExtensions")?;
    let webgl_parameters = required_key_values(object, "webGl:parameters")?;
    let webgl2_parameters = required_key_values(object, "webGl2:parameters")?;
    let webgl_shader = required_key_values(object, "webGl:shaderPrecisionFormats")?;
    let webgl2_shader = required_key_values(object, "webGl2:shaderPrecisionFormats")?;
    let webgl_context = required_key_values(object, "webGl:contextAttributes")?;
    let webgl2_context = required_key_values(object, "webGl2:contextAttributes")?;
    let fonts = required_string_list(object, "fonts")?;
    let languages = required_string_list(object, "navigator.languages")?;
    let speech_voices = optional_speech_voices(object)?;

    ProfileStableIdentity::new(
        PROFILE_STABLE_SCHEMA_VERSION,
        BrowserOsIdentity::new(
            user_agent.clone(),
            firefox_major(&user_agent)?,
            required_string(object, "navigator.platform")?,
            required_string(object, "navigator.oscpu")?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        HardwareCapabilityIdentity::new(
            required_u16(object, "navigator.hardwareConcurrency")?,
            optional_u16(object, "navigator.deviceMemory")?,
            required_u16(object, "navigator.maxTouchPoints")?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        DisplayIdentity::new(
            required_u32(object, "screen.width")?,
            required_u32(object, "screen.height")?,
            required_u32(object, "screen.availWidth")?,
            required_u32(object, "screen.availHeight")?,
            required_i32(object, "screen.availLeft")?,
            required_i32(object, "screen.availTop")?,
            required_u16(object, "screen.colorDepth")?,
            required_u16(object, "screen.pixelDepth")?,
            required_dpr_milli(object)?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        GraphicsIdentity::new(
            required_string(object, "webGl:vendor")?,
            required_string(object, "webGl:renderer")?,
            canonical_webgl_extensions_sha256(
                &webgl_extensions,
                &webgl2_extensions,
                &mut digester,
            )
            .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_webgl_parameters_sha256(&webgl_parameters, &mut digester)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_webgl2_parameters_sha256(&webgl2_parameters, &mut digester)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_shader_precision_sha256(&webgl_shader, &webgl2_shader, &mut digester)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_context_attributes_sha256(
                &webgl_context,
                &webgl2_context,
                &mut digester,
            )
            .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        FontIdentity::new(
            canonical_font_set_sha256(&fonts, &mut digester)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_font_spacing_seed_sha256(
                &required_browser_value(object, "fonts:spacing_seed")?,
                &mut digester,
            )
            .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        OriginDeterministicIdentity::new(
            OriginDeterminismMode::ProfileGenerationSeed,
            canonical_canvas_seed_sha256(
                &required_browser_value(object, "canvas:seed")?,
                &mut digester,
            )
            .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            canonical_audio_seed_sha256(
                &required_browser_value(object, "audio:seed")?,
                &mut digester,
            )
            .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
        LocaleIdentity::new(
            required_string(object, "navigator.language")?,
            canonical_navigator_languages_sha256(&languages, &mut digester)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?,
            speech_voices
                .as_deref()
                .map(|voices| {
                    canonical_speech_voices_sha256(voices, &mut digester)
                        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)
                })
                .transpose()?,
        )
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?,
    )
    .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

struct Sha256Adapter;

impl BrowserVisibleSha256Port for Sha256Adapter {
    type Error = core::convert::Infallible;

    fn sha256(&mut self, canonical_bytes: &[u8]) -> Result<[u8; 32], Self::Error> {
        Ok(Sha256::digest(canonical_bytes).into())
    }
}

fn read_regular_bounded(path: &Path) -> Result<Vec<u8>, CamoufoxIdentityProjectionError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    fs::read(path).map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, CamoufoxIdentityProjectionError> {
    object
        .get(key)
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<String, CamoufoxIdentityProjectionError> {
    required(object, key)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn required_bool(
    object: &Map<String, Value>,
    key: &str,
) -> Result<bool, CamoufoxIdentityProjectionError> {
    required(object, key)?
        .as_bool()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn integer(value: &Value) -> Result<i64, CamoufoxIdentityProjectionError> {
    if let Some(value) = value.as_i64() {
        return Ok(value);
    }
    if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
        return Ok(value);
    }
    let value = value
        .as_f64()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    if !value.is_finite()
        || value.fract() != 0.0
        || value < i64::MIN as f64
        || value > i64::MAX as f64
    {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    Ok(value as i64)
}

fn required_u16(
    object: &Map<String, Value>,
    key: &str,
) -> Result<u16, CamoufoxIdentityProjectionError> {
    u16::try_from(integer(required(object, key)?)?)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn optional_u16(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<u16>, CamoufoxIdentityProjectionError> {
    object
        .get(key)
        .map(|value| {
            u16::try_from(integer(value)?)
                .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
        })
        .transpose()
}

fn required_u32(
    object: &Map<String, Value>,
    key: &str,
) -> Result<u32, CamoufoxIdentityProjectionError> {
    u32::try_from(integer(required(object, key)?)?)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn required_i32(
    object: &Map<String, Value>,
    key: &str,
) -> Result<i32, CamoufoxIdentityProjectionError> {
    i32::try_from(integer(required(object, key)?)?)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn required_dpr_milli(
    object: &Map<String, Value>,
) -> Result<u32, CamoufoxIdentityProjectionError> {
    let value = required(object, "window.devicePixelRatio")?
        .as_f64()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    let milli = value * 1000.0;
    if !milli.is_finite() || milli <= 0.0 || (milli.round() - milli).abs() > f64::EPSILON {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    u32::try_from(milli.round() as u64)
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn firefox_major(user_agent: &str) -> Result<u16, CamoufoxIdentityProjectionError> {
    let major = user_agent
        .rsplit_once("Firefox/")
        .map(|(_, suffix)| suffix)
        .and_then(|suffix| suffix.split('.').next())
        .filter(|major| !major.is_empty() && major.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    major
        .parse()
        .map_err(|_| CamoufoxIdentityProjectionError::InvalidMaterialization)
}

fn required_string_list(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, CamoufoxIdentityProjectionError> {
    let values = required(object, key)?
        .as_array()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)
        })
        .collect()
}

fn required_key_values(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<BrowserKeyValueObservation>, CamoufoxIdentityProjectionError> {
    let values = required(object, key)?
        .as_object()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    map_key_values(values, 0)
}

fn required_browser_value(
    object: &Map<String, Value>,
    key: &str,
) -> Result<BrowserValue, CamoufoxIdentityProjectionError> {
    browser_value(required(object, key)?, 0)
}

fn map_key_values(
    object: &Map<String, Value>,
    depth: usize,
) -> Result<Vec<BrowserKeyValueObservation>, CamoufoxIdentityProjectionError> {
    if depth > MAX_JSON_DEPTH {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    object
        .iter()
        .map(|(name, value)| {
            Ok(BrowserKeyValueObservation {
                name: name.clone(),
                value: browser_value(value, depth + 1)?,
            })
        })
        .collect()
}

fn browser_value(
    value: &Value,
    depth: usize,
) -> Result<BrowserValue, CamoufoxIdentityProjectionError> {
    if depth > MAX_JSON_DEPTH {
        return Err(CamoufoxIdentityProjectionError::InvalidMaterialization);
    }
    match value {
        Value::Null => Ok(BrowserValue::Null),
        Value::Bool(value) => Ok(BrowserValue::Bool(*value)),
        Value::Number(value) => Ok(BrowserValue::Number(value.to_string())),
        Value::String(value) if !value.is_empty() => Ok(BrowserValue::Text(value.clone())),
        Value::String(_) => Err(CamoufoxIdentityProjectionError::InvalidMaterialization),
        Value::Array(values) => values
            .iter()
            .map(|value| browser_value(value, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(BrowserValue::List),
        Value::Object(values) => map_key_values(values, depth + 1).map(BrowserValue::Map),
    }
}

fn optional_speech_voices(
    object: &Map<String, Value>,
) -> Result<Option<Vec<SpeechVoiceObservation>>, CamoufoxIdentityProjectionError> {
    let Some(value) = object.get("voices") else {
        return Ok(None);
    };
    let voices = value
        .as_array()
        .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
    voices
        .iter()
        .map(|voice| {
            let voice = voice
                .as_object()
                .ok_or(CamoufoxIdentityProjectionError::InvalidMaterialization)?;
            // Pinned Camoufox exposes voiceURI as part of every voice record. The current domain
            // voice digest does not yet carry it; requiring it here prevents schema ambiguity while
            // the next bounded R10 domain-version slice makes it identity-bearing.
            let _voice_uri = required_string(voice, "voiceURI")?;
            Ok(SpeechVoiceObservation {
                language: required_string(voice, "lang")?,
                name: required_string(voice, "name")?,
                local_service: required_bool(voice, "isLocalService")?,
                is_default: required_bool(voice, "isDefault")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::{
        CamoufoxIdentityProjectionError, profile_stable_identity_from_config_bytes,
    };

    fn config(number: &str) -> String {
        format!(
            concat!(
                "{{",
                "\"navigator.userAgent\":\"Mozilla/5.0 Firefox/152.0\",",
                "\"navigator.platform\":\"Win32\",",
                "\"navigator.oscpu\":\"Windows NT 10.0; Win64; x64\",",
                "\"navigator.hardwareConcurrency\":8,",
                "\"navigator.deviceMemory\":8,",
                "\"navigator.maxTouchPoints\":0,",
                "\"screen.width\":1920,\"screen.height\":1080,",
                "\"screen.availWidth\":1920,\"screen.availHeight\":1040,",
                "\"screen.availLeft\":0,\"screen.availTop\":0,",
                "\"screen.colorDepth\":24,\"screen.pixelDepth\":24,",
                "\"window.devicePixelRatio\":1,",
                "\"webGl:vendor\":\"Vendor\",\"webGl:renderer\":\"Renderer\",",
                "\"webGl:supportedExtensions\":[\"EXT_b\",\"EXT_a\"],",
                "\"webGl2:supportedExtensions\":[\"EXT_c\"],",
                "\"webGl:parameters\":{{\"MAX_TEXTURE_SIZE\":4096,\"VALUE\":{number}}},",
                "\"webGl2:parameters\":{{\"MAX_TEXTURE_SIZE\":4096}},",
                "\"webGl:shaderPrecisionFormats\":{{\"HIGH_FLOAT\":{{\"precision\":23}}}},",
                "\"webGl2:shaderPrecisionFormats\":{{\"HIGH_FLOAT\":{{\"precision\":23}}}},",
                "\"webGl:contextAttributes\":{{\"alpha\":true}},",
                "\"webGl2:contextAttributes\":{{\"alpha\":true}},",
                "\"fonts\":[\"Arial\",\"Segoe UI\"],",
                "\"fonts:spacing_seed\":1,\"canvas:seed\":2,\"audio:seed\":3,",
                "\"navigator.language\":\"en-US\",",
                "\"navigator.languages\":[\"en-US\",\"en\"],",
                "\"voices\":[{{\"isLocalService\":true,\"isDefault\":true,",
                "\"voiceURI\":\"urn:voice:one\",\"name\":\"Voice One\",\"lang\":\"en-US\"}}]",
                "}}"
            ),
        )
    }

    #[test]
    fn typed_projection_uses_domain_numeric_canonicalization()
    -> Result<(), Box<dyn std::error::Error>> {
        let integer = profile_stable_identity_from_config_bytes(config("1").as_bytes())?;
        let decimal = profile_stable_identity_from_config_bytes(config("1.0").as_bytes())?;
        let exponent = profile_stable_identity_from_config_bytes(config("1e0").as_bytes())?;
        assert_eq!(integer, decimal);
        assert_eq!(integer, exponent);
        Ok(())
    }

    #[test]
    fn missing_required_semantic_field_fails_closed() {
        let malformed = config("1").replace("\"navigator.oscpu\":\"Windows NT 10.0; Win64; x64\",", "");
        assert_eq!(
            profile_stable_identity_from_config_bytes(malformed.as_bytes()),
            Err(CamoufoxIdentityProjectionError::InvalidMaterialization)
        );
    }
}
