mod mime;
mod source;

use crate::d1_mailboxes::D1MailboxRepository;
use crate::gmail_send_credential::{
    GmailSendCredential, GmailSendCredentialError, resolve_gmail_send_credential,
};
use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError, ProviderMessageReference,
};
use mailbox_domain::MailboxProvider;
use mime::{encode_base64url_unpadded, render_mime};
use profile_platform_primitives::ActorContext;
use serde::{Deserialize, Serialize};
use source::{PreparationFailure, resolve_message_context};
use worker::d1::D1Database;
use worker::{Env, Fetch, Headers, Method, Request, RequestInit};
use zeroize::Zeroize;

const GMAIL_SEND_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";
const MAX_SEND_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_SEND_REQUEST_BYTES: usize = 2 * 1024 * 1024;

pub struct CloudflareGmailOutboundMailProvider<'a> {
    env: &'a Env,
    repository: D1MailboxRepository,
}

impl<'a> CloudflareGmailOutboundMailProvider<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, database: D1Database) -> Self {
        Self {
            env,
            repository: D1MailboxRepository::new(database),
        }
    }
}

impl OutboundMailProviderPort for CloudflareGmailOutboundMailProvider<'_> {
    async fn send(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
    ) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
        let binding = match self
            .repository
            .find_binding(actor.tenant_scope(), intent.binding_id())
            .await
        {
            Ok(Some(binding)) => binding,
            Ok(None) => return Ok(OutboundMailProviderOutcome::Rejected),
            Err(_) => return Ok(OutboundMailProviderOutcome::RetryableNotSent),
        };
        if binding.binding_id() != intent.binding_id()
            || binding.provider() != MailboxProvider::GmailApi
            || !binding.is_executable()
        {
            return Ok(OutboundMailProviderOutcome::Rejected);
        }

        let credential = match resolve_gmail_send_credential(self.env, &binding).await {
            Ok(credential) => credential,
            Err(error) => return Ok(credential_failure_outcome(error)),
        };
        let context = match resolve_message_context(intent, &credential).await {
            Ok(context) => context,
            Err(error) => return Ok(preparation_failure_outcome(error)),
        };
        let mime = match render_mime(&context, intent.body()) {
            Ok(mime) => mime,
            Err(error) => return Ok(preparation_failure_outcome(error)),
        };
        let raw = encode_base64url_unpadded(&mime);
        if raw.len() > MAX_SEND_REQUEST_BYTES {
            return Ok(OutboundMailProviderOutcome::Rejected);
        }
        send_gmail_message(&credential, raw, context.thread_id.as_deref()).await
    }
}

const fn credential_failure_outcome(
    error: GmailSendCredentialError,
) -> OutboundMailProviderOutcome {
    match error {
        GmailSendCredentialError::RetryableNotSent => {
            OutboundMailProviderOutcome::RetryableNotSent
        }
        GmailSendCredentialError::ReauthRequired
        | GmailSendCredentialError::Rejected
        | GmailSendCredentialError::IntegrityFailure => OutboundMailProviderOutcome::Rejected,
    }
}

const fn preparation_failure_outcome(error: PreparationFailure) -> OutboundMailProviderOutcome {
    match error {
        PreparationFailure::RetryableNotSent => OutboundMailProviderOutcome::RetryableNotSent,
        PreparationFailure::ReauthRequired | PreparationFailure::Rejected => {
            OutboundMailProviderOutcome::Rejected
        }
    }
}

async fn send_gmail_message(
    credential: &GmailSendCredential,
    raw: String,
    thread_id: Option<&str>,
) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
    let payload = GmailSendRequest { raw, thread_id };
    let body = match serde_json::to_string(&payload) {
        Ok(body) if body.len() <= MAX_SEND_REQUEST_BYTES => body,
        _ => return Ok(OutboundMailProviderOutcome::Rejected),
    };
    let headers = match send_headers(credential) {
        Ok(headers) => headers,
        Err(()) => return Ok(OutboundMailProviderOutcome::Rejected),
    };
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.into()));
    let request = match Request::new_with_init(GMAIL_SEND_ENDPOINT, &init) {
        Ok(request) => request,
        Err(_) => return Ok(OutboundMailProviderOutcome::Rejected),
    };
    let mut response = match Fetch::Request(request).send().await {
        Ok(response) => response,
        Err(_) => return Ok(OutboundMailProviderOutcome::Ambiguous),
    };
    match classify_send_status(response.status_code()) {
        SendStatus::Success => {}
        SendStatus::RetryableNotSent => {
            return Ok(OutboundMailProviderOutcome::RetryableNotSent);
        }
        SendStatus::Rejected => return Ok(OutboundMailProviderOutcome::Rejected),
        SendStatus::Ambiguous => return Ok(OutboundMailProviderOutcome::Ambiguous),
    }
    let response_too_large =
        match response_content_length_exceeds(&response, MAX_SEND_RESPONSE_BYTES) {
            Ok(value) => value,
            Err(()) => true,
        };
    if response_too_large {
        return Ok(OutboundMailProviderOutcome::Ambiguous);
    }
    let bytes = match response.bytes().await {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_SEND_RESPONSE_BYTES => bytes,
        _ => return Ok(OutboundMailProviderOutcome::Ambiguous),
    };
    let document: GmailSendResponse = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(_) => return Ok(OutboundMailProviderOutcome::Ambiguous),
    };
    let reference = match provider_reference(&document.id) {
        Ok(reference) => reference,
        Err(()) => return Ok(OutboundMailProviderOutcome::Ambiguous),
    };
    Ok(OutboundMailProviderOutcome::Sent {
        provider_message_reference: Some(reference),
    })
}

fn send_headers(credential: &GmailSendCredential) -> Result<Headers, ()> {
    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + credential.access_token().len());
    authorization.push_str("Bearer ");
    authorization.push_str(credential.access_token());
    let auth_result = headers.set("authorization", &authorization);
    authorization.zeroize();
    auth_result.map_err(|_| ())?;
    headers.set("accept", "application/json").map_err(|_| ())?;
    headers
        .set("content-type", "application/json; charset=utf-8")
        .map_err(|_| ())?;
    Ok(headers)
}

fn provider_reference(id: &str) -> Result<ProviderMessageReference, ()> {
    if id.is_empty()
        || id.len() > 500
        || id
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(());
    }
    ProviderMessageReference::parse(format!("gmail:{id}")).map_err(|_| ())
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, ()> {
    let value = response.headers().get("content-length").map_err(|_| ())?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value.parse::<usize>().map_err(|_| ())?;
    Ok(length > maximum)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SendStatus {
    Success,
    RetryableNotSent,
    Rejected,
    Ambiguous,
}

const fn classify_send_status(status: u16) -> SendStatus {
    match status {
        200 => SendStatus::Success,
        408 | 425 | 500..=599 => SendStatus::Ambiguous,
        429 => SendStatus::RetryableNotSent,
        400..=499 => SendStatus::Rejected,
        _ => SendStatus::Ambiguous,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GmailSendRequest<'a> {
    raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
}

#[derive(Deserialize)]
struct GmailSendResponse {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::{SendStatus, classify_send_status, provider_reference};
    use application_ports::outbound_mail::ProviderMessageReference;

    #[test]
    fn send_statuses_preserve_ambiguity_boundary() {
        assert_eq!(classify_send_status(200), SendStatus::Success);
        assert_eq!(classify_send_status(400), SendStatus::Rejected);
        assert_eq!(classify_send_status(401), SendStatus::Rejected);
        assert_eq!(classify_send_status(408), SendStatus::Ambiguous);
        assert_eq!(classify_send_status(429), SendStatus::RetryableNotSent);
        assert_eq!(classify_send_status(500), SendStatus::Ambiguous);
    }

    #[test]
    fn provider_reference_is_bounded_and_prefixed() {
        let reference = provider_reference("abc123");
        assert!(reference.is_ok());
        assert_eq!(
            reference.as_ref().map(ProviderMessageReference::as_str),
            Ok("gmail:abc123")
        );
    }
}
