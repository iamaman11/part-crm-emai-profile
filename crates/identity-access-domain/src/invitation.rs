use crate::lifecycle::{MembershipLifecycleError, require_active_owner};
use crate::{Membership, MembershipRole, MembershipStatus};
use core::fmt;
use profile_platform_primitives::{
    ActorContext, ActorId, IdentityId, InvitationId, TenantId, UnixMillis,
};

const MIN_CONTACT_HMAC_LENGTH: usize = 16;
const MAX_CONTACT_HMAC_LENGTH: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invitation {
    tenant_id: TenantId,
    invitation_id: InvitationId,
    invited_contact_hmac: String,
    status: InvitationStatus,
    expires_at: UnixMillis,
    created_by_actor_id: ActorId,
}

impl Invitation {
    pub fn create(
        actor: &ActorContext,
        actor_membership: &Membership,
        invitation_id: InvitationId,
        invited_contact_hmac: impl Into<String>,
        expires_at: UnixMillis,
        now: UnixMillis,
    ) -> Result<Self, InvitationError> {
        require_active_owner(actor, actor_membership).map_err(InvitationError::Membership)?;
        let invited_contact_hmac = invited_contact_hmac.into();
        if !(MIN_CONTACT_HMAC_LENGTH..=MAX_CONTACT_HMAC_LENGTH)
            .contains(&invited_contact_hmac.len())
            || !invited_contact_hmac
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(InvitationError::InvalidContactHmac);
        }
        if expires_at <= now {
            return Err(InvitationError::InvalidExpiry);
        }

        Ok(Self {
            tenant_id: actor.tenant_scope().tenant_id().clone(),
            invitation_id,
            invited_contact_hmac,
            status: InvitationStatus::Pending,
            expires_at,
            created_by_actor_id: actor.actor_id().clone(),
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn invitation_id(&self) -> &InvitationId {
        &self.invitation_id
    }

    #[must_use]
    pub fn invited_contact_hmac(&self) -> &str {
        &self.invited_contact_hmac
    }

    #[must_use]
    pub const fn status(&self) -> InvitationStatus {
        self.status
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn created_by_actor_id(&self) -> &ActorId {
        &self.created_by_actor_id
    }

    pub fn accept(
        &mut self,
        identity_id: &IdentityId,
        actor_id: ActorId,
        now: UnixMillis,
    ) -> Result<Membership, InvitationError> {
        if self.status != InvitationStatus::Pending {
            return Err(InvitationError::NotPending);
        }
        if now >= self.expires_at {
            self.status = InvitationStatus::Expired;
            return Err(InvitationError::Expired);
        }
        if identity_id.as_str().is_empty() {
            return Err(InvitationError::InvalidIdentity);
        }

        self.status = InvitationStatus::Accepted;
        Ok(Membership::new(
            self.tenant_id.clone(),
            actor_id,
            MembershipRole::Member,
            MembershipStatus::Active,
        ))
    }

    pub fn revoke(
        &mut self,
        actor: &ActorContext,
        actor_membership: &Membership,
    ) -> Result<(), InvitationError> {
        require_active_owner(actor, actor_membership).map_err(InvitationError::Membership)?;
        if actor.tenant_scope().tenant_id() != &self.tenant_id {
            return Err(InvitationError::TenantMismatch);
        }
        if self.status != InvitationStatus::Pending {
            return Err(InvitationError::NotPending);
        }
        self.status = InvitationStatus::Revoked;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvitationError {
    Membership(MembershipLifecycleError),
    InvalidContactHmac,
    InvalidExpiry,
    InvalidIdentity,
    TenantMismatch,
    NotPending,
    Expired,
}

impl fmt::Display for InvitationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Membership(error) => error.fmt(formatter),
            Self::InvalidContactHmac => formatter.write_str("invitation contact HMAC is invalid"),
            Self::InvalidExpiry => formatter.write_str("invitation expiry must be in the future"),
            Self::InvalidIdentity => formatter.write_str("invitation identity is invalid"),
            Self::TenantMismatch => formatter.write_str("invitation tenant mismatch"),
            Self::NotPending => formatter.write_str("invitation is not pending"),
            Self::Expired => formatter.write_str("invitation has expired"),
        }
    }
}

impl std::error::Error for InvitationError {}

#[cfg(test)]
mod tests {
    use super::{Invitation, InvitationError, InvitationStatus};
    use crate::{Membership, MembershipRole, MembershipStatus};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, IdentityId, InvitationId, TenantId, TenantScope,
        UnixMillis,
    };

    fn owner_fixture() -> Result<(ActorContext, Membership), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JINVITE")?;
        let actor_id = ActorId::parse("actor_01JINVITE")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            actor_id.clone(),
            CorrelationId::parse("corr_01JINVITE")?,
        );
        let membership = Membership::new(
            tenant_id,
            actor_id,
            MembershipRole::TenantOwner,
            MembershipStatus::Active,
        );
        Ok((actor, membership))
    }

    #[test]
    fn owner_creates_and_identity_accepts_pending_invitation()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, owner) = owner_fixture()?;
        let mut invitation = Invitation::create(
            &actor,
            &owner,
            InvitationId::parse("invite_01JINVITE")?,
            "contact_hmac_01JINVITE",
            UnixMillis::new(200),
            UnixMillis::new(100),
        )?;
        let membership = invitation.accept(
            &IdentityId::parse("identity_01JINVITE")?,
            ActorId::parse("actor_02JINVITE")?,
            UnixMillis::new(150),
        )?;
        assert_eq!(invitation.status(), InvitationStatus::Accepted);
        assert_eq!(membership.role(), MembershipRole::Member);
        assert_eq!(membership.status(), MembershipStatus::Active);
        Ok(())
    }

    #[test]
    fn expired_invitation_cannot_activate_membership()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, owner) = owner_fixture()?;
        let mut invitation = Invitation::create(
            &actor,
            &owner,
            InvitationId::parse("invite_02JINVITE")?,
            "contact_hmac_02JINVITE",
            UnixMillis::new(200),
            UnixMillis::new(100),
        )?;
        assert_eq!(
            invitation.accept(
                &IdentityId::parse("identity_02JINVITE")?,
                ActorId::parse("actor_03JINVITE")?,
                UnixMillis::new(200),
            ),
            Err(InvitationError::Expired)
        );
        assert_eq!(invitation.status(), InvitationStatus::Expired);
        Ok(())
    }
}
