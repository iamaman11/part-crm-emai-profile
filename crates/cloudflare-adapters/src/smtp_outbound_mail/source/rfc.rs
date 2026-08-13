#[cfg(test)]
mod tests;

use super::PreparationFailure;
use application_ports::outbound_mail::{MailAddress, MailRecipients};

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;

pub(super) struct SourceHeaders {
    pub(super) subject: Option<String>,
    pub(super) from: Option<String>,
    pub(super) reply_to: Option<String>,
    pub(super) to: Option<String>,
    pub(super) cc: Option<String>,
    pub(super) message_id: Option<String>,
    pub(super) references: Option<String>,
}

pub(super) fn parse_source_headers(bytes: &[u8]) -> Result<SourceHeaders, PreparationFailure> {
    if bytes.len() > MAX_HEADER_BYTES {
        return Err(PreparationFailure::Rejected);
    }
    let normalized = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in normalized.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            let Some((_, value)) = headers.last_mut() else {
                return Err(PreparationFailure::Rejected);
            };
            value.push(' ');
            value.push_str(line.trim());
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(PreparationFailure::Rejected);
        };
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| (33..=126).contains(&byte) && byte != b':')
        {
            return Err(PreparationFailure::Rejected);
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
        if headers.len() > 256 {
            return Err(PreparationFailure::Rejected);
        }
    }
    Ok(SourceHeaders {
        subject: header_value(&headers, "subject")?,
        from: header_value(&headers, "from")?,
        reply_to: header_value(&headers, "reply-to")?,
        to: header_value(&headers, "to")?,
        cc: header_value(&headers, "cc")?,
        message_id: header_value(&headers, "message-id")?,
        references: header_value(&headers, "references")?,
    })
}

pub(super) fn reply_recipients(
    headers: &SourceHeaders,
) -> Result<MailRecipients, PreparationFailure> {
    let value = headers
        .reply_to
        .as_ref()
        .or(headers.from.as_ref())
        .ok_or(PreparationFailure::Rejected)?;
    MailRecipients::new(parse_addresses(value)?, Vec::new(), Vec::new())
        .map_err(|_| PreparationFailure::Rejected)
}

pub(super) fn reply_all_recipients(
    headers: &SourceHeaders,
    sender: &str,
) -> Result<MailRecipients, PreparationFailure> {
    let primary = headers
        .reply_to
        .as_ref()
        .or(headers.from.as_ref())
        .ok_or(PreparationFailure::Rejected)?;
    let mut to = parse_addresses(primary)?;
    let mut cc = Vec::new();
    if let Some(value) = headers.to.as_deref() {
        for address in parse_addresses(value)? {
            push_unique(&mut to, address, sender);
        }
    }
    if let Some(value) = headers.cc.as_deref() {
        for address in parse_addresses(value)? {
            if !contains(&to, address.as_str()) {
                push_unique(&mut cc, address, sender);
            }
        }
    }
    to.retain(|address| !address.as_str().eq_ignore_ascii_case(sender));
    if to.is_empty() && !cc.is_empty() {
        to.push(cc.remove(0));
    }
    MailRecipients::new(to, cc, Vec::new()).map_err(|_| PreparationFailure::Rejected)
}

pub(super) fn reference_chain(
    references: Option<&str>,
    message_id: &str,
) -> Result<String, PreparationFailure> {
    validate_header_value(message_id)?;
    let combined = match references.filter(|value| !value.trim().is_empty()) {
        Some(value)
            if value
                .split_ascii_whitespace()
                .any(|token| token == message_id) =>
        {
            value.to_owned()
        }
        Some(value) => format!("{} {}", value.trim(), message_id),
        None => message_id.to_owned(),
    };
    validate_header_value(&combined)?;
    Ok(combined)
}

fn parse_addresses(value: &str) -> Result<Vec<MailAddress>, PreparationFailure> {
    validate_header_value(value)?;
    let tokens = split_address_tokens(value)?;
    let mut output = Vec::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let address = match (token.rfind('<'), token.rfind('>')) {
            (Some(open), Some(close)) if close > open => token[open + 1..close].trim(),
            (None, None) => token,
            _ => return Err(PreparationFailure::Rejected),
        };
        let address =
            MailAddress::parse(address.to_owned()).map_err(|_| PreparationFailure::Rejected)?;
        if !contains(&output, address.as_str()) {
            output.push(address);
        }
    }
    if output.is_empty() {
        Err(PreparationFailure::Rejected)
    } else {
        Ok(output)
    }
}

fn split_address_tokens(value: &str) -> Result<Vec<&str>, PreparationFailure> {
    let mut output = Vec::new();
    let mut start = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut angle_depth = 0_u8;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '<' if !quoted => {
                angle_depth = angle_depth
                    .checked_add(1)
                    .ok_or(PreparationFailure::Rejected)?;
            }
            '>' if !quoted => {
                angle_depth = angle_depth
                    .checked_sub(1)
                    .ok_or(PreparationFailure::Rejected)?;
            }
            ',' if !quoted && angle_depth == 0 => {
                output.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted || angle_depth != 0 {
        return Err(PreparationFailure::Rejected);
    }
    output.push(&value[start..]);
    Ok(output)
}

fn push_unique(values: &mut Vec<MailAddress>, address: MailAddress, sender: &str) {
    if !address.as_str().eq_ignore_ascii_case(sender) && !contains(values, address.as_str()) {
        values.push(address);
    }
}

fn contains(values: &[MailAddress], candidate: &str) -> bool {
    values
        .iter()
        .any(|value| value.as_str().eq_ignore_ascii_case(candidate))
}

fn header_value(
    headers: &[(String, String)],
    name: &str,
) -> Result<Option<String>, PreparationFailure> {
    let result = headers
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.trim().to_owned());
    if let Some(value) = result.as_deref() {
        validate_header_value(value)?;
    }
    Ok(result.filter(|value| !value.is_empty()))
}

fn validate_header_value(value: &str) -> Result<(), PreparationFailure> {
    if value.len() > MAX_HEADER_VALUE_BYTES
        || value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        Err(PreparationFailure::Rejected)
    } else {
        Ok(())
    }
}
