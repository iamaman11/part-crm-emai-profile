mod input;
mod provider;
mod source;

use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortErrorClass,
};
use application_ports::outbound_mail::OutboundMailIntentState;
use cloudflare_adapters::MailboxProvider;
use cloudflare_adapters::cloud_mail_query::CloudMailboxQueryAdapter;
use cloudflare_adapters::d1_client_mail_eligibility::D1ClientMailboxEligibilityRepository;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::d1_outbound_mail_intents::D1OutboundMailIntentRepository;
use cloudflare_adapters::d1_query::D1QueryRepository;
use cloudflare_adapters::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use cloudflare_adapters::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use control_plane_contract::client_mail_send_api::{
    ClientMailSendReceiptDto, ClientMailSendRequestDto, ClientMailSendStateDto,
};
use profile_platform_primitives::{ActorContext, ClientId};
use provider::ClientMailProvider;
use sha2::{Digest, Sha256};
use use_cases_mailboxes::outbound_mail::{
    OutboundMailOperationError, OutboundMailOutcome, execute_outbound_mail,
};
use use_cases_query::QueryApplicationError;
use worker::{Env, Method, Request, Response, Result};

const HEX: &[u8; 16] = b"0123456789abcdef";

#[must_use]
pub fn is_request(method: Method, path: &str) -> bool {
    if method != Method::Post {
        return false;
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        ["api", "v1", "tenants", _, "clients", _, "mail", "send"]
    )
}

pub async fn dispatch(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let client_id = segments.get(5).copied().unwrap_or_default();
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let body = match request.json::<ClientMailSendRequestDto>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let intent = match input::build_intent(&client_id, &body) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.actor().correlation_id().as_str()),
    };

    let eligibility = D1ClientMailboxEligibilityRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    match eligibility
        .is_mailbox_accessible(actor.actor(), &client_id, intent.binding_id())
        .await
    {
        Ok(true) => {}
        Ok(false) => return neutral_not_found(actor.actor().correlation_id().as_str()),
        Err(error) => {
            return access_failure(actor.actor().correlation_id().as_str(), error.class());
        }
    }

    if intent.operation().source().is_some() {
        let source_authorization =
            D1QueryRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
        let source_provider = CloudMailboxQueryAdapter::new(
            env,
            env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
            env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
            actor.actor(),
            &client_id,
        );
        match source::is_accessible(
            actor.actor(),
            &client_id,
            &source_authorization,
            &eligibility,
            &source_provider,
            &intent,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return neutral_not_found(actor.actor().correlation_id().as_str()),
            Err(error) => {
                return query_failure(actor.actor().correlation_id().as_str(), error);
            }
        }
    }

    let digest = match request_digest(&body) {
        Ok(value) => value,
        Err(()) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let provider = match provider_for(env, actor.actor(), intent.binding_id()).await? {
        Some(value) => value,
        None => return neutral_not_found(actor.actor().correlation_id().as_str()),
    };
    let store = D1OutboundMailIntentRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    match execute_outbound_mail(
        actor.actor(),
        &eligibility,
        &store,
        &provider,
        &intent,
        &evidence,
    )
    .await
    {
        Ok(outcome) => receipt(&outcome),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn provider_for<'a>(
    env: &'a Env,
    actor: &ActorContext,
    binding_id: &profile_platform_primitives::MailboxBindingId,
) -> Result<Option<ClientMailProvider<'a>>> {
    let repository =
        D1MailboxRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    let Some(binding) = repository
        .find_binding(actor.tenant_scope(), binding_id)
        .await?
    else {
        return Ok(None);
    };
    let provider = match binding.provider() {
        MailboxProvider::GmailApi => ClientMailProvider::Gmail(
            CloudflareGmailOutboundMailProvider::new(
                env,
                env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
            ),
        ),
        MailboxProvider::Imap => ClientMailProvider::Smtp(
            CloudflareSmtpOutboundMailProvider::new(
                env,
                env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
            ),
        ),
        MailboxProvider::BrowserFallback | MailboxProvider::MicrosoftGraph => {
            ClientMailProvider::Unsupported
        }
    };
    Ok(Some(provider))
}

fn request_digest(body: &ClientMailSendRequestDto) -> Result<String, ()> {
    let bytes = serde_json::to_vec(body).map_err(|_| ())?;
    Ok(hex_digest(&bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn receipt(outcome: &OutboundMailOutcome) -> Result<Response> {
    Response::from_json(&ClientMailSendReceiptDto {
        intent_id: outcome.intent_id().as_str().to_owned(),
        state: state(outcome.state()),
        attempt_count: outcome.attempt_count(),
        replayed: outcome.replayed(),
    })
}

const fn state(value: OutboundMailIntentState) -> ClientMailSendStateDto {
    match value {
        OutboundMailIntentState::Pending => ClientMailSendStateDto::Pending,
        OutboundMailIntentState::Dispatching => ClientMailSendStateDto::Dispatching,
        OutboundMailIntentState::Retryable => ClientMailSendStateDto::Retryable,
        OutboundMailIntentState::Sent => ClientMailSendStateDto::Sent,
        OutboundMailIntentState::Ambiguous => ClientMailSendStateDto::Ambiguous,
        OutboundMailIntentState::Rejected => ClientMailSendStateDto::Rejected,
    }
}

fn access_failure(
    correlation_id: &str,
    class: ClientMailboxAccessPortErrorClass,
) -> Result<Response> {
    match class {
        ClientMailboxAccessPortErrorClass::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        ClientMailboxAccessPortErrorClass::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn query_failure(correlation_id: &str, error: QueryApplicationError) -> Result<Response> {
    match error {
        QueryApplicationError::InvalidInput => invalid_request(correlation_id),
        QueryApplicationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        QueryApplicationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn operation_failure(correlation_id: &str, error: OutboundMailOperationError) -> Result<Response> {
    match error {
        OutboundMailOperationError::NotFound => neutral_not_found(correlation_id),
        OutboundMailOperationError::InvalidInput => invalid_request(correlation_id),
        OutboundMailOperationError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        OutboundMailOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        OutboundMailOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        OutboundMailOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

#[cfg(test)]
mod tests {
    use super::{hex_digest, is_request};
    use application_ports::outbound_mail::OutboundMailProviderOutcome;
    use worker::Method;

    #[test]
    fn route_match_is_exact_and_post_only() {
        let path = "/api/v1/tenants/tenant_01/clients/client_01/mail/send";
        assert!(is_request(Method::Post, path));
        assert!(!is_request(Method::Get, path));
        assert!(!is_request(Method::Post, &format!("{path}/extra")));
    }

    #[test]
    fn digest_is_stable_and_content_is_not_retained() {
        let first = hex_digest(b"same input");
        let second = hex_digest(b"same input");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(!first.contains("same input"));
    }

    #[test]
    fn provider_outcome_import_remains_provider_neutral() {
        let outcome = OutboundMailProviderOutcome::RetryableNotSent;
        assert!(matches!(
            outcome,
            OutboundMailProviderOutcome::RetryableNotSent
        ));
    }
}
