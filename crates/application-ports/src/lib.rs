#![forbid(unsafe_code)]

use client_domain::ClientRecord;
use contracts::ProblemCode;
use identity_access_domain::{ClientGrant, Membership, ProfileGrant};
use mailbox_domain::{MailboxBinding, MailboxJob};
use profile_domain::{BrowserProfile, ProfileGeneration};
use profile_platform_primitives::{
    ActorContext, ActorId, ClientId, DeviceId, GenerationId, MailboxBindingId, ProfileId,
    TenantScope, UnixMillis,
};
use session_domain::ProfileLease;

pub trait ClockPort {
    fn now(&self) -> UnixMillis;
}

pub trait MembershipRepository {
    type Error;

    fn find_membership(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
    ) -> Result<Option<Membership>, Self::Error>;

    fn find_profile_grant(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileGrant>, Self::Error>;

    fn find_client_grant(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        client_id: &ClientId,
    ) -> Result<Option<ClientGrant>, Self::Error>;
}

pub trait ClientRepository {
    type Error;

    fn get_client(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, Self::Error>;

    fn save_client(
        &mut self,
        actor: &ActorContext,
        client: &ClientRecord,
    ) -> Result<(), Self::Error>;
}

pub trait ProfileRepository {
    type Error;

    fn get_profile(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<BrowserProfile>, Self::Error>;

    fn get_generation(
        &self,
        scope: &TenantScope,
        generation_id: &GenerationId,
    ) -> Result<Option<ProfileGeneration>, Self::Error>;

    fn save_profile(
        &mut self,
        actor: &ActorContext,
        profile: &BrowserProfile,
    ) -> Result<(), Self::Error>;
}

pub trait ProfileCoordinatorPort {
    type Error;

    fn acquire_lease(
        &mut self,
        actor: &ActorContext,
        profile_id: &ProfileId,
        device_id: &DeviceId,
    ) -> Result<ProfileLease, Self::Error>;

    fn close_lease(&mut self, lease: &ProfileLease) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationObjectReference {
    generation_id: GenerationId,
    ciphertext_digest: String,
}

impl GenerationObjectReference {
    #[must_use]
    pub fn new(generation_id: GenerationId, ciphertext_digest: impl Into<String>) -> Self {
        Self {
            generation_id,
            ciphertext_digest: ciphertext_digest.into(),
        }
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn ciphertext_digest(&self) -> &str {
        &self.ciphertext_digest
    }
}

pub trait GenerationObjectStorePort {
    type Error;

    fn verify_generation_object(
        &self,
        scope: &TenantScope,
        reference: &GenerationObjectReference,
    ) -> Result<bool, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxObservation {
    binding_id: MailboxBindingId,
    provider_status: String,
    bounded_item_count: u32,
    next_cursor: Option<String>,
}

impl MailboxObservation {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        provider_status: impl Into<String>,
        bounded_item_count: u32,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            binding_id,
            provider_status: provider_status.into(),
            bounded_item_count,
            next_cursor,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub fn provider_status(&self) -> &str {
        &self.provider_status
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

pub trait MailboxProviderPort {
    type Error;

    fn check_mailbox(
        &mut self,
        binding: &MailboxBinding,
        job: &MailboxJob,
    ) -> Result<MailboxObservation, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditResult {
    Succeeded,
    Rejected(ProblemCode),
    Failed(ProblemCode),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord<'a> {
    actor: &'a ActorContext,
    action: &'static str,
    resource_id: &'a str,
    result: AuditResult,
}

impl<'a> AuditRecord<'a> {
    #[must_use]
    pub const fn new(
        actor: &'a ActorContext,
        action: &'static str,
        resource_id: &'a str,
        result: AuditResult,
    ) -> Self {
        Self {
            actor,
            action,
            resource_id,
            result,
        }
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        self.actor
    }

    #[must_use]
    pub const fn action(&self) -> &'static str {
        self.action
    }

    #[must_use]
    pub const fn resource_id(&self) -> &str {
        self.resource_id
    }

    #[must_use]
    pub const fn result(&self) -> AuditResult {
        self.result
    }
}

pub trait AuditPort {
    type Error;

    fn append(&mut self, record: AuditRecord<'_>) -> Result<(), Self::Error>;
}
