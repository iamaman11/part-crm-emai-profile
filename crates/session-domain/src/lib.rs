#![forbid(unsafe_code)]

pub mod coordinator;

use core::fmt;
use profile_platform_primitives::{
    ActorContext, ActorId, DeviceId, FencingToken, LaunchIntentId, ProfileId, SessionId, TenantId,
    UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchIntent {
    tenant_id: TenantId,
    launch_intent_id: LaunchIntentId,
    actor_id: ActorId,
    device_id: DeviceId,
    profile_id: ProfileId,
    expires_at: UnixMillis,
    redeemed_at: Option<UnixMillis>,
}

impl LaunchIntent {
    #[must_use]
    pub const fn issue(
        tenant_id: TenantId,
        launch_intent_id: LaunchIntentId,
        actor_id: ActorId,
        device_id: DeviceId,
        profile_id: ProfileId,
        expires_at: UnixMillis,
    ) -> Self {
        Self {
            tenant_id,
            launch_intent_id,
            actor_id,
            device_id,
            profile_id,
            expires_at,
            redeemed_at: None,
        }
    }

    pub fn redeem(
        &mut self,
        actor: &ActorContext,
        device_id: &DeviceId,
        now: UnixMillis,
    ) -> Result<&ProfileId, LaunchIntentError> {
        if actor.tenant_scope().tenant_id() != &self.tenant_id {
            return Err(LaunchIntentError::TenantMismatch);
        }
        if actor.actor_id() != &self.actor_id {
            return Err(LaunchIntentError::ActorMismatch);
        }
        if device_id != &self.device_id {
            return Err(LaunchIntentError::DeviceMismatch);
        }
        if self.redeemed_at.is_some() {
            return Err(LaunchIntentError::ReplayRejected);
        }
        if now >= self.expires_at {
            return Err(LaunchIntentError::Expired);
        }

        self.redeemed_at = Some(now);
        Ok(&self.profile_id)
    }

    #[must_use]
    pub const fn launch_intent_id(&self) -> &LaunchIntentId {
        &self.launch_intent_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseStatus {
    Active,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileLease {
    tenant_id: TenantId,
    profile_id: ProfileId,
    session_id: SessionId,
    device_id: DeviceId,
    epoch: u64,
    fencing_token: FencingToken,
    status: LeaseStatus,
}

impl ProfileLease {
    pub fn issue(
        tenant_id: TenantId,
        profile_id: ProfileId,
        session_id: SessionId,
        device_id: DeviceId,
        epoch: u64,
        fencing_token: FencingToken,
    ) -> Result<Self, LeaseError> {
        if epoch == 0 {
            return Err(LeaseError::InvalidEpoch);
        }
        Ok(Self {
            tenant_id,
            profile_id,
            session_id,
            device_id,
            epoch,
            fencing_token,
            status: LeaseStatus::Active,
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
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub const fn fencing_token(&self) -> &FencingToken {
        &self.fencing_token
    }

    #[must_use]
    pub const fn status(&self) -> LeaseStatus {
        self.status
    }

    #[must_use]
    pub fn accepts_commit(&self, epoch: u64, token: &FencingToken) -> bool {
        self.status == LeaseStatus::Active && self.epoch == epoch && &self.fencing_token == token
    }

    pub fn close(&mut self, epoch: u64, token: &FencingToken) -> Result<(), LeaseError> {
        if !self.accepts_commit(epoch, token) {
            return Err(LeaseError::StaleWriter);
        }
        self.status = LeaseStatus::Closed;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchIntentError {
    TenantMismatch,
    ActorMismatch,
    DeviceMismatch,
    ReplayRejected,
    Expired,
}

impl fmt::Display for LaunchIntentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TenantMismatch => "launch intent tenant mismatch",
            Self::ActorMismatch => "launch intent actor mismatch",
            Self::DeviceMismatch => "launch intent device mismatch",
            Self::ReplayRejected => "launch intent replay rejected",
            Self::Expired => "launch intent expired",
        })
    }
}

impl std::error::Error for LaunchIntentError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseError {
    InvalidEpoch,
    StaleWriter,
}

impl fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEpoch => "lease epoch must be positive",
            Self::StaleWriter => "lease command has stale epoch or fencing token",
        })
    }
}

impl std::error::Error for LeaseError {}

#[cfg(test)]
mod tests {
    use super::{LaunchIntent, LaunchIntentError, ProfileLease};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, FencingToken, LaunchIntentId, ProfileId,
        SessionId, TenantId, TenantScope, UnixMillis,
    };

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JSESSION")?;
        Ok(ActorContext::new(
            TenantScope::new(tenant_id),
            ActorId::parse("actor_01JSESSION")?,
            CorrelationId::parse("corr_01JSESSION")?,
        ))
    }

    fn intent(
        actor: &ActorContext,
        device_id: &DeviceId,
    ) -> Result<LaunchIntent, Box<dyn std::error::Error>> {
        Ok(LaunchIntent::issue(
            actor.tenant_scope().tenant_id().clone(),
            LaunchIntentId::parse("intent_01JSESSION")?,
            actor.actor_id().clone(),
            device_id.clone(),
            ProfileId::parse("profile_01JSESSION")?,
            UnixMillis::new(100),
        ))
    }

    #[test]
    fn launch_intent_is_single_use() -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let device_id = DeviceId::parse("device_01JSESSION")?;
        let mut intent = intent(&actor, &device_id)?;
        intent.redeem(&actor, &device_id, UnixMillis::new(50))?;
        assert_eq!(
            intent.redeem(&actor, &device_id, UnixMillis::new(51)),
            Err(LaunchIntentError::ReplayRejected)
        );
        Ok(())
    }

    #[test]
    fn launch_intent_expires_at_exact_deadline() -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let device_id = DeviceId::parse("device_01JSESSION")?;
        let mut intent = intent(&actor, &device_id)?;
        assert_eq!(
            intent.redeem(&actor, &device_id, UnixMillis::new(100)),
            Err(LaunchIntentError::Expired)
        );
        Ok(())
    }

    #[test]
    fn wrong_device_cannot_redeem_intent() -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        let mut intent = intent(&actor, &DeviceId::parse("device_01JSESSION")?)?;
        assert_eq!(
            intent.redeem(
                &actor,
                &DeviceId::parse("device_02JSESSION")?,
                UnixMillis::new(50),
            ),
            Err(LaunchIntentError::DeviceMismatch)
        );
        Ok(())
    }

    #[test]
    fn stale_fencing_token_cannot_commit() -> Result<(), Box<dyn std::error::Error>> {
        let token = FencingToken::parse("fence_01JSESSION")?;
        let lease = ProfileLease::issue(
            TenantId::parse("tenant_01JSESSION")?,
            ProfileId::parse("profile_01JSESSION")?,
            SessionId::parse("session_01JSESSION")?,
            DeviceId::parse("device_01JSESSION")?,
            2,
            token.clone(),
        )?;
        assert!(lease.accepts_commit(2, &token));
        assert!(!lease.accepts_commit(1, &token));
        assert!(!lease.accepts_commit(2, &FencingToken::parse("fence_02JSESSION")?));
        Ok(())
    }
}
