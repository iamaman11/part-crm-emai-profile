use super::ProfileStableIdentity;

const MAX_COLLECTION_ROWS: usize = 512;
const MAX_OBSERVED_TEXT_BYTES: usize = 4096;

/// Pure-domain port for the SHA-256 effect used by Profile-Stable canonicalization.
///
/// `browser-execution-domain` owns the exact canonical bytes and domain separation. Outer
/// adapters only provide SHA-256 and typed values; they may not invent an alternate JSON/hash
/// convention for generation identity or browser admission.
pub trait BrowserVisibleSha256Port {
    type Error;

    fn sha256(&mut self, canonical_bytes: &[u8]) -> Result<[u8; 32], Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observed<T> {
    Available(T),
    Unavailable,
}

/// JSON-independent typed value used for WebGL capability/config observations.
///
/// A runtime/config adapter parses its native representation into this tree. The domain owns the
/// recursive canonical encoding, so serde/JavaScript/Python object serialization is never an
/// identity authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserValue {
    Null,
    Bool(bool),
    Number(String),
    Text(String),
    List(Vec<BrowserValue>),
    Map(Vec<BrowserKeyValueObservation>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserKeyValueObservation {
    pub name: String,
    pub value: BrowserValue,
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
    pub voice_uri: String,
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

/// Browser-visible graphics surface.
///
/// WebGL1 and WebGL2 are explicit inputs. The generation identity intentionally stores one
/// combined extensions digest and one combined shader/context digest, but the combination and
/// context labels are owned here rather than by a config or test adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsObservation {
    pub webgl_vendor: Observed<String>,
    pub webgl_renderer: Observed<String>,
    pub webgl_extensions: Observed<Vec<String>>,
    pub webgl2_extensions: Observed<Vec<String>>,
    pub webgl_parameters: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl2_parameters: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl_shader_precision: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl2_shader_precision: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl_context_attributes: Observed<Vec<BrowserKeyValueObservation>>,
    pub webgl2_context_attributes: Observed<Vec<BrowserKeyValueObservation>>,
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
    /// This is the semantic admission owner for Profile-Stable browser fields. Runtime adapters
    /// collect typed values and provide the SHA-256 primitive only; aggregate probe/config hashes
    /// cannot replace this verdict or choose another canonicalization.
    pub fn compare_browser_visible<D: BrowserVisibleSha256Port>(
        &self,
        observation: &BrowserVisibleObservation,
        digester: &mut D,
    ) -> Result<(), BrowserVisibleMismatch> {
        self.compare_browser_os(&observation.browser_os)?;
        self.compare_hardware(&observation.hardware)?;
        self.compare_display(observation.display)?;
        self.compare_graphics(&observation.graphics, digester)?;
        self.compare_fonts(&observation.fonts, digester)?;
        self.compare_locale(&observation.locale, digester)
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

    fn compare_graphics<D: BrowserVisibleSha256Port>(
        &self,
        observation: &GraphicsObservation,
        digester: &mut D,
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
        compare_extensions_digest(
            expected.webgl_extensions_sha256(),
            &observation.webgl_extensions,
            &observation.webgl2_extensions,
            digester,
        )?;
        compare_key_values_digest(
            expected.webgl_parameters_sha256(),
            &observation.webgl_parameters,
            canonical_webgl_parameters_sha256,
            BrowserVisibleMismatch::WebGlParameters,
            digester,
        )?;
        compare_key_values_digest(
            expected.webgl2_parameters_sha256(),
            &observation.webgl2_parameters,
            canonical_webgl2_parameters_sha256,
            BrowserVisibleMismatch::WebGl2Parameters,
            digester,
        )?;
        compare_context_pair_digest(
            expected.shader_precision_sha256(),
            &observation.webgl_shader_precision,
            &observation.webgl2_shader_precision,
            canonical_shader_precision_sha256,
            BrowserVisibleMismatch::ShaderPrecision,
            digester,
        )?;
        compare_context_pair_digest(
            expected.context_attributes_sha256(),
            &observation.webgl_context_attributes,
            &observation.webgl2_context_attributes,
            canonical_context_attributes_sha256,
            BrowserVisibleMismatch::ContextAttributes,
            digester,
        )
    }

    fn compare_fonts<D: BrowserVisibleSha256Port>(
        &self,
        observation: &FontObservation,
        digester: &mut D,
    ) -> Result<(), BrowserVisibleMismatch> {
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
        let Some(observed_sha256) = canonical_font_set_sha256(&families, digester) else {
            return Err(BrowserVisibleMismatch::FontSet);
        };
        if observed_sha256 != self.fonts().font_set_sha256() {
            return Err(BrowserVisibleMismatch::FontSet);
        }
        Ok(())
    }

    fn compare_locale<D: BrowserVisibleSha256Port>(
        &self,
        observation: &LocaleObservation,
        digester: &mut D,
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
        let Some(observed_languages) =
            canonical_navigator_languages_sha256(&observation.languages, digester)
        else {
            return Err(BrowserVisibleMismatch::Languages);
        };
        if observed_languages != expected.languages_sha256() {
            return Err(BrowserVisibleMismatch::Languages);
        }
        match (expected.speech_voices_sha256(), &observation.speech_voices) {
            (None, SpeechVoicesObservation::NotApplicable) => Ok(()),
            (Some(expected_sha256), SpeechVoicesObservation::Available(voices)) => {
                let Some(observed_sha256) = canonical_speech_voices_sha256(voices, digester) else {
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

fn compare_extensions_digest<D: BrowserVisibleSha256Port>(
    expected: &str,
    webgl: &Observed<Vec<String>>,
    webgl2: &Observed<Vec<String>>,
    digester: &mut D,
) -> Result<(), BrowserVisibleMismatch> {
    let (Observed::Available(webgl), Observed::Available(webgl2)) = (webgl, webgl2) else {
        return Err(BrowserVisibleMismatch::WebGlExtensions);
    };
    let Some(observed) = canonical_webgl_extensions_sha256(webgl, webgl2, digester) else {
        return Err(BrowserVisibleMismatch::WebGlExtensions);
    };
    if observed == expected {
        Ok(())
    } else {
        Err(BrowserVisibleMismatch::WebGlExtensions)
    }
}

fn compare_key_values_digest<D, F>(
    expected: &str,
    observed: &Observed<Vec<BrowserKeyValueObservation>>,
    canonicalize: F,
    mismatch: BrowserVisibleMismatch,
    digester: &mut D,
) -> Result<(), BrowserVisibleMismatch>
where
    D: BrowserVisibleSha256Port,
    F: Fn(&[BrowserKeyValueObservation], &mut D) -> Option<String>,
{
    let Observed::Available(values) = observed else {
        return Err(mismatch);
    };
    if canonicalize(values, digester).as_deref() == Some(expected) {
        Ok(())
    } else {
        Err(mismatch)
    }
}

fn compare_context_pair_digest<D, F>(
    expected: &str,
    webgl: &Observed<Vec<BrowserKeyValueObservation>>,
    webgl2: &Observed<Vec<BrowserKeyValueObservation>>,
    canonicalize: F,
    mismatch: BrowserVisibleMismatch,
    digester: &mut D,
) -> Result<(), BrowserVisibleMismatch>
where
    D: BrowserVisibleSha256Port,
    F: Fn(&[BrowserKeyValueObservation], &[BrowserKeyValueObservation], &mut D) -> Option<String>,
{
    let (Observed::Available(webgl), Observed::Available(webgl2)) = (webgl, webgl2) else {
        return Err(mismatch);
    };
    if canonicalize(webgl, webgl2, digester).as_deref() == Some(expected) {
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

/// Canonical Profile-Stable digest for the combined WebGL1/WebGL2 extension surface.
pub fn canonical_webgl_extensions_sha256<D: BrowserVisibleSha256Port>(
    webgl: &[String],
    webgl2: &[String],
    digester: &mut D,
) -> Option<String> {
    let mut canonical = domain_bytes("webgl-extensions-v1");
    if !append_sorted_strings(&mut canonical, "webgl", webgl, true)
        || !append_sorted_strings(&mut canonical, "webgl2", webgl2, true)
    {
        return None;
    }
    digest_canonical(&canonical, digester)
}

pub fn canonical_webgl_parameters_sha256<D: BrowserVisibleSha256Port>(
    values: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    canonical_key_values_sha256("webgl-parameters-v1", values, digester)
}

pub fn canonical_webgl2_parameters_sha256<D: BrowserVisibleSha256Port>(
    values: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    canonical_key_values_sha256("webgl2-parameters-v1", values, digester)
}

pub fn canonical_shader_precision_sha256<D: BrowserVisibleSha256Port>(
    webgl: &[BrowserKeyValueObservation],
    webgl2: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    canonical_context_pair_sha256("webgl-shader-precision-v1", webgl, webgl2, digester)
}

pub fn canonical_context_attributes_sha256<D: BrowserVisibleSha256Port>(
    webgl: &[BrowserKeyValueObservation],
    webgl2: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    canonical_context_pair_sha256("webgl-context-attributes-v1", webgl, webgl2, digester)
}

pub fn canonical_font_set_sha256<D: BrowserVisibleSha256Port>(
    families: &[String],
    digester: &mut D,
) -> Option<String> {
    if families.is_empty() {
        return None;
    }
    let mut canonical = domain_bytes("font-set-v1");
    if !append_sorted_strings(&mut canonical, "families", families, false) {
        return None;
    }
    digest_canonical(&canonical, digester)
}

pub fn canonical_navigator_languages_sha256<D: BrowserVisibleSha256Port>(
    languages: &[String],
    digester: &mut D,
) -> Option<String> {
    if languages.is_empty()
        || languages.len() > MAX_COLLECTION_ROWS
        || languages.iter().any(|value| !bounded_text(value))
    {
        return None;
    }
    let mut canonical = domain_bytes("navigator-languages-v1");
    append_component(&mut canonical, languages.len().to_string().as_str());
    for language in languages {
        append_component(&mut canonical, language);
    }
    digest_canonical(&canonical, digester)
}

pub fn canonical_speech_voices_sha256<D: BrowserVisibleSha256Port>(
    voices: &[SpeechVoiceObservation],
    digester: &mut D,
) -> Option<String> {
    if voices.len() > MAX_COLLECTION_ROWS
        || voices.iter().any(|voice| {
            !bounded_text(&voice.language)
                || !bounded_text(&voice.name)
                || !bounded_text(&voice.voice_uri)
        })
    {
        return None;
    }
    let mut sorted = voices.to_vec();
    sorted.sort();
    let mut canonical = domain_bytes("speech-voices-v2");
    append_component(&mut canonical, sorted.len().to_string().as_str());
    for voice in sorted {
        append_component(&mut canonical, &voice.language);
        append_component(&mut canonical, &voice.name);
        append_component(&mut canonical, &voice.voice_uri);
        append_component(&mut canonical, if voice.local_service { "1" } else { "0" });
        append_component(&mut canonical, if voice.is_default { "1" } else { "0" });
    }
    digest_canonical(&canonical, digester)
}

/// Profile-Stable generation seed digests are separate from browser-output evidence.
pub fn canonical_font_spacing_seed_sha256<D: BrowserVisibleSha256Port>(
    seed: &BrowserValue,
    digester: &mut D,
) -> Option<String> {
    canonical_seed_sha256("font-spacing-seed-v1", seed, digester)
}

pub fn canonical_canvas_seed_sha256<D: BrowserVisibleSha256Port>(
    seed: &BrowserValue,
    digester: &mut D,
) -> Option<String> {
    canonical_seed_sha256("canvas-seed-v1", seed, digester)
}

pub fn canonical_audio_seed_sha256<D: BrowserVisibleSha256Port>(
    seed: &BrowserValue,
    digester: &mut D,
) -> Option<String> {
    canonical_seed_sha256("audio-seed-v1", seed, digester)
}

fn canonical_seed_sha256<D: BrowserVisibleSha256Port>(
    domain: &str,
    seed: &BrowserValue,
    digester: &mut D,
) -> Option<String> {
    let mut canonical = domain_bytes(domain);
    if !append_browser_value(&mut canonical, seed) {
        return None;
    }
    digest_canonical(&canonical, digester)
}

fn canonical_context_pair_sha256<D: BrowserVisibleSha256Port>(
    domain: &str,
    webgl: &[BrowserKeyValueObservation],
    webgl2: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    let mut canonical = domain_bytes(domain);
    if !append_key_values(&mut canonical, "webgl", webgl, true)
        || !append_key_values(&mut canonical, "webgl2", webgl2, true)
    {
        return None;
    }
    digest_canonical(&canonical, digester)
}

fn canonical_key_values_sha256<D: BrowserVisibleSha256Port>(
    domain: &str,
    values: &[BrowserKeyValueObservation],
    digester: &mut D,
) -> Option<String> {
    let mut canonical = domain_bytes(domain);
    if !append_key_values(&mut canonical, "values", values, false) {
        return None;
    }
    digest_canonical(&canonical, digester)
}

fn append_sorted_strings(
    canonical: &mut Vec<u8>,
    label: &str,
    values: &[String],
    allow_empty: bool,
) -> bool {
    if (!allow_empty && values.is_empty())
        || values.len() > MAX_COLLECTION_ROWS
        || values.iter().any(|value| !bounded_text(value))
    {
        return false;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return false;
    }
    append_component(canonical, label);
    append_component(canonical, sorted.len().to_string().as_str());
    for value in sorted {
        append_component(canonical, &value);
    }
    true
}

fn append_key_values(
    canonical: &mut Vec<u8>,
    label: &str,
    values: &[BrowserKeyValueObservation],
    allow_empty: bool,
) -> bool {
    if (!allow_empty && values.is_empty())
        || values.len() > MAX_COLLECTION_ROWS
        || values.iter().any(|row| !bounded_text(&row.name))
    {
        return false;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.name.cmp(&right.name));
    if sorted.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return false;
    }
    append_component(canonical, label);
    append_component(canonical, sorted.len().to_string().as_str());
    for row in sorted {
        append_component(canonical, &row.name);
        if !append_browser_value(canonical, &row.value) {
            return false;
        }
    }
    true
}

fn append_browser_value(canonical: &mut Vec<u8>, value: &BrowserValue) -> bool {
    match value {
        BrowserValue::Null => append_component(canonical, "null"),
        BrowserValue::Bool(value) => {
            append_component(canonical, "bool");
            append_component(canonical, if *value { "1" } else { "0" });
        }
        BrowserValue::Number(value) => {
            let Some(canonical_number) = value
                .parse::<f64>()
                .ok()
                .and_then(crate::canonical_browser_number)
            else {
                return false;
            };
            append_component(canonical, "number");
            append_component(canonical, &canonical_number);
        }
        BrowserValue::Text(value) => {
            if !bounded_text(value) {
                return false;
            }
            append_component(canonical, "text");
            append_component(canonical, value);
        }
        BrowserValue::List(values) => {
            if values.len() > MAX_COLLECTION_ROWS {
                return false;
            }
            append_component(canonical, "list");
            append_component(canonical, values.len().to_string().as_str());
            for value in values {
                if !append_browser_value(canonical, value) {
                    return false;
                }
            }
        }
        BrowserValue::Map(values) => {
            append_component(canonical, "map");
            if !append_key_values(canonical, "entries", values, true) {
                return false;
            }
        }
    }
    true
}

fn domain_bytes(domain: &str) -> Vec<u8> {
    let mut canonical = Vec::new();
    append_component(&mut canonical, domain);
    canonical
}

fn append_component(canonical: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    canonical.extend_from_slice(bytes.len().to_string().as_bytes());
    canonical.push(b':');
    canonical.extend_from_slice(bytes);
    canonical.push(b';');
}

fn digest_canonical<D: BrowserVisibleSha256Port>(
    canonical: &[u8],
    digester: &mut D,
) -> Option<String> {
    digester.sha256(canonical).ok().map(encode_digest)
}

fn encode_digest(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
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
    use core::convert::Infallible;

    #[derive(Default)]
    struct TestSha256;

    impl BrowserVisibleSha256Port for TestSha256 {
        type Error = Infallible;

        fn sha256(&mut self, canonical_bytes: &[u8]) -> Result<[u8; 32], Self::Error> {
            let mut digest = [0_u8; 32];
            for (index, byte) in canonical_bytes.iter().copied().enumerate() {
                let slot = index % digest.len();
                digest[slot] = digest[slot].wrapping_mul(31).wrapping_add(byte);
            }
            let length_byte = canonical_bytes.len().to_le_bytes()[0];
            for byte in &mut digest {
                *byte ^= length_byte;
            }
            Ok(digest)
        }
    }

    fn value(number: &str) -> BrowserValue {
        BrowserValue::Number(number.to_owned())
    }

    fn key_values(prefix: &str) -> Vec<BrowserKeyValueObservation> {
        vec![
            BrowserKeyValueObservation {
                name: format!("{prefix}.a"),
                value: value("1"),
            },
            BrowserKeyValueObservation {
                name: format!("{prefix}.b"),
                value: value("2"),
            },
        ]
    }

    fn fixture(
        device_memory_gib: Option<u16>,
        voices_applicable: bool,
    ) -> Result<(ProfileStableIdentity, BrowserVisibleObservation), Box<dyn std::error::Error>>
    {
        let webgl_extensions = vec!["EXT_beta".to_owned(), "EXT_alpha".to_owned()];
        let webgl2_extensions = vec!["EXT_gamma".to_owned()];
        let webgl = key_values("webgl");
        let webgl2 = key_values("webgl2");
        let shader = key_values("shader");
        let shader2 = key_values("shader2");
        let context = key_values("context");
        let context2 = key_values("context2");
        let fonts = vec!["Arial".to_owned(), "Segoe UI".to_owned()];
        let languages = vec!["en-US".to_owned(), "en".to_owned()];
        let voices = vec![SpeechVoiceObservation {
            language: "en-US".to_owned(),
            name: "Example Voice".to_owned(),
            voice_uri: "urn:voice:example".to_owned(),
            local_service: true,
            is_default: true,
        }];
        let mut digester = TestSha256;
        let voice_hash = if voices_applicable {
            canonical_speech_voices_sha256(&voices, &mut digester)
        } else {
            None
        };
        let stable = ProfileStableIdentity::new(
            2,
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
                canonical_webgl_extensions_sha256(
                    &webgl_extensions,
                    &webgl2_extensions,
                    &mut digester,
                )
                .ok_or("extensions")?,
                canonical_webgl_parameters_sha256(&webgl, &mut digester).ok_or("webgl")?,
                canonical_webgl2_parameters_sha256(&webgl2, &mut digester).ok_or("webgl2")?,
                canonical_shader_precision_sha256(&shader, &shader2, &mut digester)
                    .ok_or("shader")?,
                canonical_context_attributes_sha256(&context, &context2, &mut digester)
                    .ok_or("context")?,
            )?,
            FontIdentity::new(
                canonical_font_set_sha256(&fonts, &mut digester).ok_or("fonts")?,
                canonical_font_spacing_seed_sha256(&value("7"), &mut digester).ok_or("spacing")?,
            )?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                canonical_canvas_seed_sha256(&value("8"), &mut digester).ok_or("canvas")?,
                canonical_audio_seed_sha256(&value("9"), &mut digester).ok_or("audio")?,
            )?,
            LocaleIdentity::new(
                "en-US",
                canonical_navigator_languages_sha256(&languages, &mut digester)
                    .ok_or("languages")?,
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
                webgl_extensions: Observed::Available(webgl_extensions),
                webgl2_extensions: Observed::Available(webgl2_extensions),
                webgl_parameters: Observed::Available(webgl),
                webgl2_parameters: Observed::Available(webgl2),
                webgl_shader_precision: Observed::Available(shader),
                webgl2_shader_precision: Observed::Available(shader2),
                webgl_context_attributes: Observed::Available(context),
                webgl2_context_attributes: Observed::Available(context2),
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
        assert_eq!(
            expected.compare_browser_visible(&observation, &mut TestSha256),
            Ok(())
        );
        Ok(())
    }

    #[test]
    fn speech_voice_uri_drift_is_identity_bearing() -> Result<(), Box<dyn std::error::Error>> {
        let (expected, mut observation) = fixture(Some(8), true)?;
        let SpeechVoicesObservation::Available(voices) = &mut observation.locale.speech_voices
        else {
            return Err("voices must be applicable".into());
        };
        voices[0].voice_uri = "urn:voice:other".to_owned();
        assert_eq!(
            expected.compare_browser_visible(&observation, &mut TestSha256),
            Err(BrowserVisibleMismatch::SpeechVoices)
        );
        Ok(())
    }

    #[test]
    fn webgl2_is_not_hidden_inside_adapter_owned_hashing() -> Result<(), Box<dyn std::error::Error>>
    {
        let (expected, mut observation) = fixture(Some(8), true)?;
        if let Observed::Available(extensions) = &mut observation.graphics.webgl2_extensions {
            extensions.push("EXT_other".to_owned());
        }
        assert_eq!(
            expected.compare_browser_visible(&observation, &mut TestSha256),
            Err(BrowserVisibleMismatch::WebGlExtensions)
        );
        Ok(())
    }

    #[test]
    fn recursive_value_canonicalization_is_order_stable_and_typed() {
        let mut left = TestSha256;
        let mut right = TestSha256;
        let first = vec![
            BrowserKeyValueObservation {
                name: "b".to_owned(),
                value: BrowserValue::Bool(true),
            },
            BrowserKeyValueObservation {
                name: "a".to_owned(),
                value: BrowserValue::List(vec![value("1"), BrowserValue::Text("x".to_owned())]),
            },
        ];
        let second = vec![first[1].clone(), first[0].clone()];
        assert_eq!(
            canonical_webgl_parameters_sha256(&first, &mut left),
            canonical_webgl_parameters_sha256(&second, &mut right)
        );
    }

    #[test]
    fn equivalent_browser_number_spellings_have_one_identity_hash() {
        let digest = |number: &str| {
            canonical_webgl_parameters_sha256(
                &[BrowserKeyValueObservation {
                    name: "number".to_owned(),
                    value: value(number),
                }],
                &mut TestSha256,
            )
        };
        assert_eq!(digest("1"), digest("1.0"));
        assert_eq!(digest("1"), digest("1e0"));
        assert_eq!(digest("0"), digest("-0"));
        assert_ne!(digest("1"), digest("1.5"));
        assert_eq!(digest("NaN"), None);
        assert_eq!(digest("inf"), None);
    }

    #[test]
    fn optional_states_are_explicit_not_silent_skips() -> Result<(), Box<dyn std::error::Error>> {
        let (expected, observation) = fixture(None, false)?;
        assert_eq!(
            expected.compare_browser_visible(&observation, &mut TestSha256),
            Ok(())
        );

        let mut wrong_memory = observation.clone();
        wrong_memory.hardware.device_memory_gib = Observed::Available(8);
        assert_eq!(
            expected.compare_browser_visible(&wrong_memory, &mut TestSha256),
            Err(BrowserVisibleMismatch::DeviceMemory)
        );

        let mut unavailable_voices = observation;
        unavailable_voices.locale.speech_voices = SpeechVoicesObservation::Unavailable;
        assert_eq!(
            expected.compare_browser_visible(&unavailable_voices, &mut TestSha256),
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
            assert_eq!(
                expected.compare_browser_visible(&value, &mut TestSha256),
                Err(mismatch)
            );
        }
        Ok(())
    }
}
