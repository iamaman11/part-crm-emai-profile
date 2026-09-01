use super::ProfileStableIdentity;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_CANONICAL_VALUE_BYTES: usize = 128 * 1024;
const MAX_COLLECTION_ROWS: usize = 512;
const MAX_OBSERVED_TEXT_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observed<T> {
    Available(T),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserOsObservation {
    pub user_agent: String,
    pub platform: String,
    pub oscpu: Observed<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HardwareCapabilityObservation {
    pub hardware_concurrency: u16,
    pub device_memory_gib: Observed<u16>,
    pub max_touch_points: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayObservation {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
    pub avail_left: i32,
    pub avail_top: i32,
    pub color_depth: u16,
    pub pixel_depth: u16,
    pub device_pixel_ratio_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DualWebGlValueObservation {
    pub webgl: Observed<Value>,
    pub webgl2: Observed<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsObservation {
    pub webgl_vendor: Observed<String>,
    pub webgl_renderer: Observed<String>,
    pub webgl_extensions: DualWebGlValueObservation,
    pub webgl_parameters: Observed<Value>,
    pub webgl2_parameters: Observed<Value>,
    pub shader_precision: DualWebGlValueObservation,
    pub context_attributes: DualWebGlValueObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontAvailabilityObservation {
    pub family: String,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontObservation {
    pub configured_fonts: Observed<Vec<FontAvailabilityObservation>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SpeechVoiceObservation {
    pub language: String,
    pub name: String,
    pub local_service: bool,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeechVoicesObservation {
    Available(Vec<SpeechVoiceObservation>),
    NotApplicable,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleObservation {
    pub language: String,
    pub languages: Vec<String>,
    pub speech_voices: SpeechVoicesObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserVisibleObservation {
    pub browser_os: BrowserOsObservation,
    pub hardware: HardwareCapabilityObservation,
    pub display: DisplayObservation,
    pub graphics: GraphicsObservation,
    pub fonts: FontObservation,
    pub locale: LocaleObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserVisibleMismatch {
    UserAgent,
    BrowserMajor,
    Platform,
    Oscpu,
    HardwareConcurrency,
    DeviceMemory,
    MaxTouchPoints,
    DisplayWidth,
    DisplayHeight,
    DisplayAvailWidth,
    DisplayAvailHeight,
    DisplayAvailLeft,
    DisplayAvailTop,
    DisplayColorDepth,
    DisplayPixelDepth,
    DevicePixelRatio,
    WebGlVendor,
    WebGlRenderer,
    WebGlExtensions,
    WebGlParameters,
    WebGl2Parameters,
    ShaderPrecision,
    ContextAttributes,
    FontSet,
    Language,
    Languages,
    SpeechVoices,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserVisibleCanonicalizationError {
    Serialization,
    ValueTooLarge,
}

impl core::fmt::Display for BrowserVisibleCanonicalizationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Serialization => "browser-visible value could not be canonicalized",
            Self::ValueTooLarge => "browser-visible canonical value exceeds its bounded size",
        })
    }
}

impl std::error::Error for BrowserVisibleCanonicalizationError {}

/// Hash one browser-visible collection using the same canonical JSON representation used by the
/// generation-owned Camoufox projection: compact `serde_json` bytes followed by exactly one LF.
/// Runtime adapters provide browser observations; they do not own digest semantics.
pub fn canonical_browser_value_sha256(
    value: &Value,
) -> Result<String, BrowserVisibleCanonicalizationError> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|_| BrowserVisibleCanonicalizationError::Serialization)?;
    if bytes.len() > MAX_CANONICAL_VALUE_BYTES {
        return Err(BrowserVisibleCanonicalizationError::ValueTooLarge);
    }
    bytes.push(b'\n');
    Ok(sha256_hex(&bytes))
}

impl ProfileStableIdentity {
    /// Compare a pre-navigation browser observation with the generation-owned Profile-Stable
    /// identity. An aggregate probe hash may be retained as secondary evidence, but it is never a
    /// substitute for this field-level admission decision.
    pub fn compare_browser_visible(
        &self,
        observation: &BrowserVisibleObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        self.compare_browser_os(&observation.browser_os)?;
        self.compare_hardware(&observation.hardware)?;
        self.compare_display(observation.display)?;
        self.compare_graphics(&observation.graphics)?;
        self.compare_fonts(&observation.fonts)?;
        self.compare_locale(&observation.locale)
    }

    fn compare_browser_os(
        &self,
        observation: &BrowserOsObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.browser_os();
        if !bounded_text(&observation.user_agent)
            || expected.user_agent() != observation.user_agent
        {
            return Err(BrowserVisibleMismatch::UserAgent);
        }
        if firefox_major(&observation.user_agent) != Some(expected.browser_major()) {
            return Err(BrowserVisibleMismatch::BrowserMajor);
        }
        if !bounded_text(&observation.platform) || expected.platform() != observation.platform {
            return Err(BrowserVisibleMismatch::Platform);
        }
        match &observation.oscpu {
            Observed::Available(value) if bounded_text(value) && value == expected.oscpu() => Ok(()),
            Observed::Available(_) | Observed::Unavailable => Err(BrowserVisibleMismatch::Oscpu),
        }
    }

    fn compare_hardware(
        &self,
        observation: &HardwareCapabilityObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.hardware();
        if expected.hardware_concurrency() != observation.hardware_concurrency {
            return Err(BrowserVisibleMismatch::HardwareConcurrency);
        }
        match (expected.device_memory_gib(), &observation.device_memory_gib) {
            (Some(expected_value), Observed::Available(value)) if expected_value == *value => {}
            (None, Observed::Unavailable) => {}
            (Some(_), Observed::Available(_))
            | (Some(_), Observed::Unavailable)
            | (None, Observed::Available(_)) => return Err(BrowserVisibleMismatch::DeviceMemory),
        }
        if expected.max_touch_points() != observation.max_touch_points {
            return Err(BrowserVisibleMismatch::MaxTouchPoints);
        }
        Ok(())
    }

    fn compare_display(
        &self,
        observation: DisplayObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.display();
        for (matches, mismatch) in [
            (
                expected.width() == observation.width,
                BrowserVisibleMismatch::DisplayWidth,
            ),
            (
                expected.height() == observation.height,
                BrowserVisibleMismatch::DisplayHeight,
            ),
            (
                expected.avail_width() == observation.avail_width,
                BrowserVisibleMismatch::DisplayAvailWidth,
            ),
            (
                expected.avail_height() == observation.avail_height,
                BrowserVisibleMismatch::DisplayAvailHeight,
            ),
            (
                expected.avail_left() == observation.avail_left,
                BrowserVisibleMismatch::DisplayAvailLeft,
            ),
            (
                expected.avail_top() == observation.avail_top,
                BrowserVisibleMismatch::DisplayAvailTop,
            ),
            (
                expected.color_depth() == observation.color_depth,
                BrowserVisibleMismatch::DisplayColorDepth,
            ),
            (
                expected.pixel_depth() == observation.pixel_depth,
                BrowserVisibleMismatch::DisplayPixelDepth,
            ),
            (
                expected.device_pixel_ratio_milli() == observation.device_pixel_ratio_milli,
                BrowserVisibleMismatch::DevicePixelRatio,
            ),
        ] {
            if !matches {
                return Err(mismatch);
            }
        }
        Ok(())
    }

    fn compare_graphics(
        &self,
        observation: &GraphicsObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.graphics();
        compare_required_text(
            expected.webgl_vendor(),
            &observation.webgl_vendor,
            BrowserVisibleMismatch::WebGlVendor,
        )?;
        compare_required_text(
            expected.webgl_renderer(),
            &observation.webgl_renderer,
            BrowserVisibleMismatch::WebGlRenderer,
        )?;
        compare_dual_webgl_digest(
            expected.webgl_extensions_sha256(),
            &observation.webgl_extensions,
            BrowserVisibleMismatch::WebGlExtensions,
        )?;
        compare_value_digest(
            expected.webgl_parameters_sha256(),
            &observation.webgl_parameters,
            BrowserVisibleMismatch::WebGlParameters,
        )?;
        compare_value_digest(
            expected.webgl2_parameters_sha256(),
            &observation.webgl2_parameters,
            BrowserVisibleMismatch::WebGl2Parameters,
        )?;
        compare_dual_webgl_digest(
            expected.shader_precision_sha256(),
            &observation.shader_precision,
            BrowserVisibleMismatch::ShaderPrecision,
        )?;
        compare_dual_webgl_digest(
            expected.context_attributes_sha256(),
            &observation.context_attributes,
            BrowserVisibleMismatch::ContextAttributes,
        )
    }

    fn compare_fonts(&self, observation: &FontObservation) -> Result<(), BrowserVisibleMismatch> {
        let Observed::Available(fonts) = &observation.configured_fonts else {
            return Err(BrowserVisibleMismatch::FontSet);
        };
        if fonts.is_empty()
            || fonts.len() > MAX_COLLECTION_ROWS
            || fonts.iter().any(|font| {
                !font.available || !bounded_text(&font.family) || font.family.trim() != font.family
            })
        {
            return Err(BrowserVisibleMismatch::FontSet);
        }
        let value = Value::Array(
            fonts
                .iter()
                .map(|font| Value::String(font.family.clone()))
                .collect(),
        );
        compare_digest(
            self.fonts().font_set_sha256(),
            &value,
            BrowserVisibleMismatch::FontSet,
        )
    }

    fn compare_locale(
        &self,
        observation: &LocaleObservation,
    ) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.locale();
        if !bounded_text(&observation.language) || expected.language() != observation.language {
            return Err(BrowserVisibleMismatch::Language);
        }
        if observation.languages.is_empty()
            || observation.languages.len() > MAX_COLLECTION_ROWS
            || observation.languages.first() != Some(&observation.language)
            || observation
                .languages
                .iter()
                .any(|value| !bounded_text(value))
        {
            return Err(BrowserVisibleMismatch::Languages);
        }
        let languages = Value::Array(
            observation
                .languages
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect(),
        );
        compare_digest(
            expected.languages_sha256(),
            &languages,
            BrowserVisibleMismatch::Languages,
        )?;

        match (expected.speech_voices_sha256(), &observation.speech_voices) {
            (None, SpeechVoicesObservation::NotApplicable) => Ok(()),
            (Some(expected_sha256), SpeechVoicesObservation::Available(voices)) => {
                let Some(value) = canonical_speech_voices(voices) else {
                    return Err(BrowserVisibleMismatch::SpeechVoices);
                };
                compare_digest(
                    expected_sha256,
                    &value,
                    BrowserVisibleMismatch::SpeechVoices,
                )
            }
            (None, SpeechVoicesObservation::Available(_))
            | (None, SpeechVoicesObservation::Unavailable)
            | (Some(_), SpeechVoicesObservation::NotApplicable)
            | (Some(_), SpeechVoicesObservation::Unavailable) => {
                Err(BrowserVisibleMismatch::SpeechVoices)
            }
        }
    }
}

fn compare_required_text(
    expected: &str,
    observed: &Observed<String>,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    match observed {
        Observed::Available(value) if bounded_text(value) && value == expected => Ok(()),
        Observed::Available(_) | Observed::Unavailable => Err(mismatch),
    }
}

fn compare_value_digest(
    expected: &str,
    observed: &Observed<Value>,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    match observed {
        Observed::Available(value) => compare_digest(expected, value, mismatch),
        Observed::Unavailable => Err(mismatch),
    }
}

fn compare_dual_webgl_digest(
    expected: &str,
    observed: &DualWebGlValueObservation,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    let (Observed::Available(webgl), Observed::Available(webgl2)) =
        (&observed.webgl, &observed.webgl2)
    else {
        return Err(mismatch);
    };
    compare_digest(
        expected,
        &json!({"webgl": webgl, "webgl2": webgl2}),
        mismatch,
    )
}

fn compare_digest(
    expected: &str,
    observed: &Value,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    let Ok(observed_sha256) = canonical_browser_value_sha256(observed) else {
        return Err(mismatch);
    };
    if observed_sha256 == expected {
        Ok(())
    } else {
        Err(mismatch)
    }
}

fn canonical_speech_voices(voices: &[SpeechVoiceObservation]) -> Option<Value> {
    if voices.len() > MAX_COLLECTION_ROWS
        || voices
            .iter()
            .any(|voice| !bounded_text(&voice.language) || !bounded_text(&voice.name))
    {
        return None;
    }
    let mut sorted = voices.to_vec();
    sorted.sort();
    Some(Value::Array(
        sorted
            .into_iter()
            .map(|voice| {
                json!({
                    "default": voice.is_default,
                    "lang": voice.language,
                    "localService": voice.local_service,
                    "name": voice.name,
                })
            })
            .collect(),
    ))
}

fn firefox_major(user_agent: &str) -> Option<u16> {
    let (_, suffix) = user_agent.rsplit_once("Firefox/")?;
    let major = suffix.split('.').next()?;
    if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    major.parse().ok()
}

fn bounded_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OBSERVED_TEXT_BYTES
        && !value
            .bytes()
            .any(|byte| byte == b'\0' || byte == b'\r' || byte == b'\n')
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserOsIdentity, DisplayIdentity, FontIdentity, GraphicsIdentity,
        HardwareCapabilityIdentity, LocaleIdentity, OriginDeterminismMode,
        OriginDeterministicIdentity,
    };

    fn digest(value: &Value) -> Result<String, BrowserVisibleCanonicalizationError> {
        canonical_browser_value_sha256(value)
    }

    fn fixture(
        device_memory_gib: Option<u16>,
        voices_applicable: bool,
    ) -> Result<
        (ProfileStableIdentity, BrowserVisibleObservation),
        Box<dyn std::error::Error>,
    > {
        let webgl_extensions = json!(["EXT_alpha", "EXT_beta"]);
        let webgl2_extensions = json!(["EXT_alpha", "EXT_beta", "EXT_gamma"]);
        let webgl_parameters = json!({"3379": 16384, "3410": 8});
        let webgl2_parameters = json!({"3379": 16384, "34076": 16384});
        let webgl_shader = json!({"35632": {"highp": [127, 127, 23]}});
        let webgl2_shader = json!({"35633": {"highp": [127, 127, 23]}});
        let webgl_context = json!({"alpha": true, "antialias": true});
        let webgl2_context = json!({"alpha": true, "antialias": true});
        let fonts = vec!["Arial".to_owned(), "Segoe UI".to_owned()];
        let languages = vec!["en-US".to_owned(), "en".to_owned()];
        let voices = vec![SpeechVoiceObservation {
            language: "en-US".to_owned(),
            name: "Example Voice".to_owned(),
            local_service: true,
            is_default: true,
        }];
        let voice_hash = if voices_applicable {
            let value = canonical_speech_voices(&voices).ok_or("invalid voice fixture")?;
            Some(digest(&value)?)
        } else {
            None
        };
        let stable = ProfileStableIdentity::new(
            1,
            BrowserOsIdentity::new(
                "Mozilla/5.0 Firefox/152.0",
                152,
                "Win32",
                "Windows NT 10.0; Win64; x64",
            )?,
            HardwareCapabilityIdentity::new(8, device_memory_gib, 0)?,
            DisplayIdentity::new(1920, 1080, 1920, 1040, 0, 0, 24, 24, 1000)?,
            GraphicsIdentity::new(
                "Vendor",
                "Renderer",
                digest(&json!({
                    "webgl": webgl_extensions,
                    "webgl2": webgl2_extensions,
                }))?,
                digest(&webgl_parameters)?,
                digest(&webgl2_parameters)?,
                digest(&json!({
                    "webgl": webgl_shader,
                    "webgl2": webgl2_shader,
                }))?,
                digest(&json!({
                    "webgl": webgl_context,
                    "webgl2": webgl2_context,
                }))?,
            )?,
            FontIdentity::new(
                digest(&Value::Array(
                    fonts.iter().cloned().map(Value::String).collect(),
                ))?,
                "7".repeat(64),
            )?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                "8".repeat(64),
                "9".repeat(64),
            )?,
            LocaleIdentity::new(
                "en-US",
                digest(&Value::Array(
                    languages.iter().cloned().map(Value::String).collect(),
                ))?,
                voice_hash,
            )?,
        )?;
        let observation = BrowserVisibleObservation {
            browser_os: BrowserOsObservation {
                user_agent: "Mozilla/5.0 Firefox/152.0".to_owned(),
                platform: "Win32".to_owned(),
                oscpu: Observed::Available("Windows NT 10.0; Win64; x64".to_owned()),
            },
            hardware: HardwareCapabilityObservation {
                hardware_concurrency: 8,
                device_memory_gib: device_memory_gib
                    .map_or(Observed::Unavailable, Observed::Available),
                max_touch_points: 0,
            },
            display: DisplayObservation {
                width: 1920,
                height: 1080,
                avail_width: 1920,
                avail_height: 1040,
                avail_left: 0,
                avail_top: 0,
                color_depth: 24,
                pixel_depth: 24,
                device_pixel_ratio_milli: 1000,
            },
            graphics: GraphicsObservation {
                webgl_vendor: Observed::Available("Vendor".to_owned()),
                webgl_renderer: Observed::Available("Renderer".to_owned()),
                webgl_extensions: DualWebGlValueObservation {
                    webgl: Observed::Available(json!(["EXT_alpha", "EXT_beta"])),
                    webgl2: Observed::Available(json!([
                        "EXT_alpha",
                        "EXT_beta",
                        "EXT_gamma"
                    ])),
                },
                webgl_parameters: Observed::Available(webgl_parameters),
                webgl2_parameters: Observed::Available(webgl2_parameters),
                shader_precision: DualWebGlValueObservation {
                    webgl: Observed::Available(webgl_shader),
                    webgl2: Observed::Available(webgl2_shader),
                },
                context_attributes: DualWebGlValueObservation {
                    webgl: Observed::Available(webgl_context),
                    webgl2: Observed::Available(webgl2_context),
                },
            },
            fonts: FontObservation {
                configured_fonts: Observed::Available(
                    fonts
                        .into_iter()
                        .map(|family| FontAvailabilityObservation {
                            family,
                            available: true,
                        })
                        .collect(),
                ),
            },
            locale: LocaleObservation {
                language: "en-US".to_owned(),
                languages,
                speech_voices: if voices_applicable {
                    SpeechVoicesObservation::Available(voices)
                } else {
                    SpeechVoicesObservation::NotApplicable
                },
            },
        };
        Ok((stable, observation))
    }

    #[test]
    fn canonical_collection_hash_matches_generation_projection_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            digest(&json!({"b": 2, "a": ["x", "y"]}))?,
            digest(&json!({"a": ["x", "y"], "b": 2}))?
        );
        Ok(())
    }

    #[test]
    fn exact_typed_browser_observation_matches_canonical_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let (expected, observation) = fixture(Some(8), true)?;
        assert_eq!(expected.compare_browser_visible(&observation), Ok(()));
        Ok(())
    }

    #[test]
    fn optional_states_are_explicit_not_silent_skips() -> Result<(), Box<dyn std::error::Error>> {
        let (expected, observation) = fixture(None, false)?;
        assert_eq!(expected.compare_browser_visible(&observation), Ok(()));

        let mut wrong_memory = observation.clone();
        wrong_memory.hardware.device_memory_gib = Observed::Available(8);
        assert_eq!(
            expected.compare_browser_visible(&wrong_memory),
            Err(BrowserVisibleMismatch::DeviceMemory)
        );

        let mut unavailable_voices = observation;
        unavailable_voices.locale.speech_voices = SpeechVoicesObservation::Unavailable;
        assert_eq!(
            expected.compare_browser_visible(&unavailable_voices),
            Err(BrowserVisibleMismatch::SpeechVoices)
        );
        Ok(())
    }

    #[test]
    fn representative_field_drift_is_rejected_with_typed_reason()
    -> Result<(), Box<dyn std::error::Error>> {
        let (expected, observation) = fixture(Some(8), true)?;
        let cases = [
            {
                let mut value = observation.clone();
                value.browser_os.oscpu = Observed::Available("Linux x86_64".to_owned());
                (value, BrowserVisibleMismatch::Oscpu)
            },
            {
                let mut value = observation.clone();
                value.hardware.hardware_concurrency = 16;
                (value, BrowserVisibleMismatch::HardwareConcurrency)
            },
            {
                let mut value = observation.clone();
                value.display.device_pixel_ratio_milli = 1250;
                (value, BrowserVisibleMismatch::DevicePixelRatio)
            },
            {
                let mut value = observation.clone();
                value.graphics.webgl_renderer = Observed::Available("Other Renderer".to_owned());
                (value, BrowserVisibleMismatch::WebGlRenderer)
            },
            {
                let mut value = observation.clone();
                value.graphics.webgl2_parameters = Observed::Unavailable;
                (value, BrowserVisibleMismatch::WebGl2Parameters)
            },
            {
                let mut value = observation.clone();
                if let Observed::Available(fonts) = &mut value.fonts.configured_fonts
                    && let Some(font) = fonts.first_mut()
                {
                    font.available = false;
                }
                (value, BrowserVisibleMismatch::FontSet)
            },
            {
                let mut value = observation.clone();
                value.locale.language = "de-DE".to_owned();
                (value, BrowserVisibleMismatch::Language)
            },
            {
                let mut value = observation.clone();
                value.locale.speech_voices = SpeechVoicesObservation::Unavailable;
                (value, BrowserVisibleMismatch::SpeechVoices)
            },
        ];
        for (value, mismatch) in cases {
            assert_eq!(expected.compare_browser_visible(&value), Err(mismatch));
        }
        Ok(())
    }
}
