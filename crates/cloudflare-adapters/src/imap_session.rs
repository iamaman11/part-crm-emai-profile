use crate::cloud_mailbox_secrets::{ImapCredential, ImapTlsMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use worker::{SecureTransport, Socket};
use zeroize::Zeroize;

const MAX_GREETING_BYTES: usize = 8 * 1024;
const MAX_COMMAND_TAG: u32 = 999_999;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImapTransportError {
    Authentication,
    ProviderPolicy,
    DependencyUnavailable,
    IntegrityFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImapTaggedStatus {
    Ok,
    No,
    Bad,
}

pub(crate) struct ImapCommandResponse {
    bytes: Vec<u8>,
    status: ImapTaggedStatus,
}

impl ImapCommandResponse {
    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub(crate) const fn status(&self) -> ImapTaggedStatus {
        self.status
    }

    #[must_use]
    pub(crate) fn text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

pub(crate) struct ImapSession {
    socket: Socket,
    next_tag: u32,
}

impl ImapSession {
    pub(crate) async fn connect(credential: &ImapCredential) -> Result<Self, ImapTransportError> {
        let transport = match credential.tls() {
            ImapTlsMode::Implicit => SecureTransport::On,
            ImapTlsMode::StartTls => SecureTransport::StartTls,
        };
        let mut socket = Socket::builder()
            .secure_transport(transport)
            .connect(credential.host(), credential.port())
            .map_err(|_| ImapTransportError::DependencyUnavailable)?;
        let greeting = read_until_line(&mut socket, MAX_GREETING_BYTES).await?;
        let preauthenticated = greeting.starts_with(b"* PREAUTH");
        if !greeting.starts_with(b"* OK") && !preauthenticated {
            return Err(ImapTransportError::DependencyUnavailable);
        }

        let mut session = Self {
            socket,
            next_tag: 1,
        };
        if credential.tls() == ImapTlsMode::StartTls {
            let response = session.execute("STARTTLS", MAX_GREETING_BYTES).await?;
            if response.status() != ImapTaggedStatus::Ok {
                return Err(ImapTransportError::ProviderPolicy);
            }
            session.socket = session.socket.start_tls();
        }

        if !preauthenticated {
            let mut login = String::from("LOGIN ");
            push_imap_quoted(&mut login, credential.username());
            login.push(' ');
            push_imap_quoted(&mut login, credential.password());
            let result = session.execute(&login, MAX_GREETING_BYTES).await;
            login.zeroize();
            match result?.status() {
                ImapTaggedStatus::Ok => {}
                ImapTaggedStatus::No | ImapTaggedStatus::Bad => {
                    return Err(ImapTransportError::Authentication);
                }
            }
        }
        Ok(session)
    }

    pub(crate) async fn execute(
        &mut self,
        command: &str,
        maximum_response_bytes: usize,
    ) -> Result<ImapCommandResponse, ImapTransportError> {
        if command.is_empty()
            || command
                .bytes()
                .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
            || maximum_response_bytes == 0
        {
            return Err(ImapTransportError::IntegrityFailure);
        }
        let tag = self.next_command_tag()?;
        let mut wire = String::with_capacity(tag.len() + 1 + command.len() + 2);
        wire.push_str(&tag);
        wire.push(' ');
        wire.push_str(command);
        wire.push_str("\r\n");
        self.socket
            .write_all(wire.as_bytes())
            .await
            .map_err(|_| ImapTransportError::DependencyUnavailable)?;
        self.socket
            .flush()
            .await
            .map_err(|_| ImapTransportError::DependencyUnavailable)?;
        let bytes = read_until_tag(&mut self.socket, &tag, maximum_response_bytes).await?;
        let status = tagged_status(&bytes, &tag).ok_or(ImapTransportError::IntegrityFailure)?;
        Ok(ImapCommandResponse { bytes, status })
    }

    fn next_command_tag(&mut self) -> Result<String, ImapTransportError> {
        if self.next_tag > MAX_COMMAND_TAG {
            return Err(ImapTransportError::IntegrityFailure);
        }
        let tag = format!("p{:06}", self.next_tag);
        self.next_tag = self
            .next_tag
            .checked_add(1)
            .ok_or(ImapTransportError::IntegrityFailure)?;
        Ok(tag)
    }
}

async fn read_until_line(
    socket: &mut Socket,
    maximum_response_bytes: usize,
) -> Result<Vec<u8>, ImapTransportError> {
    read_bounded(socket, None, maximum_response_bytes).await
}

async fn read_until_tag(
    socket: &mut Socket,
    tag: &str,
    maximum_response_bytes: usize,
) -> Result<Vec<u8>, ImapTransportError> {
    read_bounded(socket, Some(tag.as_bytes()), maximum_response_bytes).await
}

async fn read_bounded(
    socket: &mut Socket,
    tag: Option<&[u8]>,
    maximum_response_bytes: usize,
) -> Result<Vec<u8>, ImapTransportError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket
            .read(&mut buffer)
            .await
            .map_err(|_| ImapTransportError::DependencyUnavailable)?;
        if read == 0 {
            return Err(ImapTransportError::DependencyUnavailable);
        }
        if output.len().saturating_add(read) > maximum_response_bytes {
            return Err(ImapTransportError::ProviderPolicy);
        }
        output.extend_from_slice(&buffer[..read]);
        let complete = match tag {
            Some(tag) => response_has_tagged_line(&output, tag),
            None => output.windows(2).any(|window| window == b"\r\n"),
        };
        if complete {
            return Ok(output);
        }
    }
}

fn response_has_tagged_line(response: &[u8], tag: &[u8]) -> bool {
    response
        .split(|byte| *byte == b'\n')
        .any(|line| line.starts_with(tag) && line.get(tag.len()) == Some(&b' '))
}

fn tagged_status(response: &[u8], tag: &str) -> Option<ImapTaggedStatus> {
    let tag = tag.as_bytes();
    let line = response
        .split(|byte| *byte == b'\n')
        .find(|line| line.starts_with(tag) && line.get(tag.len()) == Some(&b' '))?;
    let suffix = line.get(tag.len() + 1..)?;
    let status = suffix
        .split(|byte| byte.is_ascii_whitespace())
        .next()
        .unwrap_or_default();
    if status.eq_ignore_ascii_case(b"OK") {
        Some(ImapTaggedStatus::Ok)
    } else if status.eq_ignore_ascii_case(b"NO") {
        Some(ImapTaggedStatus::No)
    } else if status.eq_ignore_ascii_case(b"BAD") {
        Some(ImapTaggedStatus::Bad)
    } else {
        None
    }
}

pub(crate) fn push_imap_quoted(output: &mut String, value: &str) {
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
    use super::{ImapTaggedStatus, push_imap_quoted, tagged_status};

    #[test]
    fn tagged_status_is_case_insensitive_and_tag_scoped() {
        let response = b"* STATUS INBOX (MESSAGES 1)\r\np000001 OK done\r\n";
        assert_eq!(
            tagged_status(response, "p000001"),
            Some(ImapTaggedStatus::Ok)
        );
        assert_eq!(tagged_status(response, "p000002"), None);
    }

    #[test]
    fn quoted_values_escape_protocol_delimiters() {
        let mut output = String::new();
        push_imap_quoted(&mut output, "user\\\"name");
        assert_eq!(output, "\"user\\\\\\\"name\"");
    }
}
