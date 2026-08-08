use cloudflare_adapters::access_identity::VerifiedExternalIdentity;
use cloudflare_adapters::d1_clients::D1ClientApplicationRepository;
use cloudflare_adapters::d1_identity_ceremonies::D1IdentityCeremonyApplicationRepository;
use cloudflare_adapters::d1_identity_governance::D1IdentityGovernanceApplicationRepository;
use cloudflare_adapters::d1_mailbox_bindings::D1MailboxBindingApplicationRepository;
use cloudflare_adapters::d1_mailbox_jobs::D1MailboxJobApplicationRepository;
use cloudflare_adapters::d1_profile_generation_application::D1ProfileGenerationApplicationRepository;
use cloudflare_adapters::d1_profiles::D1ProfileApplicationRepository;
use control_plane_contract::D1_CATALOG_BINDING;
use worker::{Env, Result};

pub fn client_application(env: &Env) -> Result<D1ClientApplicationRepository> {
    Ok(D1ClientApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
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

pub fn profile_application(env: &Env) -> Result<D1ProfileApplicationRepository> {
    Ok(D1ProfileApplicationRepository::new(
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
