mod auth;

use crate::smtp_send_credential::{SmtpCredential, SmtpTlsMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use worker::{SecureTransport, Socket};
use zeroize::Zeroize;

const MAX_REPLY_BYTES: usize = 64 * 1024;
const CLIENT_IDENTITY: &str = "profile.invalid";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SmtpSendFailure {
    RetryableNotSent,
    Rejected,
    Ambiguous,
    IntegrityFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SmtpReply {
    pub(super) code: u16,
    bytes: Vec<u8>,
}

impl SmtpReply {
    fn capability(&self, capability: &str) -> bool {
        let wanted = capability.as_bytes();
        self.bytes.split(|byte| *byte == b'\n').any(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let payload = line.get(4..).unwrap_or_default();
            payload
                .split(|byte| byte.is_ascii_whitespace())
                .any(|token| token.eq_ignore_ascii_case(wanted))
        })
    }

    pub(super) fn auth_mechanism(&self, mechanism: &str) -> bool {
        let wanted = mechanism.as_bytes();
        self.bytes.split(|byte| *byte == b'\n').any(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let payload = line.get(4..).unwrap_or_default();
            let mut tokens = payload.split(|byte| byte.is_ascii_whitespace());
            let Some(first) = tokens.next() else {
                return false;
            };
            first
                .strip_prefix(b"AUTH=")
                .is_some_and(|value| value.eq_ignore_ascii_case(wanted))
                || (first.eq_ignore_ascii_case(b"AUTH")
                    && tokens.any(|token| token.eq_ignore_ascii_case(wanted)))
        })
    }
}

pub(crate) struct SmtpSession {
    socket: Socket,
    ehlo: SmtpReply,
}

impl SmtpSession {
    pub(crate) async fn connect(credential: &SmtpCredential) -> Result<Self, SmtpSendFailure> {
        let transport = match credential.tls() {
            SmtpTlsMode::Implicit => SecureTransport::On,
            SmtpTlsMode::StartTls => SecureTransport::StartTls,
        };
        let mut socket = Socket::builder()
            .secure_transport(transport)
            .connect(credential.host(), credential.port())
            .map_err(|_| SmtpSendFailure::RetryableNotSent)?;
        let greeting = read_reply(&mut socket).await?;
        if greeting.code != 220 {
            return Err(classify_pre_acceptance(greeting.code));
        }

        let ehlo = send_command(&mut socket, &format!("EHLO {CLIENT_IDENTITY}"), false).await?;
        if ehlo.code != 250 {
            return Err(classify_pre_acceptance(ehlo.code));
        }
        let mut session = Self { socket, ehlo };
        if credential.tls() == SmtpTlsMode::StartTls {
            if !session.ehlo.capability("STARTTLS") {
                return Err(SmtpSendFailure::Rejected);
            }
            let starttls = send_command(&mut session.socket, "STARTTLS", false).await?;
            if starttls.code != 220 {
                return Err(classify_pre_acceptance(starttls.code));
            }
            session.socket = session.socket.start_tls();
            session.ehlo = send_command(
                &mut session.socket,
                &format!("EHLO {CLIENT_IDENTITY}"),
                false,
            )
            .await?;
            if session.ehlo.code != 250 {
                return Err(classify_pre_acceptance(session.ehlo.code));
            }
        }
        auth::authenticate(&mut session.socket, &session.ehlo, credential).await?;
        Ok(session)
    }

    pub(crate) async fn send_message(
        &mut self,
        envelope_from: &str,
        recipients: &[String],
        message: &[u8],
    ) -> Result<(), SmtpSendFailure> {
        if recipients.is_empty()
            || invalid_path(envelope_from)
            || recipients.iter().any(|value| invalid_path(value))
            || message.is_empty()
        {
            return Err(SmtpSendFailure::IntegrityFailure);
        }
        let requires_smtp_utf8 =
            !envelope_from.is_ascii() || recipients.iter().any(|recipient| !recipient.is_ascii());
        if requires_smtp_utf8 && !self.ehlo.capability("SMTPUTF8") {
            return Err(SmtpSendFailure::Rejected);
        }
        let mail_from = mail_from_command(envelope_from, requires_smtp_utf8);
        let reply = send_command(&mut self.socket, &mail_from, false).await?;
        if reply.code != 250 {
            return Err(classify_pre_acceptance(reply.code));
        }
        for recipient in recipients {
            let reply =
                send_command(&mut self.socket, &format!("RCPT TO:<{recipient}>"), false).await?;
            if !matches!(reply.code, 250 | 251) {
                let failure = classify_pre_acceptance(reply.code);
                let _ = send_command(&mut self.socket, "RSET", false).await;
                return Err(failure);
            }
        }
        let data = send_command(&mut self.socket, "DATA", false).await?;
        if data.code != 354 {
            return Err(classify_pre_acceptance(data.code));
        }

        let mut wire = dot_stuffed_message(message)?;
        let write_result = self.socket.write_all(&wire).await;
        let transfer_result = if write_result.is_ok() {
            self.socket.flush().await
        } else {
            write_result
        };
        wire.zeroize();
        if transfer_result.is_err() {
            return Err(SmtpSendFailure::Ambiguous);
        }

        let final_reply = read_reply_after_data(&mut self.socket).await?;
        match final_reply.code {
            200..=299 => {
                let _ = send_command(&mut self.socket, "QUIT", false).await;
                Ok(())
            }
            400..=499 => Err(SmtpSendFailure::RetryableNotSent),
            500..=599 => Err(SmtpSendFailure::Rejected),
            _ => Err(SmtpSendFailure::Ambiguous),
        }
    }
}

pub(super) fn classify_auth_reply(code: u16) -> Result<(), SmtpSendFailure> {
    match code {
        235 => Ok(()),
        400..=499 => Err(SmtpSendFailure::RetryableNotSent),
        500..=599 => Err(SmtpSendFailure::Rejected),
        _ => Err(SmtpSendFailure::IntegrityFailure),
    }
}

const fn classify_pre_acceptance(code: u16) -> SmtpSendFailure {
    match code {
        400..=499 => SmtpSendFailure::RetryableNotSent,
        500..=599 => SmtpSendFailure::Rejected,
        _ => SmtpSendFailure::IntegrityFailure,
    }
}

fn mail_from_command(envelope_from: &str, requires_smtp_utf8: bool) -> String {
    if requires_smtp_utf8 {
        format!("MAIL FROM:<{envelope_from}> SMTPUTF8")
    } else {
        format!("MAIL FROM:<{envelope_from}>")
    }
}

pub(super) async fn send_command(
    socket: &mut Socket,
    command: &str,
    secret: bool,
) -> Result<SmtpReply, SmtpSendFailure> {
    if command
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        return Err(SmtpSendFailure::IntegrityFailure);
    }
    let capacity = command
        .len()
        .checked_add(2)
        .ok_or(SmtpSendFailure::IntegrityFailure)?;
    let mut wire = String::with_capacity(capacity);
    wire.push_str(command);
    wire.push_str("\r\n");
    let write = socket.write_all(wire.as_bytes()).await;
    let flush = if write.is_ok() {
        socket.flush().await
    } else {
        write
    };
    if secret {
        wire.zeroize();
    }
    flush.map_err(|_| SmtpSendFailure::RetryableNotSent)?;
    read_reply(socket).await
}

async fn read_reply(socket: &mut Socket) -> Result<SmtpReply, SmtpSendFailure> {
    read_reply_inner(socket, SmtpSendFailure::RetryableNotSent).await
}

async fn read_reply_after_data(socket: &mut Socket) -> Result<SmtpReply, SmtpSendFailure> {
    read_reply_inner(socket, SmtpSendFailure::Ambiguous).await
}

async fn read_reply_inner(
    socket: &mut Socket,
    io_failure: SmtpSendFailure,
) -> Result<SmtpReply, SmtpSendFailure> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket.read(&mut buffer).await.map_err(|_| io_failure)?;
        let next_length = output.len().checked_add(read).ok_or(io_failure)?;
        if read == 0 || next_length > MAX_REPLY_BYTES {
            return Err(io_failure);
        }
        output.extend_from_slice(&buffer[..read]);
        if let Some(code) = complete_reply_code(&output) {
            return Ok(SmtpReply {
                code,
                bytes: output,
            });
        }
    }
}

fn complete_reply_code(response: &[u8]) -> Option<u16> {
    let first = response.split(|byte| *byte == b'\n').next()?;
    let code = parse_reply_code(first)?;
    let prefix = code.to_string();
    let prefix = prefix.as_bytes();
    response.split(|byte| *byte == b'\n').find_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        (line.len() >= 4 && line.get(..3) == Some(prefix) && line[3] == b' ').then_some(code)
    })
}

fn parse_reply_code(line: &[u8]) -> Option<u16> {
    let digits = line.get(..3)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    core::str::from_utf8(digits).ok()?.parse::<u16>().ok()
}

fn dot_stuffed_message(message: &[u8]) -> Result<Vec<u8>, SmtpSendFailure> {
    if message.contains(&b'\0') {
        return Err(SmtpSendFailure::IntegrityFailure);
    }
    let capacity = message
        .len()
        .checked_add(16)
        .ok_or(SmtpSendFailure::IntegrityFailure)?;
    let mut output = Vec::with_capacity(capacity);
    let mut line_start = true;
    for byte in message.iter().copied() {
        if line_start && byte == b'.' {
            output.push(b'.');
        }
        output.push(byte);
        line_start = byte == b'\n';
    }
    if !output.ends_with(b"\r\n") {
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b".\r\n");
    Ok(output)
}

fn invalid_path(value: &str) -> bool {
    value.is_empty()
        || value.bytes().any(|byte| {
            byte.is_ascii_control() || byte.is_ascii_whitespace() || matches!(byte, b'<' | b'>')
        })
}

#[cfg(test)]
mod tests {
    use super::{
        SmtpReply, SmtpSendFailure, classify_pre_acceptance, complete_reply_code,
        dot_stuffed_message, mail_from_command,
    };

    #[test]
    fn multiline_reply_requires_terminal_space_line() {
        assert_eq!(complete_reply_code(b"250-one\r\n250-two\r\n"), None);
        assert_eq!(complete_reply_code(b"250-one\r\n250 two\r\n"), Some(250));
    }

    #[test]
    fn pre_acceptance_statuses_preserve_safe_retry_boundary() {
        assert_eq!(
            classify_pre_acceptance(421),
            SmtpSendFailure::RetryableNotSent
        );
        assert_eq!(classify_pre_acceptance(550), SmtpSendFailure::Rejected);
    }

    #[test]
    fn smtp_utf8_capability_and_mail_from_are_explicit() {
        let ehlo = SmtpReply {
            code: 250,
            bytes: b"250-example.test\r\n250-SMTPUTF8\r\n250 AUTH PLAIN\r\n".to_vec(),
        };
        assert!(ehlo.capability("SMTPUTF8"));
        assert_eq!(
            mail_from_command("user+utf8@example.com", true),
            "MAIL FROM:<user+utf8@example.com> SMTPUTF8"
        );
        assert_eq!(
            mail_from_command("user@example.com", false),
            "MAIL FROM:<user@example.com>"
        );
    }

    #[test]
    fn data_is_dot_stuffed_and_terminated() -> Result<(), Box<dyn std::error::Error>> {
        let wire = dot_stuffed_message(b"one\r\n.two\r\nthree")
            .map_err(|_| std::io::Error::other("dot stuffing failed"))?;
        assert_eq!(wire, b"one\r\n..two\r\nthree\r\n.\r\n");
        Ok(())
    }
}
