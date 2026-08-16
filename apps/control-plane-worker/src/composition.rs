mod outbound_mail;

pub use outbound_mail::{client_mail_outbound_provider, outbound_mail_intent_repository};

#[cfg(not(target_arch = "wasm32"))]
use application_ports::clients::{
    ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
    ContactProtectionPortError, ContactProtectionPortErrorClass,
};
#[cfg(not(target_arch = "wasm32"))]
use client_domain::{EncryptedContactValue, ExactLookupToken};
use cloudflare_adapters::access_identity::VerifiedExternalIdentity;
use cloudflare_adapters::cloud_mail_query::CloudMailboxQueryAdapter;
use cloudflare_adapters::d1_client_mail_eligibility::D1ClientMailboxEligibilityRepository;
use cloudflare_adapters::d1_clients::D1ClientRepository;
use cloudflare_adapters::d1_clients_query::D1ClientQueryRepository;
use cloudflare_adapters::d1_device_jobs::D1DeviceJobRepository;
use cloudflare_adapters::d1_mailbox_execution::D1BrowserMailboxExecutionRepository;
use cloudflare_adapters::d1_mailbox_jobs::D1MailboxJobRepository;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository;
use cloudflare_adapters::d1_notifications::D1NotificationRepository;
use cloudflare_adapters::d1_profile_assignments::D1ProfileAssignmentRepository;
use cloudflare_adapters::d1_profile_generations::D1ProfileGenerationRepository;
use cloudflare_adapters::d1_profile_grants::D1ProfileGrantRepository;
use cloudflare_adapters::d1_profiles::D1ProfileRepository;
use cloudflare_adapters::d1_query::D1QueryRepository;
use cloudflare_adapters::d1_tenant_memberships::D1TenantMembershipRepository;
use cloudflare_adapters::d1_tenants::D1TenantRepository;
use cloudflare_adapters::profile_generation_registry::D1ProfileGenerationRegistry;
use cloudflare_adapters::r2_profile_generations::R2ProfileGenerationObjectStore;
use cloudflare_adapters::CloudMailboxProviderRouter;
use control_plane_contract::{D1_CATALOG_BINDING, PROFILE_OBJECTS_R2_BINDING};
use profile_platform_primitives::{ActorContext, ClientId, ProfileId};
use use_cases::generations::ProfileGenerationApplication;
use use_cases::profiles::ProfileApplication;
use use_cases_clients::ClientApplication;
use use_cases_devices::DeviceJobRepository;
use use_cases_identity::{IdentityCeremonyApplication, IdentityGovernanceApplication};
use use_cases_mailboxes::{
    BrowserMailboxExecutionApplication, MailboxBindingApplication, MailboxJobApplication,
};
use worker::{Env, Result};

#[must_use]
pub fn client_application(env: &Env) -> Result<ClientApplication<D1ClientRepository>> {
    Ok(ClientApplication::new(D1ClientRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    )))
}

#[must_use]
pub fn identity_ceremony_application(
    env: &Env,
) -> Result<IdentityCeremonyApplication<D1TenantRepository, D1TenantMembershipRepository>> {
    Ok(IdentityCeremonyApplication::new(
        D1TenantRepository::new(env.d1(D1_CATALOG_BINDING)?),
        D1TenantMembershipRepository::new(env.d1(D1_CATALOG_BINDING)?),
    ))
}

#[must_use]
pub fn identity_governance_application(
    env: &Env,
) -> Result<IdentityGovernanceApplication<D1TenantRepository, D1TenantMembershipRepository>> {
    Ok(IdentityGovernanceApplication::new(
        D1TenantRepository::new(env.d1(D1_CATALOG_BINDING)?),
        D1TenantMembershipRepository::new(env.d1(D1_CATALOG_BINDING)?),
    ))
}

#[must_use]
pub fn profile_application(env: &Env) -> Result<ProfileApplication<D1ProfileRepository>> {
    Ok(ProfileApplication::new(D1ProfileRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    )))
}

#[must_use]
pub fn profile_generation_application(
    env: &Env,
) -> Result<
    ProfileGenerationApplication<
        D1ProfileGenerationRegistry,
        R2ProfileGenerationObjectStore,
        D1ProfileGenerationRepository,
    >,
> {
    Ok(ProfileGenerationApplication::new(
        D1ProfileGenerationRegistry::new(env.d1(D1_CATALOG_BINDING)?),
        R2ProfileGenerationObjectStore::new(env.bucket(PROFILE_OBJECTS_R2_BINDING)?),
        D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?),
    ))
}

#[must_use]
pub fn mailbox_binding_application(
    env: &Env,
) -> Result<MailboxBindingApplication<D1MailboxRepository>> {
    Ok(MailboxBindingApplication::new(D1MailboxRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    )))
}

#[must_use]
pub fn browser_mailbox_execution_application(
    env: &Env,
) -> Result<BrowserMailboxExecutionApplication<D1BrowserMailboxExecutionRepository>> {
    Ok(BrowserMailboxExecutionApplication::new(
        D1BrowserMailboxExecutionRepository::new(env.d1(D1_CATALOG_BINDING)?),
    ))
}

#[must_use]
pub fn mailbox_job_application(
    env: &Env,
) -> Result<MailboxJobApplication<D1MailboxJobRepository>> {
    Ok(MailboxJobApplication::new(D1MailboxJobRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    )))
}

#[must_use]
pub fn device_job_repository(env: &Env) -> Result<D1DeviceJobRepository> {
    Ok(D1DeviceJobRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

#[must_use]
pub fn query_repository(env: &Env) -> Result<D1QueryRepository> {
    Ok(D1QueryRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

#[must_use]
pub fn client_mail_eligibility_repository(
    env: &Env,
) -> Result<D1ClientMailboxEligibilityRepository> {
    Ok(D1ClientMailboxEligibilityRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn client_mail_query_provider<'a>(
    env: &'a Env,
    actor: &ActorContext,
    client_id: &ClientId,
) -> Result<CloudMailboxQueryAdapter<'a>> {
    Ok(CloudMailboxQueryAdapter::new(
        env,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        actor,
        client_id,
    ))
}

#[must_use]
pub fn notification_operations_repository(env: &Env) -> Result<D1NotificationOperationsRepository> {
    Ok(D1NotificationOperationsRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

#[must_use]
pub fn notification_cursor_repository(env: &Env) -> Result<D1NotificationRepository> {
    Ok(D1NotificationRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn mailbox_job_provider(env: &Env, actor: &ActorContext) -> Result<CloudMailboxProviderRouter> {
    CloudMailboxProviderRouter::new(env, actor)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct LocalContactProtection;

#[cfg(not(target_arch = "wasm32"))]
impl ContactProtectionPort for LocalContactProtection {
    async fn encrypt_contact(
        &self,
        request: ContactEncryptionRequest,
    ) -> std::result::Result<
        use_cases_clients::ContactEncryptionResult,
        ContactProtectionPortError,
    > {
        let ciphertext = format!("enc:{}", request.normalized_value());
        let lookup_token = ExactLookupToken::parse(format!("lookup:{}", request.normalized_value()))
            .map_err(|_| ContactProtectionPortError::new(ContactProtectionPortErrorClass::IntegrityFailure))?;
        let value = EncryptedContactValue::from_protected(
            ciphertext,
            request.normalized_value().len() as u64,
        )
        .map_err(|_| ContactProtectionPortError::new(ContactProtectionPortErrorClass::IntegrityFailure))?;
        Ok(use_cases_clients::ContactEncryptionResult::new(value, lookup_token))
    }

    async fn exact_lookup_token(
        &self,
        request: ContactExactLookupRequest,
    ) -> std::result::Result<ExactLookupToken, ContactProtectionPortError> {
        ExactLookupToken::parse(format!("lookup:{}", request.normalized_value())).map_err(|_| {
            ContactProtectionPortError::new(ContactProtectionPortErrorClass::IntegrityFailure)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::LocalContactProtection;
    use application_ports::clients::{
        ContactEncryptionRequest, ContactProtectionPort, ContactProtectionPortErrorClass,
    };
    use profile_platform_primitives::TenantId;

    #[test]
    fn local_contact_protection_returns_protected_value_and_lookup_token() {
        let protection = LocalContactProtection;
        let request = ContactEncryptionRequest::new(
            TenantId::parse("tenant_01").expect("tenant"),
            "email",
            "owner@example.com",
        );
        let result = futures::executor::block_on(protection.encrypt_contact(request))
            .expect("encrypted contact");
        assert_eq!(result.value().exposure(), client_domain::ContactExposure::Protected);
        assert_eq!(result.value().value_len(), 17);
        assert_eq!(result.lookup_token().as_str(), "lookup:owner@example.com");
    }

    #[test]
    fn local_contact_protection_rejects_empty_normalized_value() {
        let protection = LocalContactProtection;
        let request = ContactEncryptionRequest::new(
            TenantId::parse("tenant_01").expect("tenant"),
            "email",
            "",
        );
        let error = futures::executor::block_on(protection.encrypt_contact(request))
            .expect_err("empty input is invalid");
        assert_eq!(error.class(), ContactProtectionPortErrorClass::IntegrityFailure);
    }
}
