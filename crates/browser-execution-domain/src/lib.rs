#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use std::collections::BTreeSet;

const SHA256_HEX_LENGTH: usize = 64;
const MIN_TOKEN_BYTES: usize = 1;
const MAX_TOKEN_BYTES: usize = 128;
const MAX_TIMEZONE_BYTES: usize = 96;
const MAX_ROUTE_IDENTITY_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserIdentityManifest {
    compatibility_version: u32,
    runtime_version: String,
    runtime_inventory_sha256: String,
    fingerprint_source: String,
    fingerprint_config_sha256: String,
}

impl BrowserIdentityManifest {
    pub fn new(
        compatibility_version: u32,
        runtime_version: impl Into<String>,
        runtime_inventory_sha256: impl Into<String>,
        fingerprint_source: impl Into<String>,
        fingerprint_config_sha256: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        if compatibility_version == 0 {
            return Err(BrowserExecutionError::InvalidCompatibilityVersion);
        }
        let runtime_version = runtime_version.into();
        let runtime_inventory_sha256 = runtime_inventory_sha256.into();
        let fingerprint_source = fingerprint_source.into();
        let fingerprint_config_sha256 = fingerprint_config_sha256.into();
        if !valid_token(&runtime_version) || !valid_token(&fingerprint_source) {
            return Err(BrowserExecutionError::InvalidIdentityToken);
        }
        if !valid_sha256(&runtime_inventory_sha256) || !valid_sha256(&fingerprint_config_sha256) {
            return Err(BrowserExecutionError::InvalidDigest);
        }
        Ok(Self {
            compatibility_version,
            runtime_version,
            runtime_inventory_sha256,
            fingerprint_source,
            fingerprint_config_sha256,
        })
    }

    #[must_use]
    pub const fn compatibility_version(&self) -> u32 {
        self.compatibility_version
    }

    #[must_use]
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    #[must_use]
    pub fn runtime_inventory_sha256(&self) -> &str {
        &self.runtime_inventory_sha256
    }

    #[must_use]
    pub fn fingerprint_source(&self) -> &str {
        &self.fingerprint_source
    }

    #[must_use]
    pub fn fingerprint_config_sha256(&self) -> &str {
        &self.fingerprint_config_sha256
    }

    #[must_use]
    pub fn compatibility_with(&self, accepted: &Self) -> BrowserIdentityCompatibility {
        if self == accepted {
            BrowserIdentityCompatibility::Compatible
        } else {
            BrowserIdentityCompatibility::RequiresCandidateGenerationMigration
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserIdentityCompatibility {
    Compatible,
    RequiresCandidateGenerationMigration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationBinding {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    source_container_sha256: String,
    materialized_inventory_digest: u64,
    browser_identity: BrowserIdentityManifest,
}

impl MaterializationBinding {
    pub fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        source_container_sha256: impl Into<String>,
        materialized_inventory_digest: u64,
        browser_identity: BrowserIdentityManifest,
    ) -> Result<Self, BrowserExecutionError> {
        let source_container_sha256 = source_container_sha256.into();
        if !valid_sha256(&source_container_sha256) {
            return Err(BrowserExecutionError::InvalidDigest);
        }
        if materialized_inventory_digest == 0 {
            return Err(BrowserExecutionError::InvalidMaterializationInventory);
        }
        Ok(Self {
            tenant_id,
            profile_id,
            generation_id,
            source_container_sha256,
            materialized_inventory_digest,
            browser_identity,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn source_container_sha256(&self) -> &str {
        &self.source_container_sha256
    }

    #[must_use]
    pub const fn materialized_inventory_digest(&self) -> u64 {
        self.materialized_inventory_digest
    }

    #[must_use]
    pub const fn browser_identity(&self) -> &BrowserIdentityManifest {
        &self.browser_identity
    }

    #[must_use]
    pub fn matches(
        &self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        source_container_sha256: &str,
        materialized_inventory_digest: u64,
        browser_identity: &BrowserIdentityManifest,
    ) -> bool {
        self.tenant_id == *tenant_id
            && self.profile_id == *profile_id
            && self.generation_id == *generation_id
            && self.source_container_sha256 == source_container_sha256
            && self.materialized_inventory_digest == materialized_inventory_digest
            && self.browser_identity.compatibility_with(browser_identity)
                == BrowserIdentityCompatibility::Compatible
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NetworkClass {
    Fixed,
    Residential,
    Mobile,
    Datacenter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkIdentityPolicy {
    country: Option<String>,
    region: Option<String>,
    timezone: Option<String>,
    allowed_network_classes: BTreeSet<NetworkClass>,
    allowed_asns: BTreeSet<u32>,
    required_route_identity: Option<String>,
}

impl NetworkIdentityPolicy {
    pub fn new(
        country: Option<String>,
        region: Option<String>,
        timezone: Option<String>,
        allowed_network_classes: impl IntoIterator<Item = NetworkClass>,
        allowed_asns: impl IntoIterator<Item = u32>,
        required_route_identity: Option<String>,
    ) -> Result<Self, BrowserExecutionError> {
        if country
            .as_deref()
            .is_some_and(|value| !valid_location(value))
            || region
                .as_deref()
                .is_some_and(|value| !valid_location(value))
            || timezone
                .as_deref()
                .is_some_and(|value| !valid_timezone(value))
            || required_route_identity
                .as_deref()
                .is_some_and(|value| !valid_route_identity(value))
        {
            return Err(BrowserExecutionError::InvalidNetworkPolicy);
        }
        let allowed_network_classes = allowed_network_classes.into_iter().collect::<BTreeSet<_>>();
        let allowed_asns = allowed_asns.into_iter().collect::<BTreeSet<_>>();
        if allowed_network_classes.is_empty() || allowed_asns.contains(&0) {
            return Err(BrowserExecutionError::InvalidNetworkPolicy);
        }
        Ok(Self {
            country,
            region,
            timezone,
            allowed_network_classes,
            allowed_asns,
            required_route_identity,
        })
    }

    #[must_use]
    pub fn evaluate(&self, observation: &NetworkIdentityObservation) -> NetworkIdentityDecision {
        if self
            .country
            .as_deref()
            .is_some_and(|expected| expected != observation.country)
            || self
                .region
                .as_deref()
                .is_some_and(|expected| expected != observation.region)
            || self
                .timezone
                .as_deref()
                .is_some_and(|expected| expected != observation.timezone)
            || !self
                .allowed_network_classes
                .contains(&observation.network_class)
            || (!self.allowed_asns.is_empty() && !self.allowed_asns.contains(&observation.asn))
        {
            return NetworkIdentityDecision::OperatorRemediationRequired;
        }

        if self
            .required_route_identity
            .as_deref()
            .is_some_and(|expected| expected != observation.route_identity)
        {
            return NetworkIdentityDecision::RetryableRouteChurn;
        }

        NetworkIdentityDecision::Accepted
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkIdentityObservation {
    country: String,
    region: String,
    timezone: String,
    network_class: NetworkClass,
    asn: u32,
    route_identity: String,
}

impl NetworkIdentityObservation {
    pub fn new(
        country: impl Into<String>,
        region: impl Into<String>,
        timezone: impl Into<String>,
        network_class: NetworkClass,
        asn: u32,
        route_identity: impl Into<String>,
    ) -> Result<Self, BrowserExecutionError> {
        let country = country.into();
        let region = region.into();
        let timezone = timezone.into();
        let route_identity = route_identity.into();
        if !valid_location(&country)
            || !valid_location(&region)
            || !valid_timezone(&timezone)
            || asn == 0
            || !valid_route_identity(&route_identity)
        {
            return Err(BrowserExecutionError::InvalidNetworkObservation);
        }
        Ok(Self {
            country,
            region,
            timezone,
            network_class,
            asn,
            route_identity,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkIdentityDecision {
    Accepted,
    RetryableRouteChurn,
    OperatorRemediationRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserWriterObservation {
    bridge_lock_present: bool,
    parent_lock_present: bool,
    browser_lock_present: bool,
    supervised_writer_active: bool,
}

impl BrowserWriterObservation {
    #[must_use]
    pub const fn new(
        bridge_lock_present: bool,
        parent_lock_present: bool,
        browser_lock_present: bool,
        supervised_writer_active: bool,
    ) -> Self {
        Self {
            bridge_lock_present,
            parent_lock_present,
            browser_lock_present,
            supervised_writer_active,
        }
    }

    #[must_use]
    pub const fn classify(self) -> BrowserWriterDecision {
        if self.supervised_writer_active {
            return BrowserWriterDecision::ProfileBusy;
        }
        if self.bridge_lock_present || self.parent_lock_present || self.browser_lock_present {
            return BrowserWriterDecision::RecoveryRequired;
        }
        BrowserWriterDecision::Ready
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserWriterDecision {
    Ready,
    ProfileBusy,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserExecutionError {
    InvalidCompatibilityVersion,
    InvalidIdentityToken,
    InvalidDigest,
    InvalidMaterializationInventory,
    InvalidNetworkPolicy,
    InvalidNetworkObservation,
}

impl fmt::Display for BrowserExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCompatibilityVersion => {
                "browser identity compatibility version is invalid"
            }
            Self::InvalidIdentityToken => "browser identity token is invalid",
            Self::InvalidDigest => "browser identity digest must be lowercase SHA-256 hex",
            Self::InvalidMaterializationInventory => {
                "materialized inventory digest must be non-zero"
            }
            Self::InvalidNetworkPolicy => "network identity policy is invalid",
            Self::InvalidNetworkObservation => "network identity observation is invalid",
        })
    }
}

impl std::error::Error for BrowserExecutionError {}

fn valid_sha256(value: &str) -> bool {
    value.len() == SHA256_HEX_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_token(value: &str) -> bool {
    (MIN_TOKEN_BYTES..=MAX_TOKEN_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_location(value: &str) -> bool {
    (2..=MAX_TOKEN_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_timezone(value: &str) -> bool {
    (3..=MAX_TIMEZONE_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
}

fn valid_route_identity(value: &str) -> bool {
    (1..=MAX_ROUTE_IDENTITY_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserIdentityCompatibility, BrowserIdentityManifest, BrowserWriterDecision,
        BrowserWriterObservation, MaterializationBinding, NetworkClass, NetworkIdentityDecision,
        NetworkIdentityObservation, NetworkIdentityPolicy,
    };
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

    fn identity(character: char) -> Result<BrowserIdentityManifest, Box<dyn std::error::Error>> {
        Ok(BrowserIdentityManifest::new(
            1,
            "1.2.3",
            character.to_string().repeat(64),
            "camoufox-v1",
            character.to_string().repeat(64),
        )?)
    }

    #[test]
    fn browser_identity_change_requires_candidate_generation_migration()
    -> Result<(), Box<dyn std::error::Error>> {
        let accepted = identity('a')?;
        assert_eq!(
            accepted.compatibility_with(&accepted),
            BrowserIdentityCompatibility::Compatible
        );
        assert_eq!(
            identity('b')?.compatibility_with(&accepted),
            BrowserIdentityCompatibility::RequiresCandidateGenerationMigration
        );
        Ok(())
    }

    #[test]
    fn materialization_binding_rejects_generation_identity_or_inventory_substitution()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant = TenantId::parse("tenant_01JBRIDGE")?;
        let profile = ProfileId::parse("profile_01JBRIDGE")?;
        let generation = GenerationId::parse("generation_01JBRIDGE")?;
        let accepted_identity = identity('a')?;
        let binding = MaterializationBinding::new(
            tenant.clone(),
            profile.clone(),
            generation.clone(),
            "c".repeat(64),
            42,
            accepted_identity.clone(),
        )?;
        assert!(binding.matches(
            &tenant,
            &profile,
            &generation,
            &"c".repeat(64),
            42,
            &accepted_identity
        ));
        assert!(!binding.matches(
            &tenant,
            &profile,
            &GenerationId::parse("generation_02JBRIDGE")?,
            &"c".repeat(64),
            42,
            &accepted_identity
        ));
        assert!(!binding.matches(
            &tenant,
            &profile,
            &generation,
            &"c".repeat(64),
            43,
            &accepted_identity
        ));
        assert!(!binding.matches(
            &tenant,
            &profile,
            &generation,
            &"c".repeat(64),
            42,
            &identity('b')?
        ));
        Ok(())
    }

    #[test]
    fn network_policy_classifies_route_churn_separately_from_policy_mismatch()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = NetworkIdentityPolicy::new(
            Some("PL".to_owned()),
            Some("Mazowieckie".to_owned()),
            Some("Europe/Warsaw".to_owned()),
            [NetworkClass::Mobile],
            [5617],
            Some("route-a".to_owned()),
        )?;
        let accepted = NetworkIdentityObservation::new(
            "PL",
            "Mazowieckie",
            "Europe/Warsaw",
            NetworkClass::Mobile,
            5617,
            "route-a",
        )?;
        assert_eq!(
            policy.evaluate(&accepted),
            NetworkIdentityDecision::Accepted
        );

        let churn = NetworkIdentityObservation::new(
            "PL",
            "Mazowieckie",
            "Europe/Warsaw",
            NetworkClass::Mobile,
            5617,
            "route-b",
        )?;
        assert_eq!(
            policy.evaluate(&churn),
            NetworkIdentityDecision::RetryableRouteChurn
        );

        let wrong_region = NetworkIdentityObservation::new(
            "DE",
            "Berlin",
            "Europe/Berlin",
            NetworkClass::Mobile,
            5617,
            "route-a",
        )?;
        assert_eq!(
            policy.evaluate(&wrong_region),
            NetworkIdentityDecision::OperatorRemediationRequired
        );
        Ok(())
    }

    #[test]
    fn ambiguous_writer_files_fail_closed_without_pid_or_deletion_assumptions() {
        assert_eq!(
            BrowserWriterObservation::new(false, false, false, false).classify(),
            BrowserWriterDecision::Ready
        );
        assert_eq!(
            BrowserWriterObservation::new(false, true, false, false).classify(),
            BrowserWriterDecision::RecoveryRequired
        );
        assert_eq!(
            BrowserWriterObservation::new(true, false, false, false).classify(),
            BrowserWriterDecision::RecoveryRequired
        );
        assert_eq!(
            BrowserWriterObservation::new(true, true, true, true).classify(),
            BrowserWriterDecision::ProfileBusy
        );
    }
}
