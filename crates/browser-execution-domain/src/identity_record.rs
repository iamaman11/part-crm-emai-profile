use super::{
    BrowserExecutionError, BrowserIdentityManifest, BrowserOsIdentity, DisplayIdentity,
    FontIdentity, GraphicsIdentity, HardwareCapabilityIdentity, LocaleIdentity,
    OriginDeterminismMode, OriginDeterministicIdentity, ProfileStableIdentity,
};
use std::collections::BTreeMap;

const RECORD_SCHEMA: &str = "profile-stable-browser-identity-v1";
const RECORD_FIELD_COUNT: usize = 39;
const NONE: &str = "none";

impl BrowserIdentityManifest {
    /// Canonical generation-owned projection of the browser identity semantic owner.
    ///
    /// This record is deliberately text-only, ordered and versioned so it can be snapshotted with
    /// the profile generation and reproduced byte-for-byte on another authorized host. Runtime
    /// configuration and local materialization evidence are projections of this record; they are
    /// not independent fingerprint authorities.
    #[must_use]
    pub fn to_canonical_record(&self) -> String {
        let stable = self.profile_stable_identity();
        let browser = stable.browser_os();
        let hardware = stable.hardware();
        let display = stable.display();
        let graphics = stable.graphics();
        let fonts = stable.fonts();
        let origin = stable.origin_deterministic();
        let locale = stable.locale();
        let origin_mode = match origin.mode() {
            OriginDeterminismMode::ProfileGenerationSeed => "profile_generation_seed",
            OriginDeterminismMode::OriginHmac => "origin_hmac",
        };
        let speech = locale.speech_voices_sha256().unwrap_or(NONE);
        format!(
            concat!(
                "schema={}\n",
                "compatibility_version={}\n",
                "fingerprint_policy_version={}\n",
                "runtime_version={}\n",
                "runtime_inventory_sha256={}\n",
                "fingerprint_source={}\n",
                "fingerprint_config_sha256={}\n",
                "profile_schema_version={}\n",
                "browser_user_agent={}\n",
                "browser_major={}\n",
                "browser_platform={}\n",
                "browser_oscpu={}\n",
                "hardware_concurrency={}\n",
                "device_memory_gib={}\n",
                "max_touch_points={}\n",
                "display_width={}\n",
                "display_height={}\n",
                "display_avail_width={}\n",
                "display_avail_height={}\n",
                "display_avail_left={}\n",
                "display_avail_top={}\n",
                "display_color_depth={}\n",
                "display_pixel_depth={}\n",
                "display_dpr_milli={}\n",
                "webgl_vendor={}\n",
                "webgl_renderer={}\n",
                "webgl_extensions_sha256={}\n",
                "webgl_parameters_sha256={}\n",
                "webgl2_parameters_sha256={}\n",
                "shader_precision_sha256={}\n",
                "context_attributes_sha256={}\n",
                "font_set_sha256={}\n",
                "spacing_seed_sha256={}\n",
                "origin_mode={}\n",
                "canvas_seed_sha256={}\n",
                "audio_seed_sha256={}\n",
                "language={}\n",
                "languages_sha256={}\n",
                "speech_voices_sha256={}\n"
            ),
            RECORD_SCHEMA,
            self.compatibility_version(),
            self.fingerprint_policy_version(),
            self.runtime_version(),
            self.runtime_inventory_sha256(),
            self.fingerprint_source(),
            self.fingerprint_config_sha256(),
            stable.schema_version(),
            browser.user_agent(),
            browser.browser_major(),
            browser.platform(),
            browser.oscpu(),
            hardware.hardware_concurrency(),
            hardware.device_memory_gib(),
            hardware.max_touch_points(),
            display.width(),
            display.height(),
            display.avail_width(),
            display.avail_height(),
            display.avail_left(),
            display.avail_top(),
            display.color_depth(),
            display.pixel_depth(),
            display.device_pixel_ratio_milli(),
            graphics.webgl_vendor(),
            graphics.webgl_renderer(),
            graphics.webgl_extensions_sha256(),
            graphics.webgl_parameters_sha256(),
            graphics.webgl2_parameters_sha256(),
            graphics.shader_precision_sha256(),
            graphics.context_attributes_sha256(),
            fonts.font_set_sha256(),
            fonts.spacing_seed_sha256(),
            origin_mode,
            origin.canvas_seed_sha256(),
            origin.audio_seed_sha256(),
            locale.language(),
            locale.languages_sha256(),
            speech,
        )
    }

    pub fn from_canonical_record(record: &str) -> Result<Self, BrowserExecutionError> {
        let values = parse_record(record)?;
        if values.len() != RECORD_FIELD_COUNT || required(&values, "schema")? != RECORD_SCHEMA {
            return Err(BrowserExecutionError::InvalidIdentityToken);
        }
        let compatibility_version = parse_number(&values, "compatibility_version")?;
        let profile_schema_version = parse_number(&values, "profile_schema_version")?;
        let browser_major = parse_number(&values, "browser_major")?;
        let hardware_concurrency = parse_number(&values, "hardware_concurrency")?;
        let device_memory_gib = parse_number(&values, "device_memory_gib")?;
        let max_touch_points = parse_number(&values, "max_touch_points")?;
        let display_width = parse_number(&values, "display_width")?;
        let display_height = parse_number(&values, "display_height")?;
        let display_avail_width = parse_number(&values, "display_avail_width")?;
        let display_avail_height = parse_number(&values, "display_avail_height")?;
        let display_avail_left = parse_number(&values, "display_avail_left")?;
        let display_avail_top = parse_number(&values, "display_avail_top")?;
        let display_color_depth = parse_number(&values, "display_color_depth")?;
        let display_pixel_depth = parse_number(&values, "display_pixel_depth")?;
        let display_dpr_milli = parse_number(&values, "display_dpr_milli")?;
        let origin_mode = match required(&values, "origin_mode")? {
            "profile_generation_seed" => OriginDeterminismMode::ProfileGenerationSeed,
            "origin_hmac" => OriginDeterminismMode::OriginHmac,
            _ => return Err(BrowserExecutionError::InvalidIdentityToken),
        };
        let speech = match required(&values, "speech_voices_sha256")? {
            NONE => None,
            value => Some(value.to_owned()),
        };
        let stable = ProfileStableIdentity::new(
            profile_schema_version,
            BrowserOsIdentity::new(
                required(&values, "browser_user_agent")?,
                browser_major,
                required(&values, "browser_platform")?,
                required(&values, "browser_oscpu")?,
            )?,
            HardwareCapabilityIdentity::new(
                hardware_concurrency,
                device_memory_gib,
                max_touch_points,
            )?,
            DisplayIdentity::new(
                display_width,
                display_height,
                display_avail_width,
                display_avail_height,
                display_avail_left,
                display_avail_top,
                display_color_depth,
                display_pixel_depth,
                display_dpr_milli,
            )?,
            GraphicsIdentity::new(
                required(&values, "webgl_vendor")?,
                required(&values, "webgl_renderer")?,
                required(&values, "webgl_extensions_sha256")?,
                required(&values, "webgl_parameters_sha256")?,
                required(&values, "webgl2_parameters_sha256")?,
                required(&values, "shader_precision_sha256")?,
                required(&values, "context_attributes_sha256")?,
            )?,
            FontIdentity::new(
                required(&values, "font_set_sha256")?,
                required(&values, "spacing_seed_sha256")?,
            )?,
            OriginDeterministicIdentity::new(
                origin_mode,
                required(&values, "canvas_seed_sha256")?,
                required(&values, "audio_seed_sha256")?,
            )?,
            LocaleIdentity::new(
                required(&values, "language")?,
                required(&values, "languages_sha256")?,
                speech,
            )?,
        )?;
        Self::new(
            compatibility_version,
            required(&values, "fingerprint_policy_version")?,
            required(&values, "runtime_version")?,
            required(&values, "runtime_inventory_sha256")?,
            required(&values, "fingerprint_source")?,
            required(&values, "fingerprint_config_sha256")?,
            stable,
        )
    }
}

fn parse_record(record: &str) -> Result<BTreeMap<&str, &str>, BrowserExecutionError> {
    if !record.ends_with('\n') || record.contains('\r') || record.contains('\0') {
        return Err(BrowserExecutionError::InvalidIdentityToken);
    }
    let mut values = BTreeMap::new();
    for line in record.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or(BrowserExecutionError::InvalidIdentityToken)?;
        if key.is_empty() || value.is_empty() || values.insert(key, value).is_some() {
            return Err(BrowserExecutionError::InvalidIdentityToken);
        }
    }
    Ok(values)
}

fn required<'a>(
    values: &'a BTreeMap<&str, &str>,
    key: &str,
) -> Result<&'a str, BrowserExecutionError> {
    values
        .get(key)
        .copied()
        .ok_or(BrowserExecutionError::InvalidIdentityToken)
}

fn parse_number<T>(values: &BTreeMap<&str, &str>, key: &str) -> Result<T, BrowserExecutionError>
where
    T: core::str::FromStr,
{
    required(values, key)?
        .parse::<T>()
        .map_err(|_| BrowserExecutionError::InvalidIdentityToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn identity() -> Result<BrowserIdentityManifest, Box<dyn std::error::Error>> {
        Ok(BrowserIdentityManifest::new(
            2,
            "profile-stability-v1",
            "2.0.0",
            digest('a'),
            "profile-stability-v1-probe-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            digest('c'),
            ProfileStableIdentity::new(
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
                LocaleIdentity::new("en-US", digest('d'), Some(digest('e')))?,
            )?,
        )?)
    }

    #[test]
    fn canonical_record_round_trips_without_semantic_loss() -> Result<(), Box<dyn std::error::Error>>
    {
        let identity = identity()?;
        let record = identity.to_canonical_record();
        assert_eq!(
            BrowserIdentityManifest::from_canonical_record(&record)?,
            identity
        );
        assert_eq!(identity.to_canonical_record(), record);
        Ok(())
    }

    #[test]
    fn canonical_record_rejects_unknown_or_missing_fields() -> Result<(), Box<dyn std::error::Error>>
    {
        let record = identity()?.to_canonical_record();
        assert!(
            BrowserIdentityManifest::from_canonical_record(
                &record.replace("language=en-US\n", "language=en-US\nunexpected=value\n")
            )
            .is_err()
        );
        assert!(
            BrowserIdentityManifest::from_canonical_record(&record.replace("language=en-US\n", ""))
                .is_err()
        );
        Ok(())
    }
}
