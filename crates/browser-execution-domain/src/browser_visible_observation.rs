use super::ProfileStableIdentity;
use sha2::{Digest, Sha256};

const MAX_COLLECTION_ROWS: usize = 512;
const MAX_OBSERVED_TEXT_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observed<T> {
    Available(T),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserKeyValueObservation {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontAvailabilityObservation {
    pub family: String,
    pub available: bool,
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
pub struct GraphicsObservation {
    pub webgl_vendor: Observed<String>,
    pub webgl_renderer: Observed<String>,
    pub webgl_extensions: Observed<Vec<String>>,
    pub webgl_parameters: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl2_parameters: Observed<Vec<BrowserKeyValueObservation>>,
    pub shader_precision: Observed<Vec<BrowserKeyValueObservation>>,
    pub context_attributes: Observed<Vec<BrowserKeyValueObservation>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontObservation {
    pub configured_fonts: Observed<Vec<FontAvailabilityObservation>>,
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

impl ProfileStableIdentity {
    /// Compare one pre-navigation browser-visible observation with the generation-owned identity.
    ///
    /// This is the semantic admission owner for Profile-Stable browser fields. Runtime adapters may
    /// collect raw browser values, but they may not replace this typed policy with an aggregate hash.
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
        if expected.user_agent() != observation.user_agent {
            return Err(BrowserVisibleMismatch::UserAgent);
        }
        if firefox_major(&observation.user_agent) != Some(expected.browser_major()) {
            return Err(BrowserVisibleMismatch::BrowserMajor);
        }
        if expected.platform() != observation.platform {
            return Err(BrowserVisibleMismatch::Platform);
        }
        match &observation.oscpu {
            Observed::Available(value) if value == expected.oscpu() => Ok(()),
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
        compare_sorted_strings_digest(
            expected.webgl_extensions_sha256(),
            &observation.webgl_extensions,
            "webgl-extensions-v1",
            BrowserVisibleMismatch::WebGlExtensions,
        )?;
        compare_key_values_digest(
            expected.webgl_parameters_sha256(),
            &observation.webgl_parameters,
            "webgl-parameters-v1",
            BrowserVisibleMismatch::WebGlParameters,
        )?;
        compare_key_values_digest(
            expected.webgl2_parameters_sha256(),
            &observation.webgl2_parameters,
            "webgl2-parameters-v1",
            BrowserVisibleMismatch::WebGl2Parameters,
        )?;
        compare_key_values_digest(
            expected.shader_precision_sha256(),
            &observation.shader_precision,
            "webgl-shader-precision-v1",
            BrowserVisibleMismatch::ShaderPrecision,
        )?;
        compare_key_values_digest(
            expected.context_attributes_sha256(),
            &observation.context_attributes,
            "webgl-context-attributes-v1",
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
        let families = fonts
            .iter()
            .map(|font| font.family.clone())
            .collect::<Vec<_>>();
        let Some(observed_sha256) = canonical_sorted_strings_sha256("font-set-v1", &families) else {
            return Err(BrowserVisibleMismatch::FontSet);
        };
        if observed_sha256 != self.fonts().font_set_sha256() {
            return Err(BrowserVisibleMismatch::FontSet);
        }
        Ok(())
    }

    fn compare_locale(&self, observation: &LocaleObservation) -> Result<(), BrowserVisibleMismatch> {
        let expected = self.locale();
        if !bounded_text(&observation.language) || expected.language() != observation.language {
            return Err(BrowserVisibleMismatch::Language);
        }
        if observation.languages.is_empty()
            || observation.languages.len() > MAX_COLLECTION_ROWS
            || observation.languages.first() != Some(&observation.language)
            || observation.languages.iter().any(|value| !bounded_text(value))
        {
            return Err(BrowserVisibleMismatch::Languages);
        }
        let observed_languages = canonical_ordered_strings_sha256(
            "navigator-languages-v1",
            &observation.languages,
        );
        if observed_languages != expected.languages_sha256() {
            return Err(BrowserVisibleMismatch::Languages);
        }
        match (expected.speech_voices_sha256(), &observation.speech_voices) {
            (None, SpeechVoicesObservation::NotApplicable) => Ok(()),
            (Some(expected_sha256), SpeechVoicesObservation::Available(voices)) => {
                let Some(observed_sha256) = canonical_speech_voices_sha256(voices) else {
                    return Err(BrowserVisibleMismatch::SpeechVoices);
                };
                if observed_sha256 == expected_sha256 {
                    Ok(())
                } else {
                    Err(BrowserVisibleMismatch::SpeechVoices)
                }
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

fn compare_sorted_strings_digest(
    expected: &str,
    observed: &Observed<Vec<String>>,
    domain: &str,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    let Observed::Available(values) = observed else {
        return Err(mismatch);
    };
    let Some(digest) = canonical_sorted_strings_sha256(domain, values) else {
        return Err(mismatch);
    };
    if digest == expected {
        Ok(())
    } else {
        Err(mismatch)
    }
}

fn compare_key_values_digest(
    expected: &str,
    observed: &Observed<Vec<BrowserKeyValueObservation>>,
    domain: &str,
    mismatch: BrowserVisibleMismatch,
) -> Result<(), BrowserVisibleMismatch> {
    let Observed::Available(values) = observed else {
        return Err(mismatch);
    };
    let Some(digest) = canonical_key_values_sha256(domain, values) else {
        return Err(mismatch);
    };
    if digest == expected {
        Ok(())
    } else {
        Err(mismatch)
    }
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

fn canonical_ordered_strings_sha256(domain: &str, values: &[String]) -> String {
    let mut hasher = canonical_hasher(domain);
    for value in values {
        hash_component(&mut hasher, value);
    }
    encode_digest(hasher.finalize())
}

fn canonical_sorted_strings_sha256(domain: &str, values: &[String]) -> Option<String> {
    if values.is_empty()
        || values.len() > MAX_COLLECTION_ROWS
        || values.iter().any(|value| !bounded_text(value))
    {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(canonical_ordered_strings_sha256(domain, &sorted))
}

fn canonical_key_values_sha256(
    domain: &str,
    values: &[BrowserKeyValueObservation],
) -> Option<String> {
    if values.is_empty()
        || values.len() > MAX_COLLECTION_ROWS
        || values
            .iter()
            .any(|row| !bounded_text(&row.name) || !bounded_text(&row.value))
    {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.name.cmp(&right.name).then(left.value.cmp(&right.value)));
    if sorted.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return None;
    }
    let mut hasher = canonical_hasher(domain);
    for row in sorted {
        hash_component(&mut hasher, &row.name);
        hash_component(&mut hasher, &row.value);
    }
    Some(encode_digest(hasher.finalize()))
}

fn canonical_speech_voices_sha256(voices: &[SpeechVoiceObservation]) -> Option<String> {
    if voices.len() > MAX_COLLECTION_ROWS
        || voices
            .iter()
            .any(|voice| !bounded_text(&voice.language) || !bounded_text(&voice.name))
    {
        return None;
    }
    let mut sorted = voices.to_vec();
    sorted.sort();
    let mut hasher = canonical_hasher("speech-voices-v1");
    for voice in sorted {
        hash_component(&mut hasher, &voice.language);
        hash_component(&mut hasher, &voice.name);
        hash_component(&mut hasher, if voice.local_service { "1" } else { "0" });
        hash_component(&mut hasher, if voice.is_default { "1" } else { "0" });
    }
    Some(encode_digest(hasher.finalize()))
}

fn canonical_hasher(domain: &str) -> Sha256 {
    let mut hasher = Sha256::new();
    hash_component(&mut hasher, domain);
    hasher
}

fn hash_component(hasher: &mut Sha256, value: &str) {
    let bytes = value.as_bytes();
    hasher.update(bytes.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(bytes);
    hasher.update(b";");
}

fn encode_digest(digest: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = digest.as_ref();
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
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

    fn key_values(prefix: &str) -> Vec<BrowserKeyValueObservation> {
        vec![
            BrowserKeyValueObservation {
                name: format!("{prefix}.a"),
                value: "1".to_owned(),
            },
            BrowserKeyValueObservation {
                name: format!("{prefix}.b"),
                value: "2".to_owned(),
            },
        ]
    }

    fn fixture(
        device_memory_gib: Option<u16>,
        voices_applicable: bool,
    ) -> Result<(ProfileStableIdentity, BrowserVisibleObservation), Box<dyn std::error::Error>> {
        let extensions = vec!["EXT_beta".to_owned(), "EXT_alpha".to_owned()];
        let webgl = key_values("webgl");
        let webgl2 = key_values("webgl2");
        let shader = key_values("shader");
        let context = key_values("context");
        let fonts = vec!["Arial".to_owned(), "Segoe UI".to_owned()];
        let languages = vec!["en-US".to_owned(), "en".to_owned()];
        let voices = vec![SpeechVoiceObservation {
            language: "en-US".to_owned(),
            name: "Example Voice".to_owned(),
            local_service: true,
            is_default: true,
        }];
        let voice_hash = voices_applicable
            .then(|| canonical_speech_voices_sha256(&voices))
            .flatten();
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
                canonical_sorted_strings_sha256("webgl-extensions-v1", &extensions)
                    .ok_or("extensions")?,
                canonical_key_values_sha256("webgl-parameters-v1", &webgl).ok_or("webgl")?,
                canonical_key_values_sha256("webgl2-parameters-v1", &webgl2).ok_or("webgl2")?,
                canonical_key_values_sha256("webgl-shader-precision-v1", &shader)
                    .ok_or("shader")?,
                canonical_key_values_sha256("webgl-context-attributes-v1", &context)
                    .ok_or("context")?,
            )?,
            FontIdentity::new(
                canonical_sorted_strings_sha256("font-set-v1", &fonts).ok_or("fonts")?,
                "7".repeat(64),
            )?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                "8".repeat(64),
                "9".repeat(64),
            )?,
            LocaleIdentity::new(
                "en-US",
                canonical_ordered_strings_sha256("navigator-languages-v1", &languages),
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
                webgl_extensions: Observed::Available(extensions),
                webgl_parameters: Observed::Available(webgl),
                webgl2_parameters: Observed::Available(webgl2),
                shader_precision: Observed::Available(shader),
                context_attributes: Observed::Available(context),
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
