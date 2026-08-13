use crate::cloud_mailbox_secrets::{MailboxCredential, resolve_mailbox_credential};
use crate::imap_session::{ImapSession, ImapTaggedStatus, ImapTransportError};
use application_ports::mailboxes::MailboxProviderPortError;
use application_ports::outbound_mail::{
    MailAddress, MailRecipients, MailSubject, OutboundMailIntent, OutboundMailOperation,
};
use mailbox_domain::{MailboxBinding, MailboxFailureDisposition};
use worker::Env;

const MAX_RESPONSE: usize = 128 * 1024;
const MAX_HEADER: usize = 64 * 1024;
const MAX_VALUE: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreparationFailure {
    RetryableNotSent,
    Rejected,
}

pub(super) struct SourceContext {
    pub recipients: MailRecipients,
    pub fallback_subject: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

struct Headers {
    subject: Option<String>,
    from: Option<String>,
    reply_to: Option<String>,
    to: Option<String>,
    cc: Option<String>,
    message_id: Option<String>,
    references: Option<String>,
}

pub(super) async fn resolve_source_context(
    env: &Env,
    binding: &MailboxBinding,
    intent: &OutboundMailIntent,
    sender: &str,
) -> Result<Option<SourceContext>, PreparationFailure> {
    let Some(source) = intent.operation().source() else {
        return Ok(None);
    };
    if source.binding_id() != binding.binding_id() {
        return Err(PreparationFailure::Rejected);
    }
    let _ = parse_reference(source.provider_reference().as_str())?;

    if let OutboundMailOperation::Forward { recipients, .. } = intent.operation() {
        return Ok(Some(SourceContext {
            recipients: recipients.clone(),
            fallback_subject: None,
            in_reply_to: None,
            references: None,
        }));
    }

    let credential = match resolve_mailbox_credential(env, binding)
        .await
        .map_err(map_resolver)?
    {
        MailboxCredential::Imap(value) => value,
        MailboxCredential::GmailApi(_) | MailboxCredential::MicrosoftGraph(_) => {
            return Err(PreparationFailure::Rejected);
        }
    };
    let (expected_validity, uid) = parse_reference(source.provider_reference().as_str())?;
    let mut session = ImapSession::connect(&credential)
        .await
        .map_err(map_transport)?;
    let examine = session
        .execute("EXAMINE INBOX", MAX_RESPONSE)
        .await
        .map_err(map_transport)?;
    require_ok(&examine)?;
    if parse_bracket_u64(&examine.text_lossy(), "UIDVALIDITY") != Some(expected_validity) {
        return Err(PreparationFailure::Rejected);
    }
    let command = format!(
        "UID FETCH {uid} (UID BODY.PEEK[HEADER.FIELDS (SUBJECT FROM REPLY-TO TO CC MESSAGE-ID REFERENCES)])"
    );
    let response = session
        .execute(&command, MAX_RESPONSE)
        .await
        .map_err(map_transport)?;
    require_ok(&response)?;
    if !response
        .bytes()
        .windows(7)
        .any(|window| window == b" FETCH ")
    {
        return Err(PreparationFailure::Rejected);
    }
    let literal = first_literal(response.bytes())?.ok_or(PreparationFailure::Rejected)?;
    let headers = parse_headers(literal)?;

    if let Some(requested) = intent.subject() {
        if headers.subject.as_deref() != Some(requested.as_str()) {
            return Err(PreparationFailure::Rejected);
        }
    }
    let fallback_subject = headers
        .subject
        .as_deref()
        .map(|value| {
            MailSubject::parse(value.to_owned())
                .map(|subject| subject.as_str().to_owned())
                .map_err(|_| PreparationFailure::Rejected)
        })
        .transpose()?;
    let message_id = headers
        .message_id
        .clone()
        .filter(|value| !value.is_empty())
        .ok_or(PreparationFailure::Rejected)?;
    valid_value(&message_id)?;
    let references = reference_chain(headers.references.as_deref(), &message_id)?;
    let recipients = match intent.operation() {
        OutboundMailOperation::Reply { .. } => reply_recipients(&headers)?,
        OutboundMailOperation::ReplyAll { .. } => reply_all_recipients(&headers, sender)?,
        OutboundMailOperation::New { .. } | OutboundMailOperation::Forward { .. } => {
            return Err(PreparationFailure::Rejected);
        }
    };
    Ok(Some(SourceContext {
        recipients,
        fallback_subject,
        in_reply_to: Some(message_id),
        references: Some(references),
    }))
}

fn reply_recipients(headers: &Headers) -> Result<MailRecipients, PreparationFailure> {
    let value = headers
        .reply_to
        .as_ref()
        .or(headers.from.as_ref())
        .ok_or(PreparationFailure::Rejected)?;
    MailRecipients::new(parse_addresses(value)?, Vec::new(), Vec::new())
        .map_err(|_| PreparationFailure::Rejected)
}

fn reply_all_recipients(
    headers: &Headers,
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

fn parse_addresses(value: &str) -> Result<Vec<MailAddress>, PreparationFailure> {
    valid_value(value)?;
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

fn parse_headers(bytes: &[u8]) -> Result<Headers, PreparationFailure> {
    if bytes.len() > MAX_HEADER {
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
    Ok(Headers {
        subject: value(&headers, "subject")?,
        from: value(&headers, "from")?,
        reply_to: value(&headers, "reply-to")?,
        to: value(&headers, "to")?,
        cc: value(&headers, "cc")?,
        message_id: value(&headers, "message-id")?,
        references: value(&headers, "references")?,
    })
}

fn value(headers: &[(String, String)], name: &str) -> Result<Option<String>, PreparationFailure> {
    let result = headers
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.trim().to_owned());
    if let Some(value) = result.as_deref() {
        valid_value(value)?;
    }
    Ok(result.filter(|value| !value.is_empty()))
}

fn valid_value(value: &str) -> Result<(), PreparationFailure> {
    if value.len() > MAX_VALUE
        || value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        Err(PreparationFailure::Rejected)
    } else {
        Ok(())
    }
}

fn reference_chain(
    references: Option<&str>,
    message_id: &str,
) -> Result<String, PreparationFailure> {
    valid_value(message_id)?;
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
    valid_value(&combined)?;
    Ok(combined)
}

fn parse_reference(reference: &str) -> Result<(u64, u64), PreparationFailure> {
    let value = reference
        .strip_prefix("imap:")
        .ok_or(PreparationFailure::Rejected)?;
    let (validity, uid) = value.split_once(':').ok_or(PreparationFailure::Rejected)?;
    let validity = validity
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PreparationFailure::Rejected)?;
    let uid = uid
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PreparationFailure::Rejected)?;
    Ok((validity, uid))
}

fn parse_bracket_u64(response: &str, name: &str) -> Option<u64> {
    let marker = format!("[{name} ");
    let start = response.find(&marker)? + marker.len();
    let tail = response.get(start..)?;
    let end = tail.find(']')?;
    tail[..end].trim().parse().ok()
}

fn first_literal(response: &[u8]) -> Result<Option<&[u8]>, PreparationFailure> {
    let Some(open) = response.iter().position(|byte| *byte == b'{') else {
        return Ok(None);
    };
    let close = response
        .get(open + 1..)
        .and_then(|tail| tail.iter().position(|byte| *byte == b'}'))
        .map(|offset| open + 1 + offset)
        .ok_or(PreparationFailure::Rejected)?;
    let length = core::str::from_utf8(&response[open + 1..close])
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(PreparationFailure::Rejected)?;
    let mut start = close + 1;
    if response.get(start..start + 2) == Some(b"\r\n") {
        start += 2;
    } else if response.get(start) == Some(&b'\n') {
        start += 1;
    } else {
        return Err(PreparationFailure::Rejected);
    }
    let end = start
        .checked_add(length)
        .ok_or(PreparationFailure::Rejected)?;
    Ok(Some(
        response
            .get(start..end)
            .ok_or(PreparationFailure::Rejected)?,
    ))
}

fn require_ok(
    response: &crate::imap_session::ImapCommandResponse,
) -> Result<(), PreparationFailure> {
    match response.status() {
        ImapTaggedStatus::Ok => Ok(()),
        ImapTaggedStatus::No | ImapTaggedStatus::Bad => Err(PreparationFailure::Rejected),
    }
}

fn map_transport(error: ImapTransportError) -> PreparationFailure {
    match error {
        ImapTransportError::DependencyUnavailable => PreparationFailure::RetryableNotSent,
        ImapTransportError::Authentication
        | ImapTransportError::ProviderPolicy
        | ImapTransportError::IntegrityFailure => PreparationFailure::Rejected,
    }
}

fn map_resolver(error: MailboxProviderPortError) -> PreparationFailure {
    match error {
        MailboxProviderPortError::IntegrityFailure => PreparationFailure::Rejected,
        MailboxProviderPortError::Failure(failure) => match failure.disposition() {
            MailboxFailureDisposition::Retryable => PreparationFailure::RetryableNotSent,
            MailboxFailureDisposition::AuthRequired
            | MailboxFailureDisposition::Suspended
            | MailboxFailureDisposition::Terminal => PreparationFailure::Rejected,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_addresses, reference_chain};

    #[test]
    fn source_translation_is_bounded_and_stable() -> Result<(), Box<dyn std::error::Error>> {
        let values =
            parse_addresses("\"Doe, Jane\" <jane@example.com>, bob@example.com, jane@example.com")
                .map_err(|_| std::io::Error::other("parse"))?;
        assert_eq!(values.len(), 2);
        let references = reference_chain(Some("<root@example.com>"), "<source@example.com>")
            .map_err(|_| std::io::Error::other("refs"))?;
        assert_eq!(references, "<root@example.com> <source@example.com>");
        Ok(())
    }
}
