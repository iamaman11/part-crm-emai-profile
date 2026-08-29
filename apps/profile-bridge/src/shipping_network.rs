use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
use crate::local_profile::GenerationWorkspace;
use bridge_domain::BridgePortError;
use browser_execution_domain::{
    NetworkClass, NetworkIdentityObservation, NetworkIdentityPolicy,
};
use profile_platform_primitives::{DeviceId, GenerationId};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const NETWORK_POLICY_SCHEMA: &str = "profile-bridge-network-policy-v1";
const NETWORK_OBSERVATION_SCHEMA: &str = "profile-bridge-network-observation-v1";
const MAX_EVIDENCE_BYTES: u64 = 32 * 1024;
const MAX_EVIDENCE_LIFETIME_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NetworkPolicyDocument {
    schema: String,
    country: Option<String>,
    region: Option<String>,
    timezone: Option<String>,
    allowed_network_classes: Vec<String>,
    allowed_asns: Vec<u32>,
    required_route_identity: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NetworkObservationDocument {
    schema: String,
    generation_id: String,
    device_id: String,
    observed_at_ms: u64,
    valid_until_ms: u64,
    country: String,
    region: String,
    timezone: String,
    network_class: String,
    asn: u32,
    route_identity: String,
}

#[derive(Clone, Debug)]
pub struct FilesystemNetworkEvidence {
    policy: NetworkIdentityPolicy,
}

impl FilesystemNetworkEvidence {
    pub fn open(policy_path: &Path) -> Result<Self, BridgePortError> {
        if !policy_path.is_absolute() {
            return Err(BridgePortError::InvalidResponse);
        }
        let policy_document = read_json::<NetworkPolicyDocument>(policy_path)?;
        if policy_document.schema != NETWORK_POLICY_SCHEMA {
            return Err(BridgePortError::InvalidResponse);
        }
        let allowed_network_classes = policy_document
            .allowed_network_classes
            .iter()
            .map(|value| parse_network_class(value))
            .collect::<Result<Vec<_>, _>>()?;
        let policy = NetworkIdentityPolicy::new(
            policy_document.country,
            policy_document.region,
            policy_document.timezone,
            allowed_network_classes,
            policy_document.allowed_asns,
            policy_document.required_route_identity,
        )
        .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(Self { policy })
    }

    #[must_use]
    pub fn policy(&self) -> NetworkIdentityPolicy {
        self.policy.clone()
    }
}

impl BrowserRuntimeObservationPort for FilesystemNetworkEvidence {
    type Error = BridgePortError;

    fn observe(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        let generation_value = workspace
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(BridgePortError::InvalidResponse)?;
        let generation_id = GenerationId::parse(generation_value.to_owned())
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let parent = workspace
            .path()
            .parent()
            .ok_or(BridgePortError::InvalidResponse)?;
        let observation_path = parent.join(format!(
            ".{}.network-observation.json",
            generation_id.as_str()
        ));
        let observation = read_json::<NetworkObservationDocument>(&observation_path)?;
        if observation.schema != NETWORK_OBSERVATION_SCHEMA
            || observation.generation_id != generation_id.as_str()
            || observation.device_id != device_id.as_str()
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let lifetime = observation
            .valid_until_ms
            .checked_sub(observation.observed_at_ms)
            .filter(|value| *value > 0 && *value <= MAX_EVIDENCE_LIFETIME_MS)
            .ok_or(BridgePortError::InvalidResponse)?;
        let now = now_ms()?;
        if now < observation.observed_at_ms || now >= observation.valid_until_ms || lifetime == 0 {
            return Err(BridgePortError::InvalidResponse);
        }
        let network = NetworkIdentityObservation::new(
            observation.country,
            observation.region,
            observation.timezone,
            parse_network_class(&observation.network_class)?,
            observation.asn,
            observation.route_identity,
        )
        .map_err(|_| BridgePortError::InvalidResponse)?;
        Ok(BrowserRuntimeObservation::new(network, false))
    }
}

fn read_json<T>(path: &Path) -> Result<T, BridgePortError>
where
    T: for<'de> Deserialize<'de>,
{
    let metadata = fs::symlink_metadata(path).map_err(|_| BridgePortError::Unavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_EVIDENCE_BYTES
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let bytes = fs::read(path).map_err(|_| BridgePortError::Unavailable)?;
    serde_json::from_slice(&bytes).map_err(|_| BridgePortError::InvalidResponse)
}

fn parse_network_class(value: &str) -> Result<NetworkClass, BridgePortError> {
    match value {
        "fixed" => Ok(NetworkClass::Fixed),
        "residential" => Ok(NetworkClass::Residential),
        "mobile" => Ok(NetworkClass::Mobile),
        "datacenter" => Ok(NetworkClass::Datacenter),
        _ => Err(BridgePortError::InvalidResponse),
    }
}

fn now_ms() -> Result<u64, BridgePortError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BridgePortError::Unavailable)?
        .as_millis();
    u64::try_from(millis).map_err(|_| BridgePortError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::{FilesystemNetworkEvidence, NETWORK_POLICY_SCHEMA};
    use crate::browser_preflight::BrowserRuntimeObservationPort;
    use crate::local_profile::MaterializationRoot;
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-p2-network-{}-{nonce}",
            std::process::id()
        )))
    }

    use std::path::PathBuf;

    #[test]
    fn missing_generation_observation_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path()?;
        let policy_path = root_path.with_extension("policy.json");
        fs::write(
            &policy_path,
            format!(
                "{{\"schema\":\"{NETWORK_POLICY_SCHEMA}\",\"country\":\"PL\",\"region\":\"Mazowieckie\",\"timezone\":\"Europe/Warsaw\",\"allowedNetworkClasses\":[\"mobile\"],\"allowedAsns\":[5617],\"requiredRouteIdentity\":\"route-a\"}}"
            ),
        )?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JP2NETWORK")?;
        let profile = ProfileId::parse("profile_01JP2NETWORK")?;
        let generation = GenerationId::parse("generation_01JP2NETWORK")?;
        let device = DeviceId::parse("device_01JP2NETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let mut evidence = FilesystemNetworkEvidence::open(&policy_path)?;
        assert!(evidence.observe(&workspace, &device).is_err());
        fs::remove_dir_all(&root_path)?;
        fs::remove_file(policy_path)?;
        Ok(())
    }
}
