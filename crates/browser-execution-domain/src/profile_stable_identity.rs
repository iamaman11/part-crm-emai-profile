use super::BrowserExecutionError;

const MAX_USER_AGENT_BYTES: usize = 512;
const MAX_VISIBLE_TEXT_BYTES: usize = 256;
const MAX_LANGUAGE_BYTES: usize = 64;
const MIN_DPR_MILLI: u32 = 250;
const MAX_DPR_MILLI: u32 = 8_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileStableIdentity {
    schema_version: u32,
    browser_os: BrowserOsIdentity,
    hardware: HardwareCapabilityIdentity,
    display: DisplayIdentity,
    graphics: GraphicsIdentity,
    fonts: FontIdentity,
    origin_deterministic: OriginDeterministicIdentity,
    locale: LocaleIdentity,
}

impl ProfileStableIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        schema_version: u32,
        browser_os: BrowserOsIdentity,
        hardware: HardwareCapabilityIdentity,
        display: DisplayIdentity,
        graphics: GraphicsIdentity,
        fonts: FontIdentity,
        origin_deterministic: OriginDeterministicIdentity,
        locale: LocaleIdentity,
    ) -> Result<Self, BrowserExecutionError> {
        if schema_version == 0 {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            schema_version,
            browser_os,
            hardware,
            display,
            graphics,
            fonts,
            origin_deterministic,
            locale,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn browser_os(&self) -> &BrowserOsIdentity {
        &self.browser_os
    }

    #[must_use]
    pub const fn hardware(&self) -> &HardwareCapabilityIdentity {
        &self.hardware
    }

    #[must_use]
    pub const fn display(&self) -> &DisplayIdentity {
        &self.display
    }

    #[must_use]
    pub const fn graphics(&self) -> &GraphicsIdentity {
        &self.graphics
    }

    #[must_use]
    pub const fn fonts(&self) -> &FontIdentity {
        &self.fonts
    }

    #[must_use]
    pub const fn origin_deterministic(&self) -> &OriginDeterministicIdentity {
        &self.origin_deterministic
    }

    #[must_use]
    pub const fn locale(&self) -> &LocaleIdentity {
        &self.locale
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserOsIdentity {
    user_agent: String,
    browser_major: u16,
    platform: String,
    oscpu: String,
}

impl BrowserOsIdentity {
    pub fn new(
        user_agent: impl Into<String>,
        browser_major: u16,
        platform: impl Into<String>,
        oscpu: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let user_agent = user_agent.into();
        let platform = platform.into();
        let oscpu = oscpu.into();
        if browser_major == 0
            || !valid_visible_text(&user_agent, MAX_USER_AGENT_BYTES)
            || !valid_visible_text(&platform, MAX_VISIBLE_TEXT_BYTES)
            || !valid_visible_text(&oscpu, MAX_VISIBLE_TEXT_BYTES)
        {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            user_agent,
            browser_major,
            platform,
            oscpu,
        })
    }

    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    #[must_use]
    pub const fn browser_major(&self) -> u16 {
        self.browser_major
    }

    #[must_use]
    pub fn platform(&self) -> &str {
        &self.platform
    }

    #[must_use]
    pub fn oscpu(&self) -> &str {
        &self.oscpu
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardwareCapabilityIdentity {
    hardware_concurrency: u16,
    device_memory_gib: Option<u16>,
    max_touch_points: u16,
}

impl HardwareCapabilityIdentity {
    pub fn new(
        hardware_concurrency: u16,
        device_memory_gib: impl Into<Option<u16>>,
        max_touch_points: u16,
    ) -> Result<Self, BrowserExecutionError> {
        let device_memory_gib = device_memory_gib.into();
        if hardware_concurrency == 0
            || hardware_concurrency > 1_024
            || device_memory_gib.is_some_and(|value| value == 0 || value > 1_024)
            || max_touch_points > 64
        {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            hardware_concurrency,
            device_memory_gib,
            max_touch_points,
        })
    }

    #[must_use]
    pub const fn hardware_concurrency(self) -> u16 {
        self.hardware_concurrency
    }

    #[must_use]
    pub const fn device_memory_gib(self) -> Option<u16> {
        self.device_memory_gib
    }

    #[must_use]
    pub const fn max_touch_points(self) -> u16 {
        self.max_touch_points
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayIdentity {
    width: u32,
    height: u32,
    avail_width: u32,
    avail_height: u32,
    avail_left: i32,
    avail_top: i32,
    color_depth: u16,
    pixel_depth: u16,
    device_pixel_ratio_milli: u32,
}

impl DisplayIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u32,
        height: u32,
        avail_width: u32,
        avail_height: u32,
        avail_left: i32,
        avail_top: i32,
        color_depth: u16,
        pixel_depth: u16,
        device_pixel_ratio_milli: u32,
    ) -> Result<Self, BrowserExecutionError> {
        if width == 0
            || height == 0
            || avail_width == 0
            || avail_height == 0
            || avail_width > width
            || avail_height > height
            || !(8..=64).contains(&color_depth)
            || !(8..=64).contains(&pixel_depth)
            || !(MIN_DPR_MILLI..=MAX_DPR_MILLI).contains(&device_pixel_ratio_milli)
        {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            width,
            height,
            avail_width,
            avail_height,
            avail_left,
            avail_top,
            color_depth,
            pixel_depth,
            device_pixel_ratio_milli,
        })
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn avail_width(self) -> u32 {
        self.avail_width
    }

    #[must_use]
    pub const fn avail_height(self) -> u32 {
        self.avail_height
    }

    #[must_use]
    pub const fn avail_left(self) -> i32 {
        self.avail_left
    }

    #[must_use]
    pub const fn avail_top(self) -> i32 {
        self.avail_top
    }

    #[must_use]
    pub const fn color_depth(self) -> u16 {
        self.color_depth
    }

    #[must_use]
    pub const fn pixel_depth(self) -> u16 {
        self.pixel_depth
    }

    #[must_use]
    pub const fn device_pixel_ratio_milli(self) -> u32 {
        self.device_pixel_ratio_milli
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsIdentity {
    webgl_vendor: String,
    webgl_renderer: String,
    webgl_extensions_sha256: String,
    webgl_parameters_sha256: String,
    webgl2_parameters_sha256: String,
    shader_precision_sha256: String,
    context_attributes_sha256: String,
}

impl GraphicsIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        webgl_vendor: impl Into<String>,
        webgl_renderer: impl Into<String>,
        webgl_extensions_sha256: impl Into<String>,
        webgl_parameters_sha256: impl Into<String>,
        webgl2_parameters_sha256: impl Into<String>,
        shader_precision_sha256: impl Into<String>,
        context_attributes_sha256: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let webgl_vendor = webgl_vendor.into();
        let webgl_renderer = webgl_renderer.into();
        let webgl_extensions_sha256 = webgl_extensions_sha256.into();
        let webgl_parameters_sha256 = webgl_parameters_sha256.into();
        let webgl2_parameters_sha256 = webgl2_parameters_sha256.into();
        let shader_precision_sha256 = shader_precision_sha256.into();
        let context_attributes_sha256 = context_attributes_sha256.into();
        if !valid_visible_text(&webgl_vendor, MAX_VISIBLE_TEXT_BYTES)
            || !valid_visible_text(&webgl_renderer, MAX_VISIBLE_TEXT_BYTES)
            || !valid_sha256(&webgl_extensions_sha256)
            || !valid_sha256(&webgl_parameters_sha256)
            || !valid_sha256(&webgl2_parameters_sha256)
            || !valid_sha256(&shader_precision_sha256)
            || !valid_sha256(&context_attributes_sha256)
        {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            webgl_vendor,
            webgl_renderer,
            webgl_extensions_sha256,
            webgl_parameters_sha256,
            webgl2_parameters_sha256,
            shader_precision_sha256,
            context_attributes_sha256,
        })
    }

    #[must_use]
    pub fn webgl_vendor(&self) -> &str {
        &self.webgl_vendor
    }

    #[must_use]
    pub fn webgl_renderer(&self) -> &str {
        &self.webgl_renderer
    }

    #[must_use]
    pub fn webgl_extensions_sha256(&self) -> &str {
        &self.webgl_extensions_sha256
    }

    #[must_use]
    pub fn webgl_parameters_sha256(&self) -> &str {
        &self.webgl_parameters_sha256
    }

    #[must_use]
    pub fn webgl2_parameters_sha256(&self) -> &str {
        &self.webgl2_parameters_sha256
    }

    #[must_use]
    pub fn shader_precision_sha256(&self) -> &str {
        &self.shader_precision_sha256
    }

    #[must_use]
    pub fn context_attributes_sha256(&self) -> &str {
        &self.context_attributes_sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontIdentity {
    font_set_sha256: String,
    spacing_seed_sha256: String,
}

impl FontIdentity {
    pub fn new(
        font_set_sha256: impl Into<String>,
        spacing_seed_sha256: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let font_set_sha256 = font_set_sha256.into();
        let spacing_seed_sha256 = spacing_seed_sha256.into();
        if !valid_sha256(&font_set_sha256) || !valid_sha256(&spacing_seed_sha256) {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            font_set_sha256,
            spacing_seed_sha256,
        })
    }

    #[must_use]
    pub fn font_set_sha256(&self) -> &str {
        &self.font_set_sha256
    }

    #[must_use]
    pub fn spacing_seed_sha256(&self) -> &str {
        &self.spacing_seed_sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginDeterminismMode {
    ProfileGenerationSeed,
    OriginHmac,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginDeterministicIdentity {
    mode: OriginDeterminismMode,
    canvas_seed_sha256: String,
    audio_seed_sha256: String,
}

impl OriginDeterministicIdentity {
    pub fn new(
        mode: OriginDeterminismMode,
        canvas_seed_sha256: impl Into<String>,
        audio_seed_sha256: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let canvas_seed_sha256 = canvas_seed_sha256.into();
        let audio_seed_sha256 = audio_seed_sha256.into();
        if !valid_sha256(&canvas_seed_sha256) || !valid_sha256(&audio_seed_sha256) {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            mode,
            canvas_seed_sha256,
            audio_seed_sha256,
        })
    }

    #[must_use]
    pub const fn mode(&self) -> OriginDeterminismMode {
        self.mode
    }

    #[must_use]
    pub fn canvas_seed_sha256(&self) -> &str {
        &self.canvas_seed_sha256
    }

    #[must_use]
    pub fn audio_seed_sha256(&self) -> &str {
        &self.audio_seed_sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleIdentity {
    language: String,
    languages_sha256: String,
    speech_voices_sha256: Option<String>,
}

impl LocaleIdentity {
    pub fn new(
        language: impl Into<String>,
        languages_sha256: impl Into<String>,
        speech_voices_sha256: Option<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let language = language.into();
        let languages_sha256 = languages_sha256.into();
        if !valid_visible_text(&language, MAX_LANGUAGE_BYTES)
            || !valid_sha256(&languages_sha256)
            || speech_voices_sha256
                .as_deref()
                .is_some_and(|value| !valid_sha256(value))
        {
            return Err(BrowserExecutionError::InvalidProfileStableIdentity);
        }
        Ok(Self {
            language,
            languages_sha256,
            speech_voices_sha256,
        })
    }

    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    #[must_use]
    pub fn languages_sha256(&self) -> &str {
        &self.languages_sha256
    }

    #[must_use]
    pub fn speech_voices_sha256(&self) -> Option<&str> {
        self.speech_voices_sha256.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileStateKind {
    Cookies,
    LocalStorage,
    IndexedDb,
    OrdinaryFirefoxState,
    DeviceAuthenticationKey,
    ProxyCredential,
    WindowsHello,
    PlatformPasskey,
    HardwareBackedPrivateKey,
    ClientCertificatePrivateKey,
    NativeIntegration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileStatePortability {
    Portable,
    RebindRequired,
    DeviceBoundUnsupported,
}

impl ProfileStateKind {
    #[must_use]
    pub const fn portability(self) -> ProfileStatePortability {
        match self {
            Self::Cookies | Self::LocalStorage | Self::IndexedDb | Self::OrdinaryFirefoxState => {
                ProfileStatePortability::Portable
            }
            Self::DeviceAuthenticationKey | Self::ProxyCredential => {
                ProfileStatePortability::RebindRequired
            }
            Self::WindowsHello
            | Self::PlatformPasskey
            | Self::HardwareBackedPrivateKey
            | Self::ClientCertificatePrivateKey
            | Self::NativeIntegration => ProfileStatePortability::DeviceBoundUnsupported,
        }
    }
}

fn valid_visible_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && !value
            .bytes()
            .any(|byte| byte == b'\0' || byte == b'\r' || byte == b'\n' || byte.is_ascii_control())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserOsIdentity, DisplayIdentity, FontIdentity, GraphicsIdentity,
        HardwareCapabilityIdentity, LocaleIdentity, OriginDeterminismMode,
        OriginDeterministicIdentity, ProfileStableIdentity, ProfileStateKind,
        ProfileStatePortability,
    };

    fn digest(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn identity() -> Result<ProfileStableIdentity, Box<dyn std::error::Error>> {
        Ok(ProfileStableIdentity::new(
            1,
            BrowserOsIdentity::new(
                "Mozilla/5.0 Firefox/152.0",
                152,
                "Win32",
                "Windows NT 10.0; Win64; x64",
            )?,
            HardwareCapabilityIdentity::new(8, 8, 0)?,
            DisplayIdentity::new(1920, 1080, 1920, 1040, 0, 0, 24, 24, 1000)?,
            GraphicsIdentity::new(
                "Google Inc. (NVIDIA)",
                "ANGLE (NVIDIA GeForce)",
                digest('1'),
                digest('2'),
                digest('3'),
                digest('4'),
                digest('5'),
            )?,
            FontIdentity::new(digest('6'), digest('7'))?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                digest('8'),
                digest('9'),
            )?,
            LocaleIdentity::new("en-US", digest('a'), Some(digest('b')))?,
        )?)
    }

    #[test]
    fn typed_identity_is_structural_not_one_opaque_hash() -> Result<(), Box<dyn std::error::Error>>
    {
        let identity = identity()?;
        assert_eq!(identity.browser_os().browser_major(), 152);
        assert_eq!(identity.hardware().hardware_concurrency(), 8);
        assert_eq!(identity.hardware().device_memory_gib(), Some(8));
        assert_eq!(identity.display().device_pixel_ratio_milli(), 1000);
        assert_eq!(identity.graphics().webgl_extensions_sha256(), digest('1'));
        assert_eq!(identity.locale().language(), "en-US");
        Ok(())
    }

    #[test]
    fn unavailable_device_memory_is_explicitly_representable() {
        assert!(HardwareCapabilityIdentity::new(8, None, 0).is_ok());
        assert!(HardwareCapabilityIdentity::new(8, Some(0), 0).is_err());
    }

    #[test]
    fn impossible_display_or_digest_fails_closed() {
        assert!(DisplayIdentity::new(1920, 1080, 0, 1040, 0, 0, 24, 24, 1000).is_err());
        assert!(FontIdentity::new("not-a-digest", digest('a')).is_err());
    }

    #[test]
    fn browser_state_portability_is_explicit() {
        assert_eq!(
            ProfileStateKind::Cookies.portability(),
            ProfileStatePortability::Portable
        );
        assert_eq!(
            ProfileStateKind::ProxyCredential.portability(),
            ProfileStatePortability::RebindRequired
        );
        assert_eq!(
            ProfileStateKind::WindowsHello.portability(),
            ProfileStatePortability::DeviceBoundUnsupported
        );
    }
}
