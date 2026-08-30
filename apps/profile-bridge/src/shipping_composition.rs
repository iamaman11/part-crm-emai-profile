#![forbid(unsafe_code)]

use bridge_domain::ClaimUri;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShippingCompositionError {
    UnsupportedPlatform,
    Configuration,
    Clock,
    ControlPlane,
    Operator,
    CommittedRecoveryRequired,
}

impl core::fmt::Display for ShippingCompositionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "shipping Profile Bridge requires Windows",
            Self::Configuration => "shipping Profile Bridge configuration is invalid",
            Self::Clock => "shipping Profile Bridge clock is unavailable",
            Self::ControlPlane => "shipping Profile Bridge control-plane lease state is invalid",
            Self::Operator => "shipping Profile Bridge operator flow failed",
            Self::CommittedRecoveryRequired => {
                "generation successor committed but local recovery is required"
            }
        })
    }
}

impl std::error::Error for ShippingCompositionError {}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfirmedSaveTrigger {
    ContinueRuntime,
    BeginConfirmedSave,
}

#[cfg(any(windows, test))]
fn confirmed_save_trigger<E>(observation: Result<bool, E>) -> Result<ConfirmedSaveTrigger, E> {
    observation.map(|controlled| {
        if controlled {
            ConfirmedSaveTrigger::BeginConfirmedSave
        } else {
            ConfirmedSaveTrigger::ContinueRuntime
        }
    })
}

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
    use super::{ConfirmedSaveTrigger, ShippingCompositionError, confirmed_save_trigger};
    use crate::camouhost_process::{
        ManagedCamouhostConfig, ManagedCamouhostProcess, RuntimeBindingSlot, RuntimeDisplayMode,
    };
    use crate::generation_reopen::VerifiedGenerationObjectDownloader;
    use crate::local_profile::MaterializationRoot;
    use crate::operator_flow::ProfileBridgeOperator;
    use crate::runtime_bundle::FilesystemRuntimeBundleSelection;
    use crate::shipping_control_plane::{
        ControlPlaneCoordinator, ControlPlaneEnrollment, ControlPlaneLeaseTiming,
    };
    use crate::shipping_network::FilesystemNetworkEvidence;
    use crate::shipping_preflight::ShippingBrowserLaunchPreflight;
    use crate::windows_generation_put::WindowsSignedGenerationObjectPut;
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
    const NETWORK_POLICY_PATH_ENV: &str = "PROFILE_BRIDGE_NETWORK_POLICY_PATH";
    const PROXY_CONFIG_PATH_ENV: &str = "PROFILE_BRIDGE_PROXY_CONFIG_PATH";
    const PACKAGED_PYTHON_EXECUTABLE: &str = "python/python.exe";
    const CONTROLLED_CLOSE_POLL_MS: u64 = 250;

    struct ShippingConfig {
        device_id: DeviceId,
        machine_cert_sha1: String,
        control_plane_origin: String,
        materialization_root: PathBuf,
        runtime_root: PathBuf,
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
        let save_transport = transport.clone();
        let signed_generation_get = WindowsSignedGenerationObjectGet::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;
        let mut generation_downloader =
            VerifiedGenerationObjectDownloader::new(signed_generation_get);
        let mut generation_uploader = WindowsSignedGenerationObjectPut::from_system()
            .map_err(|_| ShippingCompositionError::Configuration)?;

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
        let python_executable = config.runtime_root.join(PACKAGED_PYTHON_EXECUTABLE);
        let camouhost_config = ManagedCamouhostConfig::new(
            python_executable,
            config.runtime_root,
            RuntimeDisplayMode::Headful,
            None,
            config.proxy_config_path,
        )
        .map_err(|_| ShippingCompositionError::Configuration)?;
        let (process, camouhost) = ManagedCamouhostProcess::pair(camouhost_config, runtime_binding);
        let mut close_observer = camouhost.close_observer();

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

        let mut next_heartbeat_deadline = next_heartbeat_at(
            operator
                .coordinator()
                .runtime_timing()
                .map_err(|_| ShippingCompositionError::ControlPlane)?,
            now()?,
        )?;
        loop {
            let observed_at = now()?;
            let session_id = operator
                .active_session_id()
                .cloned()
                .ok_or(ShippingCompositionError::Operator)?;
            let save_trigger = confirmed_save_trigger(
                close_observer
                    .observe_controlled_close(&session_id)
                    .map_err(|_| ShippingCompositionError::Operator),
            )?;
            if save_trigger == ConfirmedSaveTrigger::BeginConfirmedSave {
                operator
                    .close(now()?)
                    .map_err(|_| ShippingCompositionError::Operator)?;
                let completion = operator
                    .save_retained_successor(
                        &materialization_root,
                        save_transport,
                        &mut generation_uploader,
                        now()?,
                    )
                    .map_err(|_| ShippingCompositionError::Operator)?;
                if completion.is_saved() {
                    return Ok(());
                }
                return Err(ShippingCompositionError::CommittedRecoveryRequired);
            }

            if observed_at >= next_heartbeat_deadline {
                operator
                    .heartbeat(observed_at)
                    .map_err(|_| ShippingCompositionError::Operator)?;
                next_heartbeat_deadline = next_heartbeat_at(
                    operator
                        .coordinator()
                        .runtime_timing()
                        .map_err(|_| ShippingCompositionError::ControlPlane)?,
                    observed_at,
                )?;
            }

            let until_heartbeat = next_heartbeat_deadline
                .value()
                .saturating_sub(observed_at.value())
                .max(1);
            thread::sleep(Duration::from_millis(
                until_heartbeat.min(CONTROLLED_CLOSE_POLL_MS),
            ));
        }
    }

    fn next_heartbeat_at(
        timing: ControlPlaneLeaseTiming,
        observed_at: UnixMillis,
    ) -> Result<UnixMillis, ShippingCompositionError> {
        let deadline = timing.idle_expires_at_ms().min(timing.hard_expires_at_ms());
        let remaining = deadline.saturating_sub(observed_at.value());
        if remaining <= 1 {
            return Err(ShippingCompositionError::ControlPlane);
        }
        let delay = (remaining / 2).max(1);
        observed_at
            .value()
            .checked_add(delay)
            .map(UnixMillis::new)
            .ok_or(ShippingCompositionError::Clock)
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
    use super::{
        ConfirmedSaveTrigger, ShippingCompositionError, confirmed_save_trigger, run_claim,
    };
    use bridge_domain::ClaimUri;

    #[test]
    fn confirmed_save_requires_positive_controlled_close_witness() {
        assert_eq!(
            confirmed_save_trigger::<ShippingCompositionError>(Ok(false)),
            Ok(ConfirmedSaveTrigger::ContinueRuntime)
        );
        assert_eq!(
            confirmed_save_trigger::<ShippingCompositionError>(Ok(true)),
            Ok(ConfirmedSaveTrigger::BeginConfirmedSave)
        );
        assert_eq!(
            confirmed_save_trigger(Err(ShippingCompositionError::Operator)),
            Err(ShippingCompositionError::Operator)
        );
    }

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
