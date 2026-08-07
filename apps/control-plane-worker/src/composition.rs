use cloudflare_adapters::d1_clients::D1ClientApplicationRepository;
use control_plane_contract::D1_CATALOG_BINDING;
use worker::{Env, Result};

pub fn client_application(env: &Env) -> Result<D1ClientApplicationRepository> {
    Ok(D1ClientApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}
