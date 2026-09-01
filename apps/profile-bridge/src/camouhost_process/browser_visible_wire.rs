use browser_execution_domain::ProfileStableIdentity;
use browser_execution_domain::browser_visible_observation::{
    BrowserKeyValueObservation, BrowserOsObservation, BrowserValue, BrowserVisibleObservation,
    BrowserVisibleSha256Port, DisplayObservation, FontAvailabilityObservation, FontObservation,
    GraphicsObservation, HardwareCapabilityObservation, LocaleObservation, Observed,
    SpeechVoiceObservation, SpeechVoicesObservation,
};
use browser_execution_domain::host_compatibility::{HostDisplayEnvironment, HostGraphicsBackend};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const MAX_BROWSER_VISIBLE_PAYLOAD_BYTES: usize = 512 * 1024;
const MAX_JSON_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BrowserVisibleWireError {
    InvalidPayload,
    IdentityMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HostRuntimeEvidence {
    pub display: HostDisplayEnvironment,
    pub graphics_backend: HostGraphicsBackend,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireObservation {
    user_agent: String,
    platform: String,
    oscpu: Option<String>,
    hardware_concurrency: u16,
    device_memory_gib: Option<u16>,
    max_touch_points: u16,
    screen: WireScreen,
    graphics: WireGraphics,
    fonts: Vec<WireFont>,
    language: String,
    languages: Vec<String>,
    speech_voices: WireSpeechVoices,
    #[serde(default)]
    test_webgl_shape: Option<WireTestWebGlShape>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTestWebGlShape {
    configured_rows: u16,
    configured_null: u16,
    configured_bool: u16,
    configured_number: u16,
    configured_text: u16,
    configured_list: u16,
    configured_map: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireScreen {
    width: u32,
    height: u32,
    avail_width: u32,
    avail_height: u32,
    avail_left: i32,
    avail_top: i32,
    color_depth: u16,
    pixel_depth: u16,
    device_pixel_ratio: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGraphics {
    vendor: Option<String>,
    renderer: Option<String>,
    webgl: Option<WireGraphicsContext>,
    webgl2: Option<WireGraphicsContext>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGraphicsContext {
    extensions: Vec<String>,
    parameters: Map<String, Value>,
    shader_precision: Map<String, Value>,
    context_attributes: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFont {
    family: String,
    available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum WireSpeechVoices {
    Available { voices: Vec<WireSpeechVoice> },
    NotApplicable,
    Unavailable,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSpeechVoice {
    language: String,
    name: String,
    voice_uri: String,
    local_service: bool,
    is_default: bool,
}

pub(super) fn verify_browser_visible_payload(
    expected: &ProfileStableIdentity,
    payload: &[u8],
) -> Result<HostRuntimeEvidence, BrowserVisibleWireError> {
    emit_test_webgl_configured_shape(payload);
    let observation = parse_browser_visible_payload(payload)?;
    emit_test_webgl_observation_shape(&observation);
    expected
        .compare_browser_visible(&observation, &mut Sha256Adapter)
        .map_err(|mismatch| {
            eprintln!("CAMOUHOST_BROWSER_VISIBLE_MISMATCH={mismatch:?}");
            BrowserVisibleWireError::IdentityMismatch
        })?;
    host_runtime_evidence(&observation)
}

fn emit_test_webgl_configured_shape(payload: &[u8]) {
    if std::env::var_os("CAMOUHOST_TEST_DIAGNOSTIC").as_deref()
        != Some(std::ffi::OsStr::new("webgl-shape"))
    {
        return;
    }
    let Ok(wire) = serde_json::from_slice::<WireObservation>(payload) else {
        return;
    };
    let Some(shape) = wire.test_webgl_shape else {
        return;
    };
    eprintln!(
        "CAMOUHOST_TEST_WEBGL_CONFIGURED_SHAPE=rows={};null={};bool={};number={};text={};list={};map={}",
        shape.configured_rows,
        shape.configured_null,
        shape.configured_bool,
        shape.configured_number,
        shape.configured_text,
        shape.configured_list,
        shape.configured_map,
    );
}

fn emit_test_webgl_observation_shape(observation: &BrowserVisibleObservation) {
    if std::env::var_os("CAMOUHOST_TEST_DIAGNOSTIC").as_deref()
        != Some(std::ffi::OsStr::new("webgl-shape"))
    {
        return;
    }
    let Observed::Available(rows) = &observation.graphics.webgl_parameters else {
        eprintln!("CAMOUHOST_TEST_WEBGL_OBSERVED_SHAPE=unavailable");
        return;
    };
    let mut nulls = 0usize;
    let mut bools = 0usize;
    let mut numbers = 0usize;
    let mut texts = 0usize;
    let mut lists = 0usize;
    let mut maps = 0usize;
    for row in rows {
        match &row.value {
            BrowserValue::Null => nulls += 1,
            BrowserValue::Bool(_) => bools += 1,
            BrowserValue::Number(_) => numbers += 1,
            BrowserValue::Text(_) => texts += 1,
            BrowserValue::List(_) => lists += 1,
            BrowserValue::Map(_) => maps += 1,
        }
    }
    eprintln!(
        "CAMOUHOST_TEST_WEBGL_OBSERVED_SHAPE=rows={};null={nulls};bool={bools};number={numbers};text={texts};list={lists};map={maps}",
        rows.len()
    );
}

fn host_runtime_evidence(
    observation: &BrowserVisibleObservation,
) -> Result<HostRuntimeEvidence, BrowserVisibleWireError> {
    let graphics_backend = match (
        &observation.graphics.webgl_extensions,
        &observation.graphics.webgl2_extensions,
    ) {
        (Observed::Available(_), Observed::Available(_)) => HostGraphicsBackend::WebGlAndWebGl2,
        (Observed::Available(_), Observed::Unavailable) => HostGraphicsBackend::WebGl,
        (Observed::Unavailable, Observed::Available(_)) => HostGraphicsBackend::WebGl2,
        (Observed::Unavailable, Observed::Unavailable) => {
            return Err(BrowserVisibleWireError::InvalidPayload);
        }
    };
    let display = observation.display;
    Ok(HostRuntimeEvidence {
        display: HostDisplayEnvironment::new(
            display.width,
            display.height,
            display.avail_width,
            display.avail_height,
            display.avail_left,
            display.avail_top,
            display.color_depth,
            display.pixel_depth,
            display.device_pixel_ratio_milli,
        ),
        graphics_backend,
    })
}

fn parse_browser_visible_payload(
    payload: &[u8],
) -> Result<BrowserVisibleObservation, BrowserVisibleWireError> {
    if payload.is_empty() || payload.len() > MAX_BROWSER_VISIBLE_PAYLOAD_BYTES {
        return Err(BrowserVisibleWireError::InvalidPayload);
    }
    let wire: WireObservation =
        serde_json::from_slice(payload).map_err(|_| BrowserVisibleWireError::InvalidPayload)?;
    let dpr_milli = dpr_milli(wire.screen.device_pixel_ratio)?;
    let graphics = graphics_observation(wire.graphics)?;
    let speech_voices = match wire.speech_voices {
        WireSpeechVoices::Available { voices } => SpeechVoicesObservation::Available(
            voices
                .into_iter()
                .map(|voice| SpeechVoiceObservation {
                    language: voice.language,
                    name: voice.name,
                    voice_uri: voice.voice_uri,
                    local_service: voice.local_service,
                    is_default: voice.is_default,
                })
                .collect(),
        ),
        WireSpeechVoices::NotApplicable => SpeechVoicesObservation::NotApplicable,
        WireSpeechVoices::Unavailable => SpeechVoicesObservation::Unavailable,
    };

    Ok(BrowserVisibleObservation {
        browser_os: BrowserOsObservation {
            user_agent: wire.user_agent,
            platform: wire.platform,
            oscpu: wire
                .oscpu
                .map_or(Observed::Unavailable, Observed::Available),
        },
        hardware: HardwareCapabilityObservation {
            hardware_concurrency: wire.hardware_concurrency,
            device_memory_gib: wire
                .device_memory_gib
                .map_or(Observed::Unavailable, Observed::Available),
            max_touch_points: wire.max_touch_points,
        },
        display: DisplayObservation {
            width: wire.screen.width,
            height: wire.screen.height,
            avail_width: wire.screen.avail_width,
            avail_height: wire.screen.avail_height,
            avail_left: wire.screen.avail_left,
            avail_top: wire.screen.avail_top,
            color_depth: wire.screen.color_depth,
            pixel_depth: wire.screen.pixel_depth,
            device_pixel_ratio_milli: dpr_milli,
        },
        graphics,
        fonts: FontObservation {
            configured_fonts: Observed::Available(
                wire.fonts
                    .into_iter()
                    .map(|font| FontAvailabilityObservation {
                        family: font.family,
                        available: font.available,
                    })
                    .collect(),
            ),
        },
        locale: LocaleObservation {
            language: wire.language,
            languages: wire.languages,
            speech_voices,
        },
    })
}

fn graphics_observation(
    wire: WireGraphics,
) -> Result<GraphicsObservation, BrowserVisibleWireError> {
    let (webgl_extensions, webgl_parameters, webgl_shader_precision, webgl_context_attributes) =
        graphics_context(wire.webgl)?;
    let (webgl2_extensions, webgl2_parameters, webgl2_shader_precision, webgl2_context_attributes) =
        graphics_context(wire.webgl2)?;

    Ok(GraphicsObservation {
        webgl_vendor: wire
            .vendor
            .map_or(Observed::Unavailable, Observed::Available),
        webgl_renderer: wire
            .renderer
            .map_or(Observed::Unavailable, Observed::Available),
        webgl_extensions,
        webgl2_extensions,
        webgl_parameters,
        webgl2_parameters,
        webgl_shader_precision,
        webgl2_shader_precision,
        webgl_context_attributes,
        webgl2_context_attributes,
    })
}

type GraphicsContextObservation = (
    Observed<Vec<String>>,
    Observed<Vec<BrowserKeyValueObservation>>,
    Observed<Vec<BrowserKeyValueObservation>>,
    Observed<Vec<BrowserKeyValueObservation>>,
);

fn graphics_context(
    wire: Option<WireGraphicsContext>,
) -> Result<GraphicsContextObservation, BrowserVisibleWireError> {
    let Some(wire) = wire else {
        return Ok((
            Observed::Unavailable,
            Observed::Unavailable,
            Observed::Unavailable,
            Observed::Unavailable,
        ));
    };
    Ok((
        Observed::Available(wire.extensions),
        Observed::Available(map_key_values(&wire.parameters, 0)?),
        Observed::Available(map_key_values(&wire.shader_precision, 0)?),
        Observed::Available(map_key_values(&wire.context_attributes, 0)?),
    ))
}

fn map_key_values(
    values: &Map<String, Value>,
    depth: usize,
) -> Result<Vec<BrowserKeyValueObservation>, BrowserVisibleWireError> {
    if depth > MAX_JSON_DEPTH {
        return Err(BrowserVisibleWireError::InvalidPayload);
    }
    values
        .iter()
        .map(|(name, value)| {
            Ok(BrowserKeyValueObservation {
                name: name.clone(),
                value: browser_value(value, depth + 1)?,
            })
        })
        .collect()
}

fn browser_value(value: &Value, depth: usize) -> Result<BrowserValue, BrowserVisibleWireError> {
    if depth > MAX_JSON_DEPTH {
        return Err(BrowserVisibleWireError::InvalidPayload);
    }
    match value {
        Value::Null => Ok(BrowserValue::Null),
        Value::Bool(value) => Ok(BrowserValue::Bool(*value)),
        Value::Number(value) => Ok(BrowserValue::Number(value.to_string())),
        Value::String(value) => Ok(BrowserValue::Text(value.clone())),
        Value::Array(values) => values
            .iter()
            .map(|value| browser_value(value, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(BrowserValue::List),
        Value::Object(values) => map_key_values(values, depth + 1).map(BrowserValue::Map),
    }
}

fn dpr_milli(value: f64) -> Result<u32, BrowserVisibleWireError> {
    let milli = value * 1000.0;
    if !value.is_finite()
        || !milli.is_finite()
        || milli <= 0.0
        || (milli.round() - milli).abs() > f64::EPSILON
        || milli > f64::from(u32::MAX)
    {
        return Err(BrowserVisibleWireError::InvalidPayload);
    }
    Ok(milli.round() as u32)
}

struct Sha256Adapter;

impl BrowserVisibleSha256Port for Sha256Adapter {
    type Error = core::convert::Infallible;

    fn sha256(&mut self, canonical_bytes: &[u8]) -> Result<[u8; 32], Self::Error> {
        Ok(Sha256::digest(canonical_bytes).into())
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserVisibleWireError, parse_browser_visible_payload};
    use browser_execution_domain::browser_visible_observation::{
        BrowserVisibleSha256Port, Observed, SpeechVoicesObservation,
    };

    const MATCHING_PAYLOAD: &str = r#"{
        "user_agent":"Mozilla/5.0 Firefox/152.0",
        "platform":"Win32",
        "oscpu":"Windows NT 10.0; Win64; x64",
        "hardware_concurrency":8,
        "device_memory_gib":8,
        "max_touch_points":0,
        "screen":{"width":1920,"height":1080,"avail_width":1920,"avail_height":1040,"avail_left":0,"avail_top":0,"color_depth":24,"pixel_depth":24,"device_pixel_ratio":1.0},
        "graphics":{"vendor":"Vendor","renderer":"Renderer","webgl":{"extensions":["EXT_a"],"parameters":{"3379":4096},"shader_precision":{"35633,36338":{"rangeMin":127,"rangeMax":127,"precision":23}},"context_attributes":{"alpha":true}},"webgl2":{"extensions":["EXT_b"],"parameters":{"3379":4096},"shader_precision":{"35633,36338":{"rangeMin":127,"rangeMax":127,"precision":23}},"context_attributes":{"alpha":true}}},
        "fonts":[{"family":"Arial","available":true}],
        "language":"en-US",
        "languages":["en-US","en"],
        "speech_voices":{"status":"available","voices":[{"language":"en-US","name":"Voice","voice_uri":"urn:voice:one","local_service":true,"is_default":true}]}
    }"#;

    #[test]
    fn strict_wire_maps_to_domain_observation() -> Result<(), Box<dyn std::error::Error>> {
        let observation = parse_browser_visible_payload(MATCHING_PAYLOAD.as_bytes())
            .map_err(|_| "payload should parse")?;
        assert_eq!(observation.browser_os.platform, "Win32");
        assert_eq!(
            observation.hardware.device_memory_gib,
            Observed::Available(8)
        );
        assert!(matches!(
            observation.locale.speech_voices,
            SpeechVoicesObservation::Available(ref voices) if voices[0].voice_uri == "urn:voice:one"
        ));
        Ok(())
    }

    #[test]
    fn unknown_or_non_integral_dpr_fails_closed() {
        let unknown = MATCHING_PAYLOAD.replace(
            "\"user_agent\":\"Mozilla/5.0 Firefox/152.0\"",
            "\"user_agent\":\"Mozilla/5.0 Firefox/152.0\",\"unexpected\":true",
        );
        assert_eq!(
            parse_browser_visible_payload(unknown.as_bytes()),
            Err(BrowserVisibleWireError::InvalidPayload)
        );
        let bad_dpr = MATCHING_PAYLOAD.replace(
            "\"device_pixel_ratio\":1.0",
            "\"device_pixel_ratio\":1.0005",
        );
        assert_eq!(
            parse_browser_visible_payload(bad_dpr.as_bytes()),
            Err(BrowserVisibleWireError::InvalidPayload)
        );
    }

    #[test]
    fn sha_adapter_is_deterministic() {
        let mut first = super::Sha256Adapter;
        let mut second = super::Sha256Adapter;
        assert_eq!(first.sha256(b"typed-wire"), second.sha256(b"typed-wire"));
    }
}
