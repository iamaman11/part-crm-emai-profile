use core::fmt;
use identity_access_domain::ProfileGrant;
use profile_domain::BrowserProfile;
use profile_platform_primitives::{ActorId, ProfileId, TenantScope};

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

#[allow(async_fn_in_trait)]
pub trait ProfileLaunchContextPort {
    async fn load_profile_launch_context(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileLaunchContext>, ProfileLaunchPortError>;
}
