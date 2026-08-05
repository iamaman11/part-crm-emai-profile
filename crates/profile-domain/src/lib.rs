#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{AggregateVersion, GenerationId, ProfileId, TenantId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileStatus {
    Draft,
    Quarantined,
    Ready,
    InUse,
    DirtyLocal,
    Syncing,
    Suspended,
    Deleting,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationVerification {
    Unverified,
    Verified,
    Corrupt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGeneration {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    verification: GenerationVerification,
}

impl ProfileGeneration {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        verification: GenerationVerification,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            verification,
        }
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
    pub const fn verification(&self) -> GenerationVerification {
        self.verification
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserProfile {
    tenant_id: TenantId,
    profile_id: ProfileId,
    version: AggregateVersion,
    status: ProfileStatus,
    active_generation_id: Option<GenerationId>,
}

impl BrowserProfile {
    #[must_use]
    pub const fn create(tenant_id: TenantId, profile_id: ProfileId) -> Self {
        Self {
            tenant_id,
            profile_id,
            version: AggregateVersion::INITIAL,
            status: ProfileStatus::Draft,
            active_generation_id: None,
        }
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
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn status(&self) -> ProfileStatus {
        self.status
    }

    #[must_use]
    pub const fn active_generation_id(&self) -> Option<&GenerationId> {
        self.active_generation_id.as_ref()
    }

    pub fn transition(&mut self, next: ProfileStatus) -> Result<(), ProfileError> {
        if !is_allowed_transition(self.status, next) {
            return Err(ProfileError::InvalidStatusTransition);
        }
        self.status = next;
        self.bump_version()
    }

    pub fn activate_generation(
        &mut self,
        generation: &ProfileGeneration,
    ) -> Result<(), ProfileError> {
        if generation.tenant_id() != &self.tenant_id {
            return Err(ProfileError::TenantMismatch);
        }
        if generation.profile_id() != &self.profile_id {
            return Err(ProfileError::ProfileMismatch);
        }
        if generation.verification() != GenerationVerification::Verified {
            return Err(ProfileError::GenerationNotVerified);
        }
        if matches!(
            self.status,
            ProfileStatus::InUse
                | ProfileStatus::DirtyLocal
                | ProfileStatus::Syncing
                | ProfileStatus::Deleting
                | ProfileStatus::Deleted
        ) {
            return Err(ProfileError::InvalidStatusTransition);
        }

        self.active_generation_id = Some(generation.generation_id().clone());
        self.status = ProfileStatus::Ready;
        self.bump_version()
    }

    fn bump_version(&mut self) -> Result<(), ProfileError> {
        self.version = self
            .version
            .next()
            .map_err(|_| ProfileError::VersionOverflow)?;
        Ok(())
    }
}

#[must_use]
pub const fn is_allowed_transition(current: ProfileStatus, next: ProfileStatus) -> bool {
    matches!(
        (current, next),
        (ProfileStatus::Draft, ProfileStatus::Quarantined | ProfileStatus::Ready)
            | (ProfileStatus::Quarantined, ProfileStatus::Ready | ProfileStatus::Suspended)
            | (
                ProfileStatus::Ready,
                ProfileStatus::InUse | ProfileStatus::Suspended | ProfileStatus::Deleting
            )
            | (ProfileStatus::InUse, ProfileStatus::DirtyLocal)
            | (ProfileStatus::DirtyLocal, ProfileStatus::Syncing)
            | (ProfileStatus::Syncing, ProfileStatus::Ready | ProfileStatus::Quarantined)
            | (ProfileStatus::Suspended, ProfileStatus::Ready | ProfileStatus::Deleting)
            | (ProfileStatus::Deleting, ProfileStatus::Deleted)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidStatusTransition,
    TenantMismatch,
    ProfileMismatch,
    GenerationNotVerified,
    VersionOverflow,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidStatusTransition => "profile status transition is invalid",
            Self::TenantMismatch => "generation tenant differs from profile tenant",
            Self::ProfileMismatch => "generation belongs to another profile",
            Self::GenerationNotVerified => "generation is not verified",
            Self::VersionOverflow => "profile version overflow",
        })
    }
}

impl std::error::Error for ProfileError {}

#[cfg(test)]
mod tests {
    use super::{
        BrowserProfile, GenerationVerification, ProfileError, ProfileGeneration, ProfileStatus,
    };
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

    fn profile() -> Result<BrowserProfile, Box<dyn std::error::Error>> {
        Ok(BrowserProfile::create(
            TenantId::parse("tenant_01JPROFILE")?,
            ProfileId::parse("profile_01JPROFILE")?,
        ))
    }

    #[test]
    fn live_profile_cannot_skip_dirty_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = profile()?;
        profile.transition(ProfileStatus::Ready)?;
        profile.transition(ProfileStatus::InUse)?;
        assert_eq!(
            profile.transition(ProfileStatus::Syncing),
            Err(ProfileError::InvalidStatusTransition)
        );
        Ok(())
    }

    #[test]
    fn unverified_generation_cannot_become_active() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = profile()?;
        let generation = ProfileGeneration::new(
            profile.tenant_id().clone(),
            profile.profile_id().clone(),
            GenerationId::parse("generation_01JPROFILE")?,
            GenerationVerification::Unverified,
        );
        assert_eq!(
            profile.activate_generation(&generation),
            Err(ProfileError::GenerationNotVerified)
        );
        Ok(())
    }

    #[test]
    fn verified_generation_activates_atomically() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = profile()?;
        let generation = ProfileGeneration::new(
            profile.tenant_id().clone(),
            profile.profile_id().clone(),
            GenerationId::parse("generation_01JPROFILE")?,
            GenerationVerification::Verified,
        );
        profile.activate_generation(&generation)?;
        assert_eq!(profile.status(), ProfileStatus::Ready);
        assert_eq!(profile.active_generation_id(), Some(generation.generation_id()));
        Ok(())
    }
}
