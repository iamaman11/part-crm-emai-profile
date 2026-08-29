#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShippingCompositionError {
    UnsupportedPlatform,
    Configuration,
    Clock,
    ControlPlane,
    Operator,
}

impl core::fmt::Display for ShippingCompositionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "shipping Profile Bridge requires Windows",
            Self::Configuration => "shipping Profile Bridge configuration is invalid",
            Self::Clock => "shipping Profile Bridge clock is unavailable",
            Self::ControlPlane => "shipping Profile Bridge control-plane lease state is invalid",
            Self::Operator => "shipping Profile Bridge operator flow failed",
        })
    }
}

impl std::error::Error for ShippingCompositionError {}

pub fn run_claim(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
    #[cfg(windows)]
    {
        return windows::run(claim);
    }
    #[cfg(not(windows))]
    {
        let _ = claim;
        Err(ShippingCompositionError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod windows {
    use super::ShippingCompositionError;
    use crate::camouhost_process::{
        ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
    };
    use crate::generation_reopen::VerifiedGenerationObjectDownloader;
    use crate::local_profile::MaterializationRoot;
    use crate::operator_flow::ProfileBridgeOperator;
    use crate::runtime_bundle::FilesystemRuntimeBundleSelection;
    use crate::shipping_control_plane::{ControlPlaneCoordinator, ControlPlaneEnrollment};
    use crate::shipping_network::FilesystemNetworkEvidence;
    use crate::shipping_preflight::ShippingBrowserLaunchPreflight;
    use crate::windows_native::{
        WindowsDeviceIdentity, WindowsMachineCertificate, WindowsSchannelMachineHttp,
        WindowsSignedGenerationObjectGet,
    };
    use bridge_domain::ClaimUri;
    use profile_platform_primitives::{DeviceId, UnixMillis};
    use std::env;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const DEVICE_ID_ENV: &str = "PROFILE_BRIDGE_DEVICE_ID";
    const MACHINE_CERT_SHA1_ENV: &str = "PROFILE_BRIDGE_MACHINE_CERT_SHA1";
    const CONTROL_PLANE_ORIGIN_ENV: &str = "PROFILE_BRIDGE_CONTROL_PLANE_ORIGIN";
    const MATERIALIZATION_ROOT_ENV: &str = "PROFILE_BRIDGE_MATERIALIZATION_ROOT";
    const RUNTIME_ROOT_ENV: &str = "PROFILE_BRIDGE_RUNTIME_ROOT";
    const PYTHON_EXECUTABLE_ENV: &str = "PROFILE_BRIDGE_PYTHON_EXECUTABLE";
    const NETWORK_POLICY_PATH_ENV: &str = "PROFILE_BRIDGE_NETWORK_POLICY_PATH";
    const PROXY_CONFIG_PATH_ENV: &str = "PROFILE_BRIDGE_PROXY_CONFIG_PATH";

    struct ShippingConfig {
        device_id: DeviceId,
        machine_cert_sha1: String,
        control_plane_origin: String,
        materialization_root: PathBuf,
        runtime_root: PathBuf,
        python_executable: PathBuf,
        network_policy_path: PathBuf,
        proxy_config_path: Option<PathBuf>,
    }

    impl ShippingConfig {
        fn from_environment() -> Result<Self, ShippingCompositionError> {
            let device_id = DeviceId::parse(required_env(DEVICE_ID_ENV)?)
                .map_err(|_| ShippingCompositionError::Configuration)?;
            let machine_cert_sha1 = required_env(MACHINE_CERT_SHA1_ENV)?;
            let control_plane_origin = required_env(CONTROL_PLANE_ORIGIN_ENV)?;
            let materialization_root = absolute_path(required_env(MATERIALIZATION_ROOT_ENV)?)?;
            let runtime_root = absolute_path(required_env(RUNTIME_ROOT_ENV)?)?;
            let python_executable = absolute_path(required_env(PYTHON_EXECUTABLE_ENV)?)?;
            let network_policy_path = absolute_path(required_env(NETWORK_POLICY_PATH_ENV)?)?;
            let proxy_config_path = optional_env(PROXY_CONFIG_PATH_ENV)?
                .map(absolute_path)
                .transpose()?;
            Ok(Self {
                device_id,
                machine_cert_sha1,
                control_plane_origin,
                materialization_root,
                runtime_root,
                python_executable,
                network_policy_path,
                proxy_config_path,
            })
        }
    }

    pub fn run(claim: &ClaimUri) -> Result<(), ShippingCompositionError> {
        let config = ShippingConfig::from_environment()?;
        let identity = WindowsDeviceIdentity::new(config.device_id.clone());
        let certificate = WindowsMachineCertificate::local_machine_my(
            config.device_id.clone(),
            &config.machine_cert_sha1,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let transport = WindowsSchannelMachineHttp::from_system(
            config.control_plane_origin,
            certificate.selector().to_owned(),
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let signed_generation_get = WindowsSignedGenerationObjectGet::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut generation_downloader =
            VerifiedGenerationObjectDownloader::new(signed_generation_get);

        let enrollment = ControlPlaneEnrollment::new(transport.clone());
        let coordinator = ControlPlaneCoordinator::new(transport);
        let runtime_bundles = FilesystemRuntimeBundleSelection::open(config.runtime_root.clone())
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let materialization_root =
            MaterializationRoot::open_or_create(&config.materialization_root)
                .map_err(|_| ShippingCompositionError::Configuration)?;
        let network_evidence = FilesystemNetworkEvidence::open(&config.network_policy_path)
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let runtime_binding = RuntimeBindingSlot::new();
        let browser_preflight = ShippingBrowserLaunchPreflight::new(
            network_evidence.policy(),
            network_evidence,
            runtime_binding.clone(),
        );
        let camouhost_config = ManagedCamouhostConfig::new(
            config.python_executable,
            config.runtime_root,
            RuntimeDisplayMode::Headful,
            None,
            config.proxy_config_path,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let (process, camouhost) = ManagedCamouhostProcess::pair(camouhost_config, runtime_binding);

        let mut operator = ProfileBridgeOperator::new(
            identity,
            certificate.clone(),
            certificate,
            enrollment,
            coordinator,
            runtime_bundles,
            browser_preflight,
            process,
            camouhost,
        );
        operator
            .open_authoritative(
                claim,
                &materialization_root,
                &mut generation_downloader,
                now()?,
            )
            .map_err(|_| ShippingCompositionError::Operator)?;

        loop {
            let observed_at = now()?;
            let timing = operator
                .coordinator()
                .runtime_timing()
                .map_err(|_| ShippingCompositionError::ControlPlane)?;
            let deadline = timing.idle_expires_at_ms().min(timing.hard_expires_at_ms());
            let remaining = deadline.saturating_sub(observed_at.value());
            let delay_ms = (remaining / 2).max(1);
            thread::sleep(Duration::from_millis(delay_ms));
            operator
                .heartbeat(now()?)
                .map_err(|_| ShippingCompositionError::Operator)?;
        }
    }

    fn required_env(name: &str) -> Result<String, ShippingCompositionError> {
        let value = env::var(name).map_err(|_| ShippingCompositionError::Configuration)?;
        if value.is_empty() || value.contains('\0') {
            return Err(ShippingCompositionError::Configuration);
        }
        Ok(value)
    }

    fn optional_env(name: &str) -> Result<Option<String>, ShippingCompositionError> {
        match env::var(name) {
            Ok(value) if value.is_empty() => Ok(None),
            Ok(value) if value.contains('\0') => Err(ShippingCompositionError::Configuration),
            Ok(value) => Ok(Some(value)),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => Err(ShippingCompositionError::Configuration),
        }
    }

    fn absolute_path(value: String) -> Result<PathBuf, ShippingCompositionError> {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(ShippingCompositionError::Configuration);
        }
        Ok(path)
    }

    fn now() -> Result<UnixMillis, ShippingCompositionError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ShippingCompositionError::Clock)?
            .as_millis();
        let millis = u64::try_from(millis).map_err(|_| ShippingCompositionError::Clock)?;
        Ok(UnixMillis::new(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::{ShippingCompositionError, run_claim};
    use bridge_domain::ClaimUri;

    #[cfg(not(windows))]
    #[test]
    fn non_windows_shipping_binary_has_no_fallback_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let claim = ClaimUri::parse("profilebridge://claim/claim_01JBRIDGE_FEASIBILITY")?;
        assert_eq!(
            run_claim(&claim),
            Err(ShippingCompositionError::UnsupportedPlatform)
        );
        Ok(())
    }
}
