use crate::smtp_send_credential::{SmtpAuthenticationMode, SmtpCredential, SmtpTlsMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use worker::{SecureTransport, Socket};
use zeroize::Zeroize;

const MAX_REPLY_BYTES: usize = 64 * 1024;
const MAX_AUTH_WIRE_BYTES: usize = 24 * 1024;
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const CLIENT_IDENTITY: &str = "profile.invalid";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SmtpSendFailure {
    RetryableNotSent,
    Rejected,
    Ambiguous,
    IntegrityFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SmtpReply {
    code: u16,
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

    fn auth_mechanism(&self, mechanism: &str) -> bool {
        let wanted = mechanism.as_bytes();
        self.bytes.split(|byte| *byte == b'\n').any(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let payload = line.get(4..).unwrap_or_default();
            let mut tokens = payload.split(|byte| byte.is_ascii_whitespace());
            let Some(first) = tokens.next() else {
                return false;
            };
            let auth_line = first.eq_ignore_ascii_case(b"AUTH")
                || first
                    .strip_prefix(b"AUTH=")
                    .is_some_and(|value| value.eq_ignore_ascii_case(wanted));
            auth_line && (first.get(5..).is_some_and(|value| value.eq_ignore_ascii_case(wanted))
                || tokens.any(|token| token.eq_ignore_ascii_case(wanted)))
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
        require_pre_acceptance(greeting.code)?;
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
        session.authenticate(credential).await?;
        Ok(session)
    }

    async fn authenticate(&mut self, credential: &SmtpCredential) -> Result<(), SmtpSendFailure> {
        match credential.authentication_mode() {
            SmtpAuthenticationMode::Password => {
                let password = credential
                    .password()
                    .ok_or(SmtpSendFailure::IntegrityFailure)?;
                if self.ehlo.auth_mechanism("PLAIN") {
                    self.auth_plain(credential.username(), password).await
                } else if self.ehlo.auth_mechanism("LOGIN") {
                    self.auth_login(credential.username(), password).await
                } else {
                    Err(SmtpSendFailure::Rejected)
                }
            }
            SmtpAuthenticationMode::Xoauth2 => {
                let token = credential
                    .access_token()
                    .ok_or(SmtpSendFailure::IntegrityFailure)?;
                if !self.ehlo.auth_mechanism("XOAUTH2") {
                    return Err(SmtpSendFailure::Rejected);
                }
                self.auth_xoauth2(credential.username(), token).await
            }
        }
    }

    async fn auth_plain(&mut self, username: &str, password: &str) -> Result<(), SmtpSendFailure> {
        let mut payload = Vec::with_capacity(username.len() + password.len() + 2);
        payload.push(0);
        payload.extend_from_slice(username.as_bytes());
        payload.push(0);
        payload.extend_from_slice(password.as_bytes());
        let mut encoded = base64_standard(&payload)?;
        payload.zeroize();
        if encoded.len() > MAX_AUTH_WIRE_BYTES {
            encoded.zeroize();
            return Err(SmtpSendFailure::Rejected);
        }
        let mut command = String::with_capacity(encoded.len() + 11);
        command.push_str("AUTH PLAIN ");
        command.push_str(&encoded);
        let result = send_command(&mut self.socket, &command, true).await;
        command.zeroize();
        encoded.zeroize();
        let reply = result?;
        classify_auth_reply(reply.code)
    }

    async fn auth_login(&mut self, username: &str, password: &str) -> Result<(), SmtpSendFailure> {
        let reply = send_command(&mut self.socket, "AUTH LOGIN", false).await?;
        if reply.code != 334 {
            return classify_auth_reply(reply.code);
        }
        let mut encoded_username = base64_standard(username.as_bytes())?;
        let username_reply = send_command(&mut self.socket, &encoded_username, true).await;
        encoded_username.zeroize();
        let username_reply = username_reply?;
        if username_reply.code != 334 {
            return classify_auth_reply(username_reply.code);
        }
        let mut encoded_password = base64_standard(password.as_bytes())?;
        let password_reply = send_command(&mut self.socket, &encoded_password, true).await;
        encoded_password.zeroize();
        classify_auth_reply(password_reply?.code)
    }

    async fn auth_xoauth2(&mut self, username: &str, token: &str) -> Result<(), SmtpSendFailure> {
        let capacity = username
            .len()
            .checked_add(token.len())
            .and_then(|value| value.checked_add(20))
            .ok_or(SmtpSendFailure::IntegrityFailure)?;
        let mut payload = String::with_capacity(capacity);
        payload.push_str("user=");
        payload.push_str(username);
        payload.push('\u{1}');
        payload.push_str("auth=Bearer ");
        payload.push_str(token);
        payload.push('\u{1}');
        payload.push('\u{1}');
        let mut encoded = base64_standard(payload.as_bytes())?;
        payload.zeroize();
        if encoded.len() > MAX_AUTH_WIRE_BYTES {
            encoded.zeroize();
            return Err(SmtpSendFailure::Rejected);
        }
        let mut command = String::with_capacity(encoded.len() + 13);
        command.push_str("AUTH XOAUTH2 ");
        command.push_str(&encoded);
        let result = send_command(&mut self.socket, &command, true).await;
        command.zeroize();
        encoded.zeroize();
        let reply = result?;
        if reply.code == 334 {
            let final_reply = send_command(&mut self.socket, "", true).await?;
            return classify_auth_reply(final_reply.code);
        }
        classify_auth_reply(reply.code)
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
        let mail_from = format!("MAIL FROM:<{envelope_from}>");
        let reply = send_command(&mut self.socket, &mail_from, false).await?;
        if reply.code != 250 {
            return Err(classify_pre_acceptance(reply.code));
        }
        for recipient in recipients {
            let command = format!("RCPT TO:<{recipient}>");
            let reply = send_command(&mut self.socket, &command, false).await?;
            if reply.code != 250 && reply.code != 251 {
                let failure = classify_pre_acceptance(reply.code);
                let _ = send_command(&mut self.socket, "RSET", false).await;
                return Err(failure);
            }
        }
        let data = send_command(&mut self.socket, "DATA", false).await?;
        if data.code != 354 {
            return Err(classify_pre_acceptance(data.code));
        }

        let wire = dot_stuffed_message(message)?;
        if self.socket.write_all(&wire).await.is_err() || self.socket.flush().await.is_err() {
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

fn classify_auth_reply(code: u16) -> Result<(), SmtpSendFailure> {
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

const fn require_pre_acceptance(code: u16) -> Result<(), SmtpSendFailure> {
    match code {
        200..=599 => Ok(()),
        _ => Err(SmtpSendFailure::IntegrityFailure),
    }
}

async fn send_command(
    socket: &mut Socket,
    command: &str,
    secret: bool,
) -> Result<SmtpReply, SmtpSendFailure> {
    if command.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | b'\0')) {
        return Err(SmtpSendFailure::IntegrityFailure);
    }
    let mut wire = String::with_capacity(command.len() + 2);
    wire.push_str(command);
    wire.push_str("\r\n");
    let write = socket.write_all(wire.as_bytes()).await;
    let flush = if write.is_ok() { socket.flush().await } else { write };
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
        if read == 0 {
            return Err(io_failure);
        }
        if output.len().saturating_add(read) > MAX_REPLY_BYTES {
            return Err(io_failure);
        }
        output.extend_from_slice(&buffer[..read]);
        if let Some(code) = complete_reply_code(&output) {
            return Ok(SmtpReply { code, bytes: output });
        }
    }
}

fn complete_reply_code(response: &[u8]) -> Option<u16> {
    let first = response.split(|byte| *byte == b'\n').next()?;
    let code = parse_reply_code(first)?;
    let prefix = code.to_string();
    let prefix = prefix.as_bytes();
    for line in response.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() >= 4 && line[..3] == *prefix && line[3] == b' ' {
            return Some(code);
        }
    }
    None
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
    let mut output = Vec::with_capacity(message.len().saturating_add(16));
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
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || matches!(byte, b'<' | b'>'))
}

fn base64_standard(input: &[u8]) -> Result<String, SmtpSendFailure> {
    let groups = input
        .len()
        .checked_add(2)
        .and_then(|value| value.checked_div(3))
        .ok_or(SmtpSendFailure::IntegrityFailure)?;
    let capacity = groups
        .checked_mul(4)
        .ok_or(SmtpSendFailure::IntegrityFailure)?;
    let mut output = String::with_capacity(capacity);
    let mut offset = 0;
    while offset < input.len() {
        let remaining = input.len() - offset;
        let a = input[offset];
        let b = if remaining > 1 { input[offset + 1] } else { 0 };
        let c = if remaining > 2 { input[offset + 2] } else { 0 };
        output.push(BASE64_ALPHABET[usize::from(a >> 2)] as char);
        output.push(BASE64_ALPHABET[usize::from(((a & 0x03) << 4) | (b >> 4))] as char);
        if remaining > 1 {
            output.push(BASE64_ALPHABET[usize::from(((b & 0x0f) << 2) | (c >> 6))] as char);
        } else {
            output.push('=');
        }
        if remaining > 2 {
            output.push(BASE64_ALPHABET[usize::from(c & 0x3f)] as char);
        } else {
            output.push('=');
        }
        offset += 3;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{
        SmtpSendFailure, classify_pre_acceptance, complete_reply_code, dot_stuffed_message,
    };

    #[test]
    fn multiline_reply_requires_terminal_space_line() {
        assert_eq!(complete_reply_code(b"250-one\r\n250-two\r\n"), None);
        assert_eq!(complete_reply_code(b"250-one\r\n250 two\r\n"), Some(250));
    }

    #[test]
    fn pre_acceptance_statuses_preserve_safe_retry_boundary() {
        assert_eq!(classify_pre_acceptance(421), SmtpSendFailure::RetryableNotSent);
        assert_eq!(classify_pre_acceptance(450), SmtpSendFailure::RetryableNotSent);
        assert_eq!(classify_pre_acceptance(550), SmtpSendFailure::Rejected);
        assert_eq!(classify_pre_acceptance(250), SmtpSendFailure::IntegrityFailure);
    }

    #[test]
    fn data_is_dot_stuffed_and_terminated() -> Result<(), Box<dyn std::error::Error>> {
        let wire = dot_stuffed_message(b"one\r\n.two\r\nthree")
            .map_err(|_| std::io::Error::other("dot stuffing failed"))?;
        assert_eq!(wire, b"one\r\n..two\r\nthree\r\n.\r\n");
        Ok(())
    }
}