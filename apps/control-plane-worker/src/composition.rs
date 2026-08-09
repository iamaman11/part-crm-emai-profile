use cloudflare_adapters::access_identity::VerifiedExternalIdentity;
use cloudflare_adapters::contact_keyring::contact_protection_from_serialized_keyring;
use cloudflare_adapters::contact_protection::{
    RustCryptoContactProtection, WorkerCryptoNonceSource,
};
use cloudflare_adapters::d1_client_merge::D1ClientMergeRepository;
use cloudflare_adapters::d1_client_persistence::D1ClientPersistenceRepository;
use cloudflare_adapters::d1_client_registry::D1ClientRegistryProjectionRepository;
use cloudflare_adapters::d1_clients::D1ClientApplicationRepository;
use cloudflare_adapters::d1_identity_ceremonies::D1IdentityCeremonyApplicationRepository;
use cloudflare_adapters::d1_identity_governance::D1IdentityGovernanceApplicationRepository;
use cloudflare_adapters::d1_mailbox_bindings::D1MailboxBindingApplicationRepository;
use cloudflare_adapters::d1_mailbox_jobs::D1MailboxJobApplicationRepository;
use cloudflare_adapters::d1_profile_application::D1ProfileApplicationBundle;
use cloudflare_adapters::d1_profile_generation_application::D1ProfileGenerationApplicationRepository;
use control_plane_contract::D1_CATALOG_BINDING;
use worker::{Env, Error, Result};

const CLIENT_CONTACT_PROTECTION_KEYRING_BINDING: &str = "CLIENT_CONTACT_PROTECTION_KEYRING";

pub fn client_application(env: &Env) -> Result<D1ClientApplicationRepository> {
    Ok(D1ClientApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub fn client_persistence_application(env: &Env) -> Result<D1ClientPersistenceRepository> {
    Ok(D1ClientPersistenceRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_merge_application(env: &Env) -> Result<D1ClientMergeRepository> {
    Ok(D1ClientMergeRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_registry_projection(env: &Env) -> Result<D1ClientRegistryProjectionRepository> {
    Ok(D1ClientRegistryProjectionRepository::new(env.d1(D1_CATALOG_BINDING)?))
}

pub fn client_contact_protection(
    env: &Env,
) -> Result<RustCryptoContactProtection<WorkerCryptoNonceSource>> {
    contact_protection_from_serialized_keyring(
        env.secret(CLIENT_CONTACT_PROTECTION_KEYRING_BINDING)?
            .to_string(),
    )
    .map_err(|_| Error::RustError("invalid client contact protection keyring".to_owned()))
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

pub fn mailbox_job_application(env: &Env) -> Result<D1MailboxJobApplicationRepository> {
    Ok(D1MailboxJobApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}
