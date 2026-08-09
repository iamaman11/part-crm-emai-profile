use crate::MailboxError;

const MAX_PROVIDER_STATUS_LENGTH: usize = 64;

pub fn validate_provider_status(value: &str) -> Result<(), MailboxError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_STATUS_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(MailboxError::InvalidProviderStatus);
    }
    Ok(())
}
