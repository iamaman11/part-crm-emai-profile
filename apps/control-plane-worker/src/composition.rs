#[cfg(not(target_arch = "wasm32"))]
use application_ports::clients::{
    ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
    ContactProtectionPortError, ContactProtectionPortErrorClass,
};
#[cfg(not(target_arch = "wasm32"))]
use client_domain::{EncryptedContactValue, ExactLookupToken};
use cloudflare_adapters::access_identity::VerifiedExternalIdentity;
#[cfg(target_arch = "wasm32")]
use cloudflare_adapters::contact_keyring::contact_protection_from_serialized_keyring;
#[cfg(target_arch = "wasm32")]
use cloudflare_adapters::contact_protection::{
    RustCryptoContactProtection, WorkerCryptoNonceSource,
};
use cloudflare_adapters::coordinator_ingress::{
    CloudflareCoordinatorIngressApplication, CloudflareDeviceGenerationCommitPort,
};
use cloudflare_adapters::d1_authenticated_device::D1AuthenticatedDevice;
use cloudflare_adapters::d1_browser_mail_execution::D1BrowserMailboxExecutionBinding;
use cloudflare_adapters::d1_client_merge::D1ClientMergeRepository;
use cloudflare_adapters::d1_client_persistence::D1ClientPersistenceRepository;
use cloudflare_adapters::d1_client_registry::D1ClientRegistryProjectionRepository;
use cloudflare_adapters::d1_clients::D1ClientApplicationRepository;
use cloudflare_adapters::d1_device_authorization::D1DeviceJobAuthorization;
use cloudflare_adapters::d1_device_generation_commit::D1DeviceGenerationCommitJournal;
use cloudflare_adapters::d1_device_jobs::D1DeviceJobRepository;
use cloudflare_adapters::d1_device_preconditions::D1DeviceExecutionPreconditions;
use cloudflare_adapters::d1_identity_ceremonies::D1IdentityCeremonyApplicationRepository;
use cloudflare_adapters::d1_identity_governance::D1IdentityGovernanceApplicationRepository;
use cloudflare_adapters::d1_mailbox_bindings::D1MailboxBindingApplicationRepository;
use cloudflare_adapters::d1_mailbox_jobs::D1MailboxJobApplicationRepository;
use cloudflare_adapters::d1_profile_application::D1ProfileApplicationBundle;
use cloudflare_adapters::d1_profile_generation_application::D1ProfileGenerationApplicationRepository;
use cloudflare_adapters::microsoft_graph_authorization::D1MicrosoftGraphAuthorization;
use cloudflare_adapters::r2_generation_objects::R2GenerationObjects;
use cloudflare_adapters::r2_generation_upload_capability::{
    R2GenerationUploadCapabilitySigner, R2SigV4Credentials,
};
use control_plane_contract::{
    D1_CATALOG_BINDING, PROFILE_COORDINATOR_BINDING, R2_PROFILES_BINDING,
};
use worker::{Env, Error, Result};

#[cfg(target_arch = "wasm32")]
const CLIENT_CONTACT_PROTECTION_KEYRING_BINDING: &str = "CLIENT_CONTACT_PROTECTION_KEYRING";
const R2_GENERATION_ACCOUNT_ID_BINDING: &str = "R2_GENERATION_ACCOUNT_ID";
const R2_GENERATION_BUCKET_NAME_BINDING: &str = "R2_GENERATION_BUCKET_NAME";
const R2_GENERATION_ACCESS_KEY_ID_BINDING: &str = "R2_GENERATION_ACCESS_KEY_ID";
const R2_GENERATION_SECRET_ACCESS_KEY_BINDING: &str = "R2_GENERATION_SECRET_ACCESS_KEY";

pub fn client_application(env: &Env) -> Result<D1ClientApplicationRepository> {
    Ok(D1ClientApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn client_persistence_application(env: &Env) -> Result<D1ClientPersistenceRepository> {
    Ok(D1ClientPersistenceRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn client_merge_application(env: &Env) -> Result<D1ClientMergeRepository> {
    Ok(D1ClientMergeRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_registry_projection(env: &Env) -> Result<D1ClientRegistryProjectionRepository> {
    Ok(D1ClientRegistryProjectionRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

#[cfg(target_arch = "wasm32")]
pub fn client_contact_protection(
    env: &Env,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>> {
    contact_protection_from_serialized_keyring(
        env.secret(CLIENT_CONTACT_PROTECTION_KEYRING_BINDING)?
            .to_string(),
    )
    .map_err(|_| Error::RustError("invalid client contact protection keyring".to_owned()))
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableContactProtection;

#[cfg(not(target_arch = "wasm32"))]
impl ContactProtectionPort for UnavailableContactProtection {
    async fn encrypt_contact_display(
        &self,
        _request: ContactEncryptionRequest<'_>,
    ) -> Result<EncryptedContactValue, ContactProtectionPortError> {
        Err(ContactProtectionPortError::new(
            ContactProtectionPortErrorClass::InternalFailure,
        ))
    }

    async fn derive_exact_lookup_token(
        &self,
        _request: ContactExactLookupRequest<'_>,
    ) -> Result<ExactLookupToken, ContactProtectionPortError> {
        Err(ContactProtectionPortError::new(
            ContactProtectionPortErrorClass::InternalFailure,
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn client_contact_protection(_env: &Env) -> Result<UnavailableContactProtection> {
    Ok(UnavailableContactProtection)
}

pub fn identity_governance_application(
    env: &Env,
) -> Result<D1IdentityGovernanceApplicationRepository> {
    Ok(D1IdentityGovernanceApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn identity_ceremony_application(
    env: &Env,
    verified_identity: VerifiedExternalIdentity,
) -> Result<D1IdentityCeremonyApplicationRepository> {
    Ok(D1IdentityCeremonyApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        verified_identity,
    ))
}

pub fn profile_application(env: &Env) -> Result<D1ProfileApplicationBundle> {
    Ok(D1ProfileApplicationBundle::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn profile_generation_application(
    env: &Env,
) -> Result<D1ProfileGenerationApplicationRepository> {
    Ok(D1ProfileGenerationApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn mailbox_binding_application(env: &Env) -> Result<D1MailboxBindingApplicationRepository> {
    Ok(D1MailboxBindingApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn browser_mailbox_execution_application(
    env: &Env,
) -> Result<D1BrowserMailboxExecutionBinding> {
    Ok(D1BrowserMailboxExecutionBinding::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn mailbox_job_application(env: &Env) -> Result<D1MailboxJobApplicationRepository> {
    Ok(D1MailboxJobApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn microsoft_graph_mailbox_authorization(env: &Env) -> Result<D1MicrosoftGraphAuthorization> {
    Ok(D1MicrosoftGraphAuthorization::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn authenticated_device(env: &Env) -> Result<D1AuthenticatedDevice> {
    Ok(D1AuthenticatedDevice::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn device_job_authorization(env: &Env) -> Result<D1DeviceJobAuthorization> {
    Ok(D1DeviceJobAuthorization::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn device_execution_preconditions(env: &Env) -> Result<D1DeviceExecutionPreconditions> {
    Ok(D1DeviceExecutionPreconditions::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn device_job_repository(env: &Env) -> Result<D1DeviceJobRepository> {
    Ok(D1DeviceJobRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn device_generation_replay_probe(env: &Env) -> Result<D1DeviceGenerationCommitJournal> {
    Ok(D1DeviceGenerationCommitJournal::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn generation_object_verifier(env: &Env) -> Result<R2GenerationObjects> {
    Ok(R2GenerationObjects::new(env.bucket(R2_PROFILES_BINDING)?))
}

pub fn generation_upload_capability_signer(
    env: &Env,
) -> Result<R2GenerationUploadCapabilitySigner> {
    let credentials = R2SigV4Credentials::new(
        env.secret(R2_GENERATION_ACCESS_KEY_ID_BINDING)?.to_string(),
        env.secret(R2_GENERATION_SECRET_ACCESS_KEY_BINDING)?
            .to_string(),
    )
    .map_err(|_| {
        Error::RustError("invalid R2 generation upload signing configuration".to_owned())
    })?;
    R2GenerationUploadCapabilitySigner::new(
        env.var(R2_GENERATION_ACCOUNT_ID_BINDING)?.to_string(),
        env.var(R2_GENERATION_BUCKET_NAME_BINDING)?.to_string(),
        credentials,
    )
    .map_err(|_| Error::RustError("invalid R2 generation upload signing configuration".to_owned()))
}

#[must_use]
pub fn coordinator_ingress_application(env: &Env) -> CloudflareCoordinatorIngressApplication<'_> {
    CloudflareCoordinatorIngressApplication::new(
        env,
        D1_CATALOG_BINDING,
        PROFILE_COORDINATOR_BINDING,
    )
}

#[must_use]
pub fn device_generation_commit(env: &Env) -> CloudflareDeviceGenerationCommitPort<'_> {
    CloudflareDeviceGenerationCommitPort::new(env, PROFILE_COORDINATOR_BINDING)
}
