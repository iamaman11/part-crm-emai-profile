use super::{SmtpReply, SmtpSendFailure, classify_auth_reply, send_command};
use crate::smtp_send_credential::{SmtpAuthenticationMode, SmtpCredential};
use worker::Socket;
use zeroize::Zeroize;

const MAX_AUTH_WIRE_BYTES: usize = 24 * 1024;
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) async fn authenticate(
    socket: &mut Socket,
    ehlo: &SmtpReply,
    credential: &SmtpCredential,
) -> Result<(), SmtpSendFailure> {
    match credential.authentication_mode() {
        SmtpAuthenticationMode::Password => {
            let password = credential
                .password()
                .ok_or(SmtpSendFailure::IntegrityFailure)?;
            if ehlo.auth_mechanism("PLAIN") {
                auth_plain(socket, credential.username(), password).await
            } else if ehlo.auth_mechanism("LOGIN") {
                auth_login(socket, credential.username(), password).await
            } else {
                Err(SmtpSendFailure::Rejected)
            }
        }
        SmtpAuthenticationMode::Xoauth2 => {
            let token = credential
                .access_token()
                .ok_or(SmtpSendFailure::IntegrityFailure)?;
            if !ehlo.auth_mechanism("XOAUTH2") {
                return Err(SmtpSendFailure::Rejected);
            }
            auth_xoauth2(socket, credential.username(), token).await
        }
    }
}

async fn auth_plain(
    socket: &mut Socket,
    username: &str,
    password: &str,
) -> Result<(), SmtpSendFailure> {
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
    let result = send_command(socket, &command, true).await;
    command.zeroize();
    encoded.zeroize();
    classify_auth_reply(result?.code)
}

async fn auth_login(
    socket: &mut Socket,
    username: &str,
    password: &str,
) -> Result<(), SmtpSendFailure> {
    let reply = send_command(socket, "AUTH LOGIN", false).await?;
    if reply.code != 334 {
        return classify_auth_reply(reply.code);
    }
    let mut encoded_username = base64_standard(username.as_bytes())?;
    let username_reply = send_command(socket, &encoded_username, true).await;
    encoded_username.zeroize();
    let username_reply = username_reply?;
    if username_reply.code != 334 {
        return classify_auth_reply(username_reply.code);
    }
    let mut encoded_password = base64_standard(password.as_bytes())?;
    let password_reply = send_command(socket, &encoded_password, true).await;
    encoded_password.zeroize();
    classify_auth_reply(password_reply?.code)
}

async fn auth_xoauth2(
    socket: &mut Socket,
    username: &str,
    token: &str,
) -> Result<(), SmtpSendFailure> {
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
    let result = send_command(socket, &command, true).await;
    command.zeroize();
    encoded.zeroize();
    let reply = result?;
    if reply.code == 334 {
        let final_reply = send_command(socket, "", true).await?;
        return classify_auth_reply(final_reply.code);
    }
    classify_auth_reply(reply.code)
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
    use super::base64_standard;

    #[test]
    fn standard_base64_is_padded() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            base64_standard(b"hello").map_err(|_| std::io::Error::other("base64 failed"))?,
            "aGVsbG8="
        );
        Ok(())
    }
}
