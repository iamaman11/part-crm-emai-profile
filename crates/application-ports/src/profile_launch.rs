use crate::CommandExecutionEvidence;
use core::fmt;
use identity_access_domain::ProfileGrant;
use profile_domain::BrowserProfile;
use profile_platform_primitives::{
    ActorContext, ActorId, DeviceId, GenerationId, ProfileId, TenantId, TenantScope, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileLaunchContext {
    profile: BrowserProfile,
    grant: Option<ProfileGrant>,
}

impl ProfileLaunchContext {
    #[must_use]
    pub const fn new(profile: BrowserProfile, grant: Option<ProfileGrant>) -> Self {
        Self { profile, grant }
    }

    #[must_use]
    pub const fn profile(&self) -> &BrowserProfile {
        &self.profile
    }

    #[must_use]
    pub const fn grant(&self) -> Option<&ProfileGrant> {
        self.grant.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileLaunchPortErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileLaunchPortError {
    class: ProfileLaunchPortErrorClass,
}

impl ProfileLaunchPortError {
    #[must_use]
    pub const fn new(class: ProfileLaunchPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ProfileLaunchPortErrorClass {
        self.class
    }
}

impl fmt::Display for ProfileLaunchPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ProfileLaunchPortErrorClass::IntegrityFailure => {
                "profile launch context integrity failure"
            }
            ProfileLaunchPortErrorClass::DependencyUnavailable => {
                "profile launch context dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ProfileLaunchPortError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileLaunchAuthorityErrorClass {
    Conflict,
    NotFound,
    ReplayRejected,
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileLaunchAuthorityError {
    class: ProfileLaunchAuthorityErrorClass,
}

impl ProfileLaunchAuthorityError {
    #[must_use]
    pub const fn new(class: ProfileLaunchAuthorityErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ProfileLaunchAuthorityErrorClass {
        self.class
    }
}

impl fmt::Display for ProfileLaunchAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ProfileLaunchAuthorityErrorClass::Conflict => "profile launch authority conflict",
            ProfileLaunchAuthorityErrorClass::NotFound => "profile launch authority not found",
            ProfileLaunchAuthorityErrorClass::ReplayRejected => {
                "profile launch authority replay rejected"
            }
            ProfileLaunchAuthorityErrorClass::IntegrityFailure => {
                "profile launch authority integrity failure"
            }
            ProfileLaunchAuthorityErrorClass::DependencyUnavailable => {
                "profile launch authority dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ProfileLaunchAuthorityError {}

#[derive(Clone, Eq, PartialEq)]
pub struct IssuedProfileLaunchAuthority {
    claim_code: String,
    expires_at: UnixMillis,
    replayed: bool,
}

impl IssuedProfileLaunchAuthority {
    #[must_use]
    pub fn new(claim_code: String, expires_at: UnixMillis, replayed: bool) -> Self {
        Self {
            claim_code,
            expires_at,
            replayed,
        }
    }

    #[must_use]
    pub fn claim_code(&self) -> &str {
        &self.claim_code
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl fmt::Debug for IssuedProfileLaunchAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedProfileLaunchAuthority")
            .field("claim_code", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("replayed", &self.replayed)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileLaunchAuthorityBinding {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    profile_id: ProfileId,
    generation_id: GenerationId,
}

impl ProfileLaunchAuthorityBinding {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        actor_id: ActorId,
        device_id: DeviceId,
        profile_id: ProfileId,
        generation_id: GenerationId,
    ) -> Self {
        Self {
            tenant_id,
            actor_id,
            device_id,
            profile_id,
            generation_id,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileLaunchMachineBinding {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
}

impl ProfileLaunchMachineBinding {
    #[must_use]
    pub const fn new(tenant_id: TenantId, actor_id: ActorId, device_id: DeviceId) -> Self {
        Self {
            tenant_id,
            actor_id,
            device_id,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

#[allow(async_fn_in_trait)]
pub trait ProfileLaunchContextPort {
    async fn load_profile_launch_context(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileLaunchContext>, ProfileLaunchPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ProfileLaunchAuthorityPort {
    async fn issue_profile_launch_authority(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        device_id: &DeviceId,
        evidence: &CommandExecutionEvidence,
    ) -> Result<IssuedProfileLaunchAuthority, ProfileLaunchAuthorityError>;

    /// Read-only validation of a still-live authority. The caller must already have authenticated
    /// the machine and supplies that authenticated device identity before any claim lookup.
    async fn inspect_profile_launch_authority(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError>;

    /// Final one-time CAS. Security-sensitive current state must be revalidated by the use-case
    /// immediately before this operation.
    async fn consume_profile_launch_authority(
        &self,
        claim_code: &str,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError>;
}
