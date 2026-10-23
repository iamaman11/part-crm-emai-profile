use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
use crate::local_profile::GenerationWorkspace;
use bridge_domain::BridgePortError;
use browser_execution_domain::{NetworkClass, NetworkIdentityObservation, NetworkIdentityPolicy};
use profile_platform_primitives::DeviceId;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const NETWORK_POLICY_SCHEMA: &str = "profile-bridge-network-policy-v1";
const NETWORK_OBSERVATION_SCHEMA: &str = "profile-bridge-network-observation-v1";
const MAX_EVIDENCE_BYTES: u64 = 32 * 1024;
const MAX_EVIDENCE_LIFETIME_MS: u64 = 5 * 60 * 1000;
pub const PROFILE_NETWORK_POLICY_FILE: &str = "profile-network-policy.json";

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
pub struct GenerationNetworkEvidence {
    policy: NetworkIdentityPolicy,
    workspace: PathBuf,
}

impl GenerationNetworkEvidence {
    /// Policy bytes (including their absence for Direct) belong to the authenticated generation
    /// and its materialization inventory. A read/decode failure never selects Direct.
    pub fn open(workspace: &GenerationWorkspace) -> Result<Self, BridgePortError> {
        let policy_path = workspace.path().join(PROFILE_NETWORK_POLICY_FILE);
        match fs::symlink_metadata(&policy_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    policy: NetworkIdentityPolicy::direct(),
                    workspace: workspace.path().to_path_buf(),
                });
            }
            Err(_) => return Err(BridgePortError::Unavailable),
            Ok(_) => {}
        }
        let policy_document = read_json::<NetworkPolicyDocument>(&policy_path)?;
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
        Ok(Self {
            policy,
            workspace: workspace.path().to_path_buf(),
        })
    }

    #[must_use]
    pub fn policy(&self) -> NetworkIdentityPolicy {
        self.policy.clone()
    }
}

impl BrowserRuntimeObservationPort for GenerationNetworkEvidence {
    type Error = BridgePortError;

    fn observe(
        &mut self,
        workspace: &GenerationWorkspace,
        device_id: &DeviceId,
    ) -> Result<BrowserRuntimeObservation, Self::Error> {
        if self.workspace != workspace.path() {
            return Err(BridgePortError::InvalidResponse);
        }
        if !self.policy.requires_observation() {
            return Ok(BrowserRuntimeObservation::without_network_observation(
                false,
            ));
        }
        let generation_id = workspace.generation_id();
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
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_EVIDENCE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| BridgePortError::Unavailable)?;
    if bytes.len() as u64 > MAX_EVIDENCE_BYTES {
        return Err(BridgePortError::InvalidResponse);
    }
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
    use super::{GenerationNetworkEvidence, NETWORK_POLICY_SCHEMA, PROFILE_NETWORK_POLICY_FILE};
    use crate::browser_preflight::{BrowserRuntimeObservation, BrowserRuntimeObservationPort};
    use crate::local_profile::{GenerationWorkspace, MaterializationRoot};
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct Fixture {
        root: PathBuf,
        workspace: GenerationWorkspace,
        device: DeviceId,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let root = std::env::temp_dir().join(format!(
                "profile-network-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let materialization = MaterializationRoot::open_or_create(&root)?;
            let workspace = materialization.create_generation(
                &TenantId::parse("tenant_01JP2NETWORK")?,
                &ProfileId::parse("profile_01JP2NETWORK")?,
                &GenerationId::parse("generation_01JP2NETWORK")?,
            )?;
            Ok(Self {
                root,
                workspace,
                device: DeviceId::parse("device_01JP2NETWORK")?,
            })
        }

        fn restrict(&self) -> TestResult {
            fs::write(
                self.workspace.path().join(PROFILE_NETWORK_POLICY_FILE),
                serde_json::to_vec(&json!({
                    "schema": NETWORK_POLICY_SCHEMA, "country": "PL", "region": "Mazowieckie",
                    "timezone": "Europe/Warsaw", "allowedNetworkClasses": ["mobile"],
                    "allowedAsns": [5617], "requiredRouteIdentity": "route-a"
                }))?,
            )?;
            Ok(())
        }

        fn observation(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
            Ok(json!({"schema": "profile-bridge-network-observation-v1",
                "generationId": "generation_01JP2NETWORK", "deviceId": self.device.as_str(),
                "observedAtMs": now, "validUntilMs": now + 60000,
                "country": "PL", "region": "Mazowieckie", "timezone": "Europe/Warsaw",
                "networkClass": "mobile", "asn": 5617, "routeIdentity": "route-a"}))
        }

        fn write_observation(&self, document: &serde_json::Value) -> TestResult {
            let parent = self.workspace.path().parent().ok_or("workspace parent")?;
            fs::write(
                parent.join(".generation_01JP2NETWORK.network-observation.json"),
                serde_json::to_vec(document)?,
            )?;
            Ok(())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.root.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }

    #[test]
    fn direct_profile_does_not_invent_or_require_network_observations() -> TestResult {
        let fixture = Fixture::new()?;
        let mut evidence = GenerationNetworkEvidence::open(&fixture.workspace)?;
        assert!(!evidence.policy().requires_observation());
        assert_eq!(
            evidence.observe(&fixture.workspace, &fixture.device)?,
            BrowserRuntimeObservation::without_network_observation(false)
        );
        Ok(())
    }

    #[test]
    fn configured_restriction_still_requires_generation_device_and_freshness() -> TestResult {
        let fixture = Fixture::new()?;
        fixture.restrict()?;
        let mut evidence = GenerationNetworkEvidence::open(&fixture.workspace)?;
        assert!(evidence.policy().requires_observation());
        assert!(
            evidence
                .observe(&fixture.workspace, &fixture.device)
                .is_err()
        );
        let valid = fixture.observation()?;
        fixture.write_observation(&valid)?;
        assert!(
            evidence
                .observe(&fixture.workspace, &fixture.device)
                .is_ok()
        );
        for (key, value) in [
            ("deviceId", json!("device_other")),
            ("generationId", json!("generation_other")),
            ("validUntilMs", json!(1)),
            ("validUntilMs", json!(u64::MAX)),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            fixture.write_observation(&invalid)?;
            assert!(
                evidence
                    .observe(&fixture.workspace, &fixture.device)
                    .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_or_non_file_policy_cannot_select_direct() -> TestResult {
        let fixture = Fixture::new()?;
        let path = fixture.workspace.path().join(PROFILE_NETWORK_POLICY_FILE);
        for bytes in [b"{}".as_slice(), b"{", b"null", b"[]"] {
            fs::write(&path, bytes)?;
            assert!(GenerationNetworkEvidence::open(&fixture.workspace).is_err());
        }
        fs::write(&path, vec![b'x'; 32 * 1024 + 1])?;
        assert!(GenerationNetworkEvidence::open(&fixture.workspace).is_err());
        fixture.restrict()?;
        let original = fs::read_to_string(&path)?;
        for invalid in [
            original.replacen("{", "{\"unexpected\":true,", 1),
            original.replacen("{", "{\"country\":\"PL\",", 1),
        ] {
            fs::write(&path, invalid)?;
            assert!(GenerationNetworkEvidence::open(&fixture.workspace).is_err());
        }
        fs::remove_file(&path)?;
        fs::create_dir(&path)?;
        assert!(GenerationNetworkEvidence::open(&fixture.workspace).is_err());
        Ok(())
    }

    #[test]
    fn direct_selection_cannot_be_reused_for_another_workspace() -> TestResult {
        let first = Fixture::new()?;
        let second = Fixture::new()?;
        second.restrict()?;
        let mut evidence = GenerationNetworkEvidence::open(&first.workspace)?;
        assert!(evidence.observe(&second.workspace, &second.device).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn linked_policy_is_rejected() -> TestResult {
        let first = Fixture::new()?;
        first.restrict()?;
        let second = Fixture::new()?;
        std::os::unix::fs::symlink(
            first.workspace.path().join(PROFILE_NETWORK_POLICY_FILE),
            second.workspace.path().join(PROFILE_NETWORK_POLICY_FILE),
        )?;
        assert!(GenerationNetworkEvidence::open(&second.workspace).is_err());
        Ok(())
    }
}
