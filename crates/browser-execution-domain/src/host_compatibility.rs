use std::collections::BTreeSet;

const MIN_SUPPORTED_CLOCK_UNIX_MS: u64 = 1_704_067_200_000; // 2024-01-01T00:00:00Z
const MAX_SUPPORTED_CLOCK_UNIX_MS: u64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z
const MAX_DISPLAY_EDGE: u32 = 32_768;
const MIN_DPR_MILLI: u32 = 250;
const MAX_DPR_MILLI: u32 = 8_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostPlatformClass {
    Windows,
    Linux,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostArchitecture {
    X86_64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostRuntimeClass {
    PackagedCamoufox,
    RepositoryPinnedCamoufox,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostExecutionMode {
    Headful,
    VirtualHeadful,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostGraphicsBackend {
    WebGl,
    WebGl2,
    WebGlAndWebGl2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostDisplayEnvironment {
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

impl HostDisplayEnvironment {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        width: u32,
        height: u32,
        avail_width: u32,
        avail_height: u32,
        avail_left: i32,
        avail_top: i32,
        color_depth: u16,
        pixel_depth: u16,
        device_pixel_ratio_milli: u32,
    ) -> Self {
        Self {
            width,
            height,
            avail_width,
            avail_height,
            avail_left,
            avail_top,
            color_depth,
            pixel_depth,
            device_pixel_ratio_milli,
        }
    }

    const fn is_supported(self) -> bool {
        self.width > 0
            && self.height > 0
            && self.avail_width > 0
            && self.avail_height > 0
            && self.width <= MAX_DISPLAY_EDGE
            && self.height <= MAX_DISPLAY_EDGE
            && self.avail_width <= MAX_DISPLAY_EDGE
            && self.avail_height <= MAX_DISPLAY_EDGE
            && self.color_depth > 0
            && self.pixel_depth > 0
            && self.device_pixel_ratio_milli >= MIN_DPR_MILLI
            && self.device_pixel_ratio_milli <= MAX_DPR_MILLI
            && self.avail_left > i32::MIN
            && self.avail_top > i32::MIN
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostCompatibilityObservation {
    platform: HostPlatformClass,
    architecture: HostArchitecture,
    runtime_class: HostRuntimeClass,
    execution_mode: HostExecutionMode,
    clock_unix_ms: u64,
    filesystem_capable: bool,
    process_capable: bool,
    display: Option<HostDisplayEnvironment>,
    graphics_backend: Option<HostGraphicsBackend>,
}

impl HostCompatibilityObservation {
    #[must_use]
    pub const fn prelaunch(
        platform: HostPlatformClass,
        architecture: HostArchitecture,
        runtime_class: HostRuntimeClass,
        execution_mode: HostExecutionMode,
        clock_unix_ms: u64,
        filesystem_capable: bool,
    ) -> Self {
        Self {
            platform,
            architecture,
            runtime_class,
            execution_mode,
            clock_unix_ms,
            filesystem_capable,
            process_capable: false,
            display: None,
            graphics_backend: None,
        }
    }

    #[must_use]
    pub const fn with_runtime_evidence(
        mut self,
        process_capable: bool,
        display: HostDisplayEnvironment,
        graphics_backend: HostGraphicsBackend,
    ) -> Self {
        self.process_capable = process_capable;
        self.display = Some(display);
        self.graphics_backend = Some(graphics_backend);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostCompatibilityPolicy {
    allowed_platforms: BTreeSet<HostPlatformClass>,
    allowed_architectures: BTreeSet<HostArchitecture>,
    allowed_runtime_classes: BTreeSet<HostRuntimeClass>,
    allowed_execution_modes: BTreeSet<HostExecutionMode>,
    allowed_graphics_backends: BTreeSet<HostGraphicsBackend>,
}

impl HostCompatibilityPolicy {
    pub fn new(
        allowed_platforms: impl IntoIterator<Item = HostPlatformClass>,
        allowed_architectures: impl IntoIterator<Item = HostArchitecture>,
        allowed_runtime_classes: impl IntoIterator<Item = HostRuntimeClass>,
        allowed_execution_modes: impl IntoIterator<Item = HostExecutionMode>,
        allowed_graphics_backends: impl IntoIterator<Item = HostGraphicsBackend>,
    ) -> Result<Self, HostCompatibilityPolicyError> {
        let policy = Self {
            allowed_platforms: allowed_platforms.into_iter().collect(),
            allowed_architectures: allowed_architectures.into_iter().collect(),
            allowed_runtime_classes: allowed_runtime_classes.into_iter().collect(),
            allowed_execution_modes: allowed_execution_modes.into_iter().collect(),
            allowed_graphics_backends: allowed_graphics_backends.into_iter().collect(),
        };
        if policy.allowed_platforms.is_empty()
            || policy.allowed_architectures.is_empty()
            || policy.allowed_runtime_classes.is_empty()
            || policy.allowed_execution_modes.is_empty()
            || policy.allowed_graphics_backends.is_empty()
        {
            return Err(HostCompatibilityPolicyError);
        }
        Ok(policy)
    }

    pub fn windows_first_release_headful() -> Result<Self, HostCompatibilityPolicyError> {
        Self::new(
            [HostPlatformClass::Windows],
            [HostArchitecture::X86_64],
            [HostRuntimeClass::PackagedCamoufox],
            [HostExecutionMode::Headful],
            [HostGraphicsBackend::WebGlAndWebGl2],
        )
    }

    pub fn repository_linux_virtual_headful() -> Result<Self, HostCompatibilityPolicyError> {
        Self::new(
            [HostPlatformClass::Linux],
            [HostArchitecture::X86_64],
            [HostRuntimeClass::RepositoryPinnedCamoufox],
            [HostExecutionMode::VirtualHeadful],
            [HostGraphicsBackend::WebGlAndWebGl2],
        )
    }

    #[must_use]
    pub fn evaluate(
        &self,
        observation: &HostCompatibilityObservation,
    ) -> HostCompatibilityDecision {
        if !self.allowed_platforms.contains(&observation.platform)
            || !self
                .allowed_architectures
                .contains(&observation.architecture)
            || !self
                .allowed_runtime_classes
                .contains(&observation.runtime_class)
            || !self
                .allowed_execution_modes
                .contains(&observation.execution_mode)
            || !(MIN_SUPPORTED_CLOCK_UNIX_MS..MAX_SUPPORTED_CLOCK_UNIX_MS)
                .contains(&observation.clock_unix_ms)
            || !observation.filesystem_capable
        {
            return HostCompatibilityDecision::IncompatibleHost;
        }

        let (Some(display), Some(graphics_backend)) =
            (observation.display, observation.graphics_backend)
        else {
            return HostCompatibilityDecision::PendingRuntimeEvidence;
        };
        if !observation.process_capable
            || !display.is_supported()
            || !self.allowed_graphics_backends.contains(&graphics_backend)
        {
            return HostCompatibilityDecision::IncompatibleHost;
        }

        HostCompatibilityDecision::Accepted
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostCompatibilityDecision {
    PendingRuntimeEvidence,
    Accepted,
    IncompatibleHost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostCompatibilityPolicyError;

impl core::fmt::Display for HostCompatibilityPolicyError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("host compatibility policy requires non-empty allowed capability sets")
    }
}

impl std::error::Error for HostCompatibilityPolicyError {}

#[cfg(test)]
mod tests {
    use super::{
        HostArchitecture, HostCompatibilityDecision, HostCompatibilityObservation,
        HostCompatibilityPolicy, HostDisplayEnvironment, HostExecutionMode, HostGraphicsBackend,
        HostPlatformClass, HostRuntimeClass,
    };

    fn display() -> HostDisplayEnvironment {
        HostDisplayEnvironment::new(1920, 1080, 1920, 1040, 0, 0, 24, 24, 1000)
    }

    #[test]
    fn first_release_windows_policy_requires_runtime_evidence_and_headful()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = HostCompatibilityPolicy::windows_first_release_headful()?;
        let prelaunch = HostCompatibilityObservation::prelaunch(
            HostPlatformClass::Windows,
            HostArchitecture::X86_64,
            HostRuntimeClass::PackagedCamoufox,
            HostExecutionMode::Headful,
            1_800_000_000_000,
            true,
        );
        assert_eq!(
            policy.evaluate(&prelaunch),
            HostCompatibilityDecision::PendingRuntimeEvidence
        );
        let accepted =
            prelaunch.with_runtime_evidence(true, display(), HostGraphicsBackend::WebGlAndWebGl2);
        assert_eq!(
            policy.evaluate(&accepted),
            HostCompatibilityDecision::Accepted
        );
        Ok(())
    }

    #[test]
    fn first_release_windows_policy_rejects_virtual_mode_and_bad_clock()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = HostCompatibilityPolicy::windows_first_release_headful()?;
        for observation in [
            HostCompatibilityObservation::prelaunch(
                HostPlatformClass::Windows,
                HostArchitecture::X86_64,
                HostRuntimeClass::PackagedCamoufox,
                HostExecutionMode::VirtualHeadful,
                1_800_000_000_000,
                true,
            ),
            HostCompatibilityObservation::prelaunch(
                HostPlatformClass::Windows,
                HostArchitecture::X86_64,
                HostRuntimeClass::PackagedCamoufox,
                HostExecutionMode::Headful,
                1,
                true,
            ),
        ] {
            assert_eq!(
                policy.evaluate(&observation),
                HostCompatibilityDecision::IncompatibleHost
            );
        }
        Ok(())
    }

    #[test]
    fn repository_linux_policy_is_not_a_windows_shipping_fallback()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = HostCompatibilityPolicy::repository_linux_virtual_headful()?;
        let observation = HostCompatibilityObservation::prelaunch(
            HostPlatformClass::Windows,
            HostArchitecture::X86_64,
            HostRuntimeClass::PackagedCamoufox,
            HostExecutionMode::Headful,
            1_800_000_000_000,
            true,
        )
        .with_runtime_evidence(true, display(), HostGraphicsBackend::WebGlAndWebGl2);
        assert_eq!(
            policy.evaluate(&observation),
            HostCompatibilityDecision::IncompatibleHost
        );
        Ok(())
    }

    #[test]
    fn runtime_surface_failure_is_incompatible_not_identity_migration()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = HostCompatibilityPolicy::windows_first_release_headful()?;
        let observation = HostCompatibilityObservation::prelaunch(
            HostPlatformClass::Windows,
            HostArchitecture::X86_64,
            HostRuntimeClass::PackagedCamoufox,
            HostExecutionMode::Headful,
            1_800_000_000_000,
            true,
        )
        .with_runtime_evidence(true, display(), HostGraphicsBackend::WebGl);
        assert_eq!(
            policy.evaluate(&observation),
            HostCompatibilityDecision::IncompatibleHost
        );
        Ok(())
    }
}
