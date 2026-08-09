use crate::cloud_mailbox_secrets::{ImapCredential, ImapTlsMode, provider_error};
use application_ports::mailboxes::MailboxProviderPortError;
use mailbox_domain::{MailboxBinding, MailboxObservation, MailboxProviderFailureClass};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use worker::{SecureTransport, Socket};
use zeroize::Zeroize;

const MAX_IMAP_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_OBSERVED_ITEMS: u32 = 10_000;

pub async fn check_imap_mailbox(
    binding: &MailboxBinding,
    credential: &ImapCredential,
) -> Result<MailboxObservation, MailboxProviderPortError> {
    let transport = match credential.tls() {
        ImapTlsMode::Implicit => SecureTransport::On,
        ImapTlsMode::StartTls => SecureTransport::StartTls,
    };
    let mut socket = Socket::builder()
        .secure_transport(transport)
        .connect(credential.host(), credential.port())
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;

    let greeting = read_until_line(&mut socket).await?;
    let preauthenticated = greeting.starts_with("* PREAUTH");
    if !greeting.starts_with("* OK") && !preauthenticated {
        return Err(provider_error(
            MailboxProviderFailureClass::TransientDependency,
        ));
    }

    if credential.tls() == ImapTlsMode::StartTls {
        write_command(&mut socket, "a0 STARTTLS\r\n").await?;
        let response = read_until_tag(&mut socket, "a0").await?;
        if tagged_status(&response, "a0") != Some("OK") {
            return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
        }
        socket = socket
            .start_tls()
            .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    }

    if !preauthenticated {
        let mut login = String::from("a1 LOGIN ");
        push_imap_quoted(&mut login, credential.username());
        login.push(' ');
        push_imap_quoted(&mut login, credential.password());
        login.push_str("\r\n");
        let write_result = write_command(&mut socket, &login).await;
        login.zeroize();
        write_result?;
        let login_response = read_until_tag(&mut socket, "a1").await?;
        match tagged_status(&login_response, "a1") {
            Some("OK") => {}
            Some("NO" | "BAD") => {
                return Err(provider_error(MailboxProviderFailureClass::Authentication));
            }
            _ => {
                return Err(provider_error(
                    MailboxProviderFailureClass::TransientDependency,
                ));
            }
        }
    }

    write_command(&mut socket, "a2 STATUS INBOX (MESSAGES UIDNEXT)\r\n").await?;
    let status_response = read_until_tag(&mut socket, "a2").await?;
    match tagged_status(&status_response, "a2") {
        Some("OK") => {}
        Some("NO" | "BAD") => {
            return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
        }
        _ => {
            return Err(provider_error(
                MailboxProviderFailureClass::TransientDependency,
            ));
        }
    }
    let (messages, uid_next) = parse_status_observation(&status_response)
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

async fn write_command(socket: &mut Socket, command: &str) -> Result<(), MailboxProviderPortError> {
    socket
        .write_all(command.as_bytes())
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    socket
        .flush()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))
}

async fn read_until_line(socket: &mut Socket) -> Result<String, MailboxProviderPortError> {
    read_bounded(socket, None).await
}

async fn read_until_tag(
    socket: &mut Socket,
    tag: &str,
) -> Result<String, MailboxProviderPortError> {
    read_bounded(socket, Some(tag)).await
}

async fn read_bounded(
    socket: &mut Socket,
    tag: Option<&str>,
) -> Result<String, MailboxProviderPortError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = socket
            .read(&mut buffer)
            .await
            .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
        if read == 0 {
            return Err(provider_error(
                MailboxProviderFailureClass::TransientDependency,
            ));
        }
        if output.len().saturating_add(read) > MAX_IMAP_RESPONSE_BYTES {
            return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
        }
        output.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&output);
        let complete = match tag {
            Some(tag) => response_has_tagged_line(&text, tag),
            None => text.contains("\r\n"),
        };
        if complete {
            return Ok(text.into_owned());
        }
    }
}

fn response_has_tagged_line(response: &str, tag: &str) -> bool {
    response.lines().any(|line| {
        line.strip_prefix(tag)
            .is_some_and(|suffix| suffix.starts_with(' '))
    })
}

fn tagged_status<'a>(response: &'a str, tag: &str) -> Option<&'a str> {
    response.lines().find_map(|line| {
        let suffix = line.strip_prefix(tag)?.strip_prefix(' ')?;
        suffix.split_ascii_whitespace().next()
    })
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

fn push_imap_quoted(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '\\') {
            output.push('\\');
        }
        output.push(character);
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::{parse_status_observation, push_imap_quoted, tagged_status};

    #[test]
    fn status_parser_extracts_only_count_and_uid_cursor() {
        let response = "* STATUS INBOX (MESSAGES 42 UIDNEXT 99)\r\na2 OK STATUS completed\r\n";
        assert_eq!(parse_status_observation(response), Some((42, 99)));
        assert_eq!(tagged_status(response, "a2"), Some("OK"));
    }

    #[test]
    fn quoted_credentials_cannot_inject_imap_commands() {
        let mut output = String::new();
        push_imap_quoted(&mut output, "user\\\"name");
        assert_eq!(output, "\"user\\\\\\\"name\"");
    }
}
