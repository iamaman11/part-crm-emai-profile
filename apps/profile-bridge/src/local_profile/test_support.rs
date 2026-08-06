use super::LocalProfileError;
use profile_platform_primitives::{ProfileId, TenantId};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn test_root(label: &str) -> Result<PathBuf, LocalProfileError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LocalProfileError::ClockRegression)?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "part-crm-step8-{label}-{}-{nonce}",
        std::process::id()
    )))
}

pub(super) fn ids() -> Result<(TenantId, ProfileId), Box<dyn std::error::Error>> {
    Ok((
        TenantId::parse("tenant_01JSTEP8")?,
        ProfileId::parse("profile_01JSTEP8")?,
    ))
}

pub(super) fn cleanup(path: PathBuf) -> Result<(), LocalProfileError> {
    fs::remove_dir_all(path)?;
    Ok(())
}
