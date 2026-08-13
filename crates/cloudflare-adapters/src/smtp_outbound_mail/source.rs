mod rfc;
#[cfg(test)]
mod tests;

use crate::cloud_mailbox_secrets::{MailboxCredential, resolve_mailbox_credential};
use crate::imap_session::{ImapSession, ImapTaggedStatus, ImapTransportError};
use application_ports::mailboxes::MailboxProviderPortError;
use application_ports::outbound_mail::{
    MailRecipients, MailSubject, OutboundMailIntent, OutboundMailOperation,
};
use mailbox_domain::{MailboxBinding, MailboxFailureDisposition};
use rfc::{parse_source_headers, reference_chain, reply_all_recipients, reply_recipients};
use worker::Env;

const MAX_RESPONSE_BYTES: usize = 128 * 1024;

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
    let (expected_uid_validity, uid) = parse_reference(source.provider_reference().as_str())?;

    if let OutboundMailOperation::Forward { recipients, .. } = intent.operation() {
        return Ok(Some(forward_context(recipients)));
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
    let mut session = ImapSession::connect(&credential)
        .await
        .map_err(map_transport)?;
    let examine = session
        .execute("EXAMINE INBOX", MAX_RESPONSE_BYTES)
        .await
        .map_err(map_transport)?;
    require_ok(&examine)?;
    if parse_bracket_u64(&examine.text_lossy(), "UIDVALIDITY") != Some(expected_uid_validity) {
        return Err(PreparationFailure::Rejected);
    }

    let command = format!(
        "UID FETCH {uid} (UID BODY.PEEK[HEADER.FIELDS (SUBJECT FROM REPLY-TO TO CC MESSAGE-ID REFERENCES)])"
    );
    let response = session
        .execute(&command, MAX_RESPONSE_BYTES)
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
    let headers = parse_source_headers(literal)?;

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

fn forward_context(recipients: &MailRecipients) -> SourceContext {
    SourceContext {
        recipients: recipients.clone(),
        fallback_subject: None,
        in_reply_to: None,
        references: None,
    }
}

fn parse_reference(reference: &str) -> Result<(u64, u64), PreparationFailure> {
    let value = reference
        .strip_prefix("imap:")
        .ok_or(PreparationFailure::Rejected)?;
    let (uid_validity, uid) = value
        .split_once(':')
        .ok_or(PreparationFailure::Rejected)?;
    let uid_validity = uid_validity
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PreparationFailure::Rejected)?;
    let uid = uid
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PreparationFailure::Rejected)?;
    Ok((uid_validity, uid))
}

fn parse_bracket_u64(response: &str, name: &str) -> Option<u64> {
    let marker = format!("[{name} ");
    let start = response.find(&marker)?.checked_add(marker.len())?;
    let tail = response.get(start..)?;
    let end = tail.find(']')?;
    tail.get(..end)?.trim().parse().ok()
}

fn first_literal(response: &[u8]) -> Result<Option<&[u8]>, PreparationFailure> {
    let Some(open) = response.iter().position(|byte| *byte == b'{') else {
        return Ok(None);
    };
    let after_open = open
        .checked_add(1)
        .ok_or(PreparationFailure::Rejected)?;
    let relative_close = response
        .get(after_open..)
        .and_then(|tail| tail.iter().position(|byte| *byte == b'}'))
        .ok_or(PreparationFailure::Rejected)?;
    let close = after_open
        .checked_add(relative_close)
        .ok_or(PreparationFailure::Rejected)?;
    let length = core::str::from_utf8(
        response
            .get(after_open..close)
            .ok_or(PreparationFailure::Rejected)?,
    )
    .ok()
    .and_then(|value| value.parse::<usize>().ok())
    .ok_or(PreparationFailure::Rejected)?;
    let mut data_start = close
        .checked_add(1)
        .ok_or(PreparationFailure::Rejected)?;
    let tail = response
        .get(data_start..)
        .ok_or(PreparationFailure::Rejected)?;
    if tail.starts_with(b"\r\n") {
        data_start = data_start
            .checked_add(2)
            .ok_or(PreparationFailure::Rejected)?;
    } else if tail.starts_with(b"\n") {
        data_start = data_start
            .checked_add(1)
            .ok_or(PreparationFailure::Rejected)?;
    } else {
        return Err(PreparationFailure::Rejected);
    }
    let data_end = data_start
        .checked_add(length)
        .ok_or(PreparationFailure::Rejected)?;
    Ok(Some(
        response
            .get(data_start..data_end)
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
