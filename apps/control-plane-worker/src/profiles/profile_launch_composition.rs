use cloudflare_adapters::d1_profile_launch::D1ProfileLaunchContext;
use cloudflare_adapters::d1_profile_launch_authority::D1ProfileLaunchAuthority;
use control_plane_contract::D1_CATALOG_BINDING;
use worker::{Env, Error, Result};

const PROFILE_LAUNCH_CLAIM_KEY: &str = "PROFILE_LAUNCH_CLAIM_KEY";

pub(super) fn launch_context(env: &Env) -> Result<D1ProfileLaunchContext> {
    Ok(D1ProfileLaunchContext::new(env.d1(D1_CATALOG_BINDING)?))
}

pub(super) fn launch_authority(env: &Env) -> Result<D1ProfileLaunchAuthority> {
    let key = env.secret(PROFILE_LAUNCH_CLAIM_KEY)?.to_string();
    D1ProfileLaunchAuthority::new(env.d1(D1_CATALOG_BINDING)?, key).map_err(|error| {
        Error::RustError(format!("profile launch authority configuration failed closed: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::PROFILE_LAUNCH_CLAIM_KEY;

    #[test]
    fn launch_claim_secret_is_dedicated_to_profile_runtime() {
        assert_eq!(PROFILE_LAUNCH_CLAIM_KEY, "PROFILE_LAUNCH_CLAIM_KEY");
        assert!(!PROFILE_LAUNCH_CLAIM_KEY.contains("RESOLVER"));
        assert!(!PROFILE_LAUNCH_CLAIM_KEY.contains("MAILBOX"));
    }
}
