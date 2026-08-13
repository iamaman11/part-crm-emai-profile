use application_ports::outbound_mail::MailAddress;

const HEADER_SOFT_LIMIT: usize = 76;
const HEADER_HARD_LIMIT: usize = 998;
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) fn validate_header_value(value: &str) -> Result<(), ()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        Err(())
    } else {
        Ok(())
    }
}

pub(super) fn push_header(output: &mut String, name: &str, value: &str) {
    output.push_str(name);
    output.push_str(": ");
    output.push_str(value);
    output.push_str("\r\n");
}

pub(super) fn push_address_header(output: &mut String, name: &str, addresses: &[MailAddress]) {
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

pub(super) fn push_subject_header(output: &mut String, subject: &str) -> Result<(), ()> {
    validate_header_value(subject)?;
    if subject.is_ascii() && subject.len() + "Subject: ".len() <= HEADER_SOFT_LIMIT {
        push_header(output, "Subject", subject);
        return Ok(());
    }
    output.push_str("Subject: ");
    let mut line_length = "Subject: ".len();
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

pub(super) fn push_reference_header(
    output: &mut String,
    name: &str,
    value: &str,
) -> Result<(), ()> {
    validate_header_value(value)?;
    let mut tokens = value.split_ascii_whitespace().peekable();
    if tokens.peek().is_none() {
        return Err(());
    }
    output.push_str(name);
    output.push_str(": ");
    let mut line_length = name.len() + 2;
    for token in tokens {
        if token.len() + 1 > HEADER_HARD_LIMIT {
            return Err(());
        }
        let separator = usize::from(line_length > name.len() + 2);
        let projected = line_length
            .saturating_add(separator)
            .saturating_add(token.len());
        if line_length > name.len() + 2 && projected > HEADER_SOFT_LIMIT {
            output.push_str("\r\n ");
            line_length = 1;
        } else if separator == 1 {
            output.push(' ');
            line_length += 1;
        }
        output.push_str(token);
        line_length = line_length.saturating_add(token.len());
    }
    output.push_str("\r\n");
    Ok(())
}

pub(super) fn push_single_body(output: &mut String, content_type: &str, body: &str) {
    output.push_str("Content-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(body.as_bytes(), true));
    output.push_str("\r\n");
}

pub(super) fn push_alternative_part(
    output: &mut String,
    boundary: &str,
    content_type: &str,
    body: &str,
) {
    output.push_str("--");
    output.push_str(boundary);
    output.push_str("\r\nContent-Type: ");
    output.push_str(content_type);
    output.push_str("; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    output.push_str(&encode_base64(body.as_bytes(), true));
    output.push_str("\r\n");
}

fn push_encoded_word(output: &mut String, chunk: &str, line_length: &mut usize) {
    let encoded = encode_base64(chunk.as_bytes(), false);
    let mut word = String::with_capacity(encoded.len() + 12);
    word.push_str("=?UTF-8?B?");
    word.push_str(&encoded);
    word.push_str("?=");
    if *line_length > "Subject: ".len()
        && (*line_length).saturating_add(1 + word.len()) > HEADER_SOFT_LIMIT
    {
        output.push_str("\r\n ");
        *line_length = 1;
    } else if *line_length > "Subject: ".len() {
        output.push(' ');
        *line_length += 1;
    }
    output.push_str(&word);
    *line_length = (*line_length).saturating_add(word.len());
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
