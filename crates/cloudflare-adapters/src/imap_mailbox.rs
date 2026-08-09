use crate::cloud_mailbox_secrets::{ImapCredential, provider_error};
use crate::imap_session::{ImapSession, ImapTaggedStatus, ImapTransportError};
use application_ports::mailboxes::{MailboxObservation, MailboxProviderPortError};
use mailbox_domain::{MailboxBinding, MailboxProviderFailureClass};

const MAX_IMAP_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_OBSERVED_ITEMS: u32 = 10_000;

pub async fn check_imap_mailbox(
    binding: &MailboxBinding,
    credential: &ImapCredential,
) -> Result<MailboxObservation, MailboxProviderPortError> {
    let mut session = ImapSession::connect(credential)
        .await
        .map_err(map_transport_error)?;
    let response = session
        .execute("STATUS INBOX (MESSAGES UIDNEXT)", MAX_IMAP_RESPONSE_BYTES)
        .await
        .map_err(map_transport_error)?;
    match response.status() {
        ImapTaggedStatus::Ok => {}
        ImapTaggedStatus::No | ImapTaggedStatus::Bad => {
            return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
        }
    }
    let text = response.text_lossy();
    let (messages, uid_next) = parse_status_observation(&text)
        .ok_or_else(|| provider_error(MailboxProviderFailureClass::ProviderPolicy))?;
    let bounded_item_count = messages.min(MAX_OBSERVED_ITEMS);
    MailboxObservation::new(
        binding.binding_id().clone(),
        "IMAP_OK",
        bounded_item_count,
        Some(uid_next.to_string()),
    )
    .map_err(|_| MailboxProviderPortError::IntegrityFailure)
}

fn map_transport_error(error: ImapTransportError) -> MailboxProviderPortError {
    match error {
        ImapTransportError::Authentication => {
            provider_error(MailboxProviderFailureClass::Authentication)
        }
        ImapTransportError::ProviderPolicy => {
            provider_error(MailboxProviderFailureClass::ProviderPolicy)
        }
        ImapTransportError::DependencyUnavailable => {
            provider_error(MailboxProviderFailureClass::TransientDependency)
        }
        ImapTransportError::IntegrityFailure => MailboxProviderPortError::IntegrityFailure,
    }
}

fn parse_status_observation(response: &str) -> Option<(u32, u64)> {
    let status_line = response
        .lines()
        .find(|line| line.starts_with("* STATUS "))?;
    let start = status_line.find('(')?;
    let end = status_line.rfind(')')?;
    if end <= start {
        return None;
    }
    let mut messages = None;
    let mut uid_next = None;
    let mut tokens = status_line[start + 1..end].split_ascii_whitespace();
    while let Some(name) = tokens.next() {
        let value = tokens.next()?;
        if name.eq_ignore_ascii_case("MESSAGES") {
            messages = value.parse::<u32>().ok();
        } else if name.eq_ignore_ascii_case("UIDNEXT") {
            uid_next = value.parse::<u64>().ok();
        }
    }
    Some((messages?, uid_next?))
}

#[cfg(test)]
mod tests {
    use super::parse_status_observation;

    #[test]
    fn status_parser_extracts_only_count_and_uid_cursor() {
        let response = "* STATUS INBOX (MESSAGES 42 UIDNEXT 99)\r\np000003 OK STATUS completed\r\n";
        assert_eq!(parse_status_observation(response), Some((42, 99)));
    }
}
