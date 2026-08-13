use super::source::{GmailMessageContext, PreparationFailure};
use application_ports::outbound_mail::{MailAddress, MailBody, MailSubject};

const ALTERNATIVE_BOUNDARY: &str = "=_profile_mail_c5_alternative";
const HEADER_SOFT_LIMIT: usize = 76;

pub(super) fn render_mime(
    context: &GmailMessageContext,
    body: &MailBody,
) -> Result<Vec<u8>, PreparationFailure> {
    let mut output = String::new();
    push_address_header(&mut output, "From", core::slice::from_ref(&context.from));
    push_address_header(&mut output, "To", context.recipients.to());
    push_address_header(&mut output, "Cc", context.recipients.cc());
    push_address_header(&mut output, "Bcc", context.recipients.bcc());
    if let Some(subject) = context.subject.as_ref() {
        push_subject_header(&mut output, subject)?;
    }
    if let Some(value) = context.in_reply_to.as_deref() {
        push_safe_header(&mut output, "In-Reply-To", value)?;
    }
    if let Some(value) = context.references.as_deref() {
        push_safe_header(&mut output, "References", value)?;
    }
    output.push_str("MIME-Version: 1.0\r\n");

    match (body.text(), body.html()) {
        (Some(text), Some(html)) => {
            output.push_str("Content-Type: multipart/alternative; boundary=\"");
            output.push_str(ALTERNATIVE_BOUNDARY);
            output.push_str("\"\r\n\r\n");
            push_alternative_part(&mut output, "text/plain", text);
            push_alternative_part(&mut output, "text/html", html);
            output.push_str("--");
            output.push_str(ALTERNATIVE_BOUNDARY);
            output.push_str("--\r\n");
        }
        (Some(text), None) => push_single_body(&mut output, "text/plain", text),
        (None, Some(html)) => push_single_body(&mut output, "text/html", html),
        (None, None) => return Err(PreparationFailure::Rejected),
    }
    Ok(output.into_bytes())
}

pub(super) fn encode_base64url_unpadded(bytes: &[u8]) -> String {
    encode_base64(bytes, Base64Alphabet::UrlSafe, false, false)
}

fn push_single_body(output: &mut String, content_type: &str, body: &str) {
    output.push_str("Content-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(
        body.as_bytes(),
        Base64Alphabet::Standard,
        true,
        true,
    ));
    output.push_str("\r\n");
}

fn push_alternative_part(output: &mut String, content_type: &str, body: &str) {
    output.push_str("--");
    output.push_str(ALTERNATIVE_BOUNDARY);
    output.push_str("\r\nContent-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(
        body.as_bytes(),
        Base64Alphabet::Standard,
        true,
        true,
    ));
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

fn push_subject_header(
    output: &mut String,
    subject: &MailSubject,
) -> Result<(), PreparationFailure> {
    let value = subject.as_str();
    if value.is_ascii() {
        return push_safe_header(output, "Subject", value);
    }
    output.push_str("Subject: ");
    let mut line_length = 9_usize;
    let mut chunk = String::new();
    for character in value.chars() {
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
    let encoded = encode_base64(chunk.as_bytes(), Base64Alphabet::Standard, true, false);
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

fn push_safe_header(
    output: &mut String,
    name: &str,
    value: &str,
) -> Result<(), PreparationFailure> {
    if value.chars().any(char::is_control) {
        return Err(PreparationFailure::Rejected);
    }
    output.push_str(name);
    output.push_str(": ");
    output.push_str(value);
    output.push_str("\r\n");
    Ok(())
}

#[derive(Clone, Copy)]
enum Base64Alphabet {
    Standard,
    UrlSafe,
}

fn encode_base64(
    bytes: &[u8],
    alphabet: Base64Alphabet,
    padding: bool,
    wrap_lines: bool,
) -> String {
    let symbols = match alphabet {
        Base64Alphabet::Standard => {
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/" as &[u8]
        }
        Base64Alphabet::UrlSafe => {
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" as &[u8]
        }
    };
    let encoded_length = bytes.len().saturating_add(2) / 3 * 4;
    let mut output = String::with_capacity(encoded_length.saturating_add(encoded_length / 76 * 2));
    let mut column = 0_usize;
    let mut index = 0_usize;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        push_symbol(
            &mut output,
            symbols[((block >> 18) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[((block >> 12) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[((block >> 6) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[(block & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        index += 3;
    }
    let remaining = bytes.len() - index;
    if remaining == 1 {
        let block = u32::from(bytes[index]) << 16;
        push_symbol(
            &mut output,
            symbols[((block >> 18) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[((block >> 12) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        if padding {
            push_symbol(&mut output, b'=', &mut column, wrap_lines);
            push_symbol(&mut output, b'=', &mut column, wrap_lines);
        }
    } else if remaining == 2 {
        let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
        push_symbol(
            &mut output,
            symbols[((block >> 18) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[((block >> 12) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        push_symbol(
            &mut output,
            symbols[((block >> 6) & 0x3f) as usize],
            &mut column,
            wrap_lines,
        );
        if padding {
            push_symbol(&mut output, b'=', &mut column, wrap_lines);
        }
    }
    output
}

fn push_symbol(output: &mut String, byte: u8, column: &mut usize, wrap_lines: bool) {
    if wrap_lines && *column == 76 {
        output.push_str("\r\n");
        *column = 0;
    }
    output.push(char::from(byte));
    *column += 1;
}

#[cfg(test)]
mod tests {
    use super::{encode_base64url_unpadded, render_mime};
    use crate::gmail_outbound_mail::source::GmailMessageContext;
    use application_ports::outbound_mail::{MailAddress, MailBody, MailRecipients, MailSubject};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn base64url_raw_encoding_is_unpadded() {
        assert_eq!(encode_base64url_unpadded(b"hello"), "aGVsbG8");
    }

    #[test]
    fn multipart_message_is_deterministic() -> TestResult {
        let context = GmailMessageContext {
            from: MailAddress::parse("sender@example.com")?,
            recipients: MailRecipients::new(
                vec![MailAddress::parse("client@example.com")?],
                Vec::new(),
                Vec::new(),
            )?,
            subject: Some(MailSubject::parse("Привет")?),
            thread_id: None,
            in_reply_to: None,
            references: None,
        };
        let body = MailBody::new(
            Some("plain body".to_owned()),
            Some("<b>html</b>".to_owned()),
        )?;
        let rendered = String::from_utf8(render_mime(&context, &body)?)?;
        assert!(rendered.contains("multipart/alternative"));
        assert!(rendered.contains("=?UTF-8?B?"));
        assert!(!rendered.contains("plain body"));
        Ok(())
    }
}
