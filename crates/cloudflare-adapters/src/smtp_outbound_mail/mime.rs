use application_ports::outbound_mail::{MailAddress, MailBody, MailRecipients, MailSubject};
use sha2::{Digest, Sha256};

const MAX_RENDERED_MESSAGE_BYTES: usize = 2 * 1024 * 1024;
const HEADER_SOFT_LIMIT: usize = 76;
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) struct RenderContext<'a> {
    pub sender: &'a str,
    pub recipients: &'a MailRecipients,
    pub subject: Option<&'a MailSubject>,
    pub fallback_subject: Option<&'a str>,
    pub in_reply_to: Option<&'a str>,
    pub references: Option<&'a str>,
}

pub(super) struct RenderedMessage {
    pub bytes: Vec<u8>,
    pub envelope_recipients: Vec<String>,
}

pub(super) fn render_mime(
    context: &RenderContext<'_>,
    body: &MailBody,
) -> Result<RenderedMessage, ()> {
    validate_address(context.sender)?;
    let envelope_recipients = envelope_recipients(context.recipients)?;
    for address in context
        .recipients
        .to()
        .iter()
        .chain(context.recipients.cc())
    {
        validate_address(address.as_str())?;
    }
    let subject = context
        .subject
        .map(MailSubject::as_str)
        .or(context.fallback_subject)
        .unwrap_or("");
    validate_header_value(subject)?;
    if let Some(value) = context.in_reply_to {
        validate_reference_header(value)?;
    }
    if let Some(value) = context.references {
        validate_reference_header(value)?;
    }

    let mut output = String::new();
    push_header(&mut output, "From", context.sender);
    push_address_header(&mut output, "To", context.recipients.to());
    push_address_header(&mut output, "Cc", context.recipients.cc());
    push_subject_header(&mut output, subject)?;
    if let Some(value) = context.in_reply_to {
        push_header(&mut output, "In-Reply-To", value);
    }
    if let Some(value) = context.references {
        push_header(&mut output, "References", value);
    }
    output.push_str("MIME-Version: 1.0\r\n");

    match (body.text(), body.html()) {
        (Some(text), Some(html)) if !text.is_empty() && !html.is_empty() => {
            let boundary = multipart_boundary(context, body);
            output.push_str("Content-Type: multipart/alternative; boundary=\"");
            output.push_str(&boundary);
            output.push_str("\"\r\n\r\n");
            push_alternative_part(&mut output, &boundary, "text/plain", text);
            push_alternative_part(&mut output, &boundary, "text/html", html);
            output.push_str("--");
            output.push_str(&boundary);
            output.push_str("--\r\n");
        }
        (Some(text), _) if !text.is_empty() => {
            push_single_body(&mut output, "text/plain", text);
        }
        (_, Some(html)) if !html.is_empty() => {
            push_single_body(&mut output, "text/html", html);
        }
        _ => return Err(()),
    }
    if output.len() > MAX_RENDERED_MESSAGE_BYTES {
        return Err(());
    }
    Ok(RenderedMessage {
        bytes: output.into_bytes(),
        envelope_recipients,
    })
}

fn envelope_recipients(recipients: &MailRecipients) -> Result<Vec<String>, ()> {
    let mut output = Vec::new();
    for address in recipients
        .to()
        .iter()
        .chain(recipients.cc())
        .chain(recipients.bcc())
    {
        validate_address(address.as_str())?;
        if !output
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(address.as_str()))
        {
            output.push(address.as_str().to_owned());
        }
    }
    if output.is_empty() {
        return Err(());
    }
    Ok(output)
}

fn validate_address(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || !value.contains('@')
    {
        return Err(());
    }
    Ok(())
}

fn validate_header_value(value: &str) -> Result<(), ()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_reference_header(value: &str) -> Result<(), ()> {
    if value.len() > 8 * 1024 {
        return Err(());
    }
    validate_header_value(value)
}

fn push_header(output: &mut String, name: &str, value: &str) {
    output.push_str(name);
    output.push_str(": ");
    output.push_str(value);
    output.push_str("\r\n");
}

fn push_address_header(output: &mut String, name: &str, addresses: &[MailAddress]) {
    if addresses.is_empty() {
        return;
    }
    output.push_str(name);
    output.push_str(": ");
    let mut line_length = name.len() + 2;
    for (index, address) in addresses.iter().enumerate() {
        let separator = usize::from(index > 0) * 2;
        let projected = line_length
            .saturating_add(separator)
            .saturating_add(address.as_str().len());
        if index > 0 && projected > HEADER_SOFT_LIMIT {
            output.push_str(",\r\n ");
            line_length = 1;
        } else if index > 0 {
            output.push_str(", ");
            line_length += 2;
        }
        output.push_str(address.as_str());
        line_length = line_length.saturating_add(address.as_str().len());
    }
    output.push_str("\r\n");
}

fn push_subject_header(output: &mut String, subject: &str) -> Result<(), ()> {
    validate_header_value(subject)?;
    if subject.is_ascii() {
        push_header(output, "Subject", subject);
        return Ok(());
    }
    output.push_str("Subject: ");
    let mut line_length = 9_usize;
    let mut chunk = String::new();
    for character in subject.chars() {
        let char_len = character.len_utf8();
        if !chunk.is_empty() && chunk.len().saturating_add(char_len) > 30 {
            push_encoded_word(output, &chunk, &mut line_length);
            chunk.clear();
        }
        chunk.push(character);
    }
    if !chunk.is_empty() {
        push_encoded_word(output, &chunk, &mut line_length);
    }
    output.push_str("\r\n");
    Ok(())
}

fn push_encoded_word(output: &mut String, chunk: &str, line_length: &mut usize) {
    let encoded = encode_base64(chunk.as_bytes(), false);
    let mut word = String::with_capacity(encoded.len() + 12);
    word.push_str("=?UTF-8?B?");
    word.push_str(&encoded);
    word.push_str("?=");
    if *line_length > 9 && (*line_length).saturating_add(1 + word.len()) > HEADER_SOFT_LIMIT {
        output.push_str("\r\n ");
        *line_length = 1;
    } else if *line_length > 9 {
        output.push(' ');
        *line_length += 1;
    }
    output.push_str(&word);
    *line_length = (*line_length).saturating_add(word.len());
}

fn push_single_body(output: &mut String, content_type: &str, body: &str) {
    output.push_str("Content-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(body.as_bytes(), true));
    output.push_str("\r\n");
}

fn push_alternative_part(output: &mut String, boundary: &str, content_type: &str, body: &str) {
    output.push_str("--");
    output.push_str(boundary);
    output.push_str("\r\nContent-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(body.as_bytes(), true));
    output.push_str("\r\n");
}

fn encode_base64(bytes: &[u8], wrap_lines: bool) -> String {
    let encoded_length = bytes.len().saturating_add(2) / 3 * 4;
    let mut output = String::with_capacity(encoded_length.saturating_add(encoded_length / 76 * 2));
    let mut column = 0_usize;
    let mut index = 0_usize;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        for shift in [18_u32, 12, 6, 0] {
            push_base64_symbol(
                &mut output,
                BASE64_ALPHABET[((block >> shift) & 0x3f) as usize],
                &mut column,
                wrap_lines,
            );
        }
        index += 3;
    }
    let remaining = bytes.len() - index;
    if remaining == 1 {
        let block = u32::from(bytes[index]) << 16;
        push_base64_symbol(
            &mut output,
            BASE64_ALPHABET[((block >> 18) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_base64_symbol(
            &mut output,
            BASE64_ALPHABET[((block >> 12) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_base64_symbol(&mut output, b'=', &mut column, wrap_lines);
        push_base64_symbol(&mut output, b'=', &mut column, wrap_lines);
    } else if remaining == 2 {
        let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
        push_base64_symbol(
            &mut output,
            BASE64_ALPHABET[((block >> 18) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_base64_symbol(
            &mut output,
            BASE64_ALPHABET[((block >> 12) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_base64_symbol(
            &mut output,
            BASE64_ALPHABET[((block >> 6) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_base64_symbol(&mut output, b'=', &mut column, wrap_lines);
    }
    output
}

fn push_base64_symbol(output: &mut String, byte: u8, column: &mut usize, wrap_lines: bool) {
    if wrap_lines && *column == 76 {
        output.push_str("\r\n");
        *column = 0;
    }
    output.push(char::from(byte));
    *column += 1;
}

fn multipart_boundary(context: &RenderContext<'_>, body: &MailBody) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context.sender.as_bytes());
    for address in context
        .recipients
        .to()
        .iter()
        .chain(context.recipients.cc())
        .chain(context.recipients.bcc())
    {
        hasher.update([0]);
        hasher.update(address.as_str().as_bytes());
    }
    hasher.update([0]);
    hasher.update(
        context
            .subject
            .map(MailSubject::as_str)
            .or(context.fallback_subject)
            .unwrap_or("")
            .as_bytes(),
    );
    if let Some(text) = body.text() {
        hasher.update([0]);
        hasher.update(text.as_bytes());
    }
    if let Some(html) = body.html() {
        hasher.update([0]);
        hasher.update(html.as_bytes());
    }
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        use core::fmt::Write as _;
        let _ = write!(&mut suffix, "{byte:02x}");
    }
    format!("profile-alt-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{RenderContext, render_mime};
    use application_ports::outbound_mail::{MailAddress, MailBody, MailRecipients, MailSubject};

    fn recipients() -> Result<MailRecipients, Box<dyn std::error::Error>> {
        Ok(MailRecipients::new(
            vec![MailAddress::parse("to@example.com")?],
            vec![MailAddress::parse("cc@example.com")?],
            vec![MailAddress::parse("bcc@example.com")?],
        )?)
    }

    fn render(body: MailBody, subject: &MailSubject) -> Result<super::RenderedMessage, std::io::Error> {
        let recipients = recipients().map_err(|_| std::io::Error::other("recipients"))?;
        let context = RenderContext {
            sender: "sender@example.com",
            recipients: &recipients,
            subject: Some(subject),
            fallback_subject: None,
            in_reply_to: Some("<source@example.com>"),
            references: Some("<root@example.com> <source@example.com>"),
        };
        render_mime(&context, &body).map_err(|()| std::io::Error::other("render failed"))
    }

    #[test]
    fn multipart_render_is_deterministic_encoded_and_bcc_is_envelope_only()
    -> Result<(), Box<dyn std::error::Error>> {
        let subject = MailSubject::parse("Привет")?;
        let body = MailBody::new(
            Some("plain body".to_owned()),
            Some("<b>html body</b>".to_owned()),
        )?;
        let first = render(body.clone(), &subject)?;
        let second = render(body, &subject)?;
        assert_eq!(first.bytes, second.bytes);
        let text = String::from_utf8(first.bytes)?;
        assert!(text.contains("multipart/alternative"));
        assert!(text.contains("Content-Transfer-Encoding: base64"));
        assert!(text.contains("=?UTF-8?B?"));
        assert!(!text.contains("plain body"));
        assert!(!text.contains("Bcc:"));
        assert_eq!(first.envelope_recipients.len(), 3);
        Ok(())
    }

    #[test]
    fn text_only_and_html_only_are_base64_encoded() -> Result<(), Box<dyn std::error::Error>> {
        let subject = MailSubject::parse("Subject")?;
        let text = render(MailBody::new(Some("plain body".to_owned()), None)?, &subject)?;
        let html = render(
            MailBody::new(None, Some("<b>html body</b>".to_owned()))?,
            &subject,
        )?;
        let text = String::from_utf8(text.bytes)?;
        let html = String::from_utf8(html.bytes)?;
        assert!(text.contains("Content-Type: text/plain"));
        assert!(html.contains("Content-Type: text/html"));
        assert!(!text.contains("plain body"));
        assert!(!html.contains("html body"));
        Ok(())
    }

    #[test]
    fn header_injection_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let recipients = recipients()?;
        let body = MailBody::new(Some("body".to_owned()), None)?;
        let context = RenderContext {
            sender: "sender@example.com",
            recipients: &recipients,
            subject: None,
            fallback_subject: Some("ok\r\nBcc: attacker@example.com"),
            in_reply_to: None,
            references: None,
        };
        assert!(render_mime(&context, &body).is_err());
        Ok(())
    }
}
