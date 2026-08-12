use cloudflare_adapters::d1_mailbox_client_associations::D1MailboxClientAssociationApplicationRepository;
use control_plane_contract::D1_CATALOG_BINDING;
use worker::{Env, Result};

pub fn mailbox_client_association_application(
    env: &Env,
) -> Result<D1MailboxClientAssociationApplicationRepository> {
    Ok(D1MailboxClientAssociationApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}
