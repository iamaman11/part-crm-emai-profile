use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use application_ports::query::{QueryCursor, QueryPageRequest, QueryPageSize};
use application_ports::query_mail_provider::{
    MailSearchTerm, MailboxMessageReference, SearchClientMailboxMessagesRequest,
};
use cloudflare_adapters::cloud_mail_query::CloudMailboxQueryAdapter;
use cloudflare_adapters::d1_client_mail_eligibility::D1ClientMailboxEligibilityRepository;
use cloudflare_adapters::d1_query::D1QueryRepository;
use profile_platform_primitives::{ClientId, MailboxBindingId};
use serde_json::{Map, Value, json};
use use_cases_query::{
    QueryApplicationError, get_client_mailbox_message, search_client_mailbox_messages,
};
use worker::{Env, Request, Response, Result};

pub fn matches_route(method: &str, path: &str) -> bool {
    if method != "POST" {
        return false;
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        ["api", "v1", "tenants", _, "clients", _, "mail", "search" | "message"]
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
    let operation = segments.get(7).copied().unwrap_or_default();
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let client_id = match ClientId::parse(client_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };

    let authorization = D1QueryRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    let eligibility = D1ClientMailboxEligibilityRepository::new(
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );
    let provider = CloudMailboxQueryAdapter::new(
        env,
        env.d1(control_plane_contract::D1_CATALOG_BINDING)?,
    );

    match operation {
        "search" => {
            let body = match request.json::<Value>().await {
                Ok(value) => value,
                Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            let (binding_id, query) = match parse_search_input(body) {
                Ok(value) => value,
                Err(()) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            match search_client_mailbox_messages(
                actor.actor(),
                &authorization,
                &eligibility,
                &provider,
                &client_id,
                &binding_id,
                &query,
            )
            .await
            {
                Ok(page) => Response::from_json(&json!({
                    "messages": page.items().iter().map(summary_json).collect::<Vec<_>>(),
                    "nextCursor": page.next_cursor().map(|cursor| cursor.as_str()),
                })),
                Err(error) => query_failure(actor.actor().correlation_id().as_str(), error),
            }
        }
        "message" => {
            let body = match request.json::<Value>().await {
                Ok(value) => value,
                Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            let reference = match parse_message_reference(body) {
                Ok(value) => value,
                Err(()) => return invalid_request(actor.actor().correlation_id().as_str()),
            };
            match get_client_mailbox_message(
                actor.actor(),
                &authorization,
                &eligibility,
                &provider,
                &client_id,
                &reference,
            )
            .await
            {
                Ok(Some(message)) => Response::from_json(&json!({
                    "summary": summary_json(message.summary()),
                    "textBody": message.text_body(),
                    "htmlBody": message.html_body(),
                })),
                Ok(None) => neutral_not_found(actor.actor().correlation_id().as_str()),
                Err(error) => query_failure(actor.actor().correlation_id().as_str(), error),
            }
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

fn parse_search_input(
    body: Value,
) -> Result<(MailboxBindingId, SearchClientMailboxMessagesRequest), ()> {
    let object = exact_object(body, &["mailboxBindingId", "term", "cursor", "limit"])?;
    let binding_id =
        MailboxBindingId::parse(required_string(&object, "mailboxBindingId")?).map_err(|_| ())?;
    let term = match nullable_string(&object, "term")? {
        Some(value) => Some(MailSearchTerm::parse(value).map_err(|_| ())?),
        None => None,
    };
    let cursor = match nullable_string(&object, "cursor")? {
        Some(value) => Some(QueryCursor::parse(value).map_err(|_| ())?),
        None => None,
    };
    let limit = object.get("limit").and_then(Value::as_u64).ok_or(())?;
    let limit = u16::try_from(limit).map_err(|_| ())?;
    let page = QueryPageRequest::new(QueryPageSize::new(limit).map_err(|_| ())?, cursor);
    Ok((binding_id, SearchClientMailboxMessagesRequest::new(term, page)))
}

fn parse_message_reference(body: Value) -> Result<MailboxMessageReference, ()> {
    let object = exact_object(body, &["mailboxBindingId", "providerReference"])?;
    let binding_id =
        MailboxBindingId::parse(required_string(&object, "mailboxBindingId")?).map_err(|_| ())?;
    MailboxMessageReference::new(binding_id, required_string(&object, "providerReference")?)
        .map_err(|_| ())
}

fn exact_object(body: Value, keys: &[&str]) -> Result<Map<String, Value>, ()> {
    let object = body.as_object().ok_or(())?;
    if object.len() != keys.len() || !object.keys().all(|key| keys.contains(&key.as_str())) {
        return Err(());
    }
    Ok(object.clone())
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ()> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(())
}

fn nullable_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, ()> {
    match object.get(key).ok_or(())? {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        _ => Err(()),
    }
}

fn summary_json(summary: &application_ports::query_mail_provider::MailMessageSummary) -> Value {
    json!({
        "reference": {
            "mailboxBindingId": summary.reference().binding_id().as_str(),
            "providerReference": summary.reference().provider_reference(),
        },
        "subject": summary.subject(),
        "sender": summary.sender(),
        "receivedAtMs": summary.received_at().value(),
    })
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

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        400,
        "invalid_request",
        "Invalid Request",
    )
}

#[cfg(test)]
mod tests {
    use super::{matches_route, parse_message_reference, parse_search_input};
    use serde_json::json;

    #[test]
    fn route_matcher_is_exact_and_post_only() {
        assert!(matches_route(
            "POST",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/search"
        ));
        assert!(matches_route(
            "POST",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/message"
        ));
        assert!(!matches_route(
            "GET",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/search"
        ));
        assert!(!matches_route(
            "POST",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/unknown"
        ));
    }

    #[test]
    fn transient_inputs_reject_unknown_fields_and_control_values() {
        assert!(
            parse_search_input(json!({
                "mailboxBindingId": "binding_01JMAILQUERY",
                "term": "subject:test",
                "cursor": null,
                "limit": 25,
            }))
            .is_ok()
        );
        assert!(
            parse_search_input(json!({
                "mailboxBindingId": "binding_01JMAILQUERY",
                "term": "bad\nterm",
                "cursor": null,
                "limit": 25,
            }))
            .is_err()
        );
        assert!(
            parse_message_reference(json!({
                "mailboxBindingId": "binding_01JMAILQUERY",
                "providerReference": "provider-message-1",
                "unexpected": true,
            }))
            .is_err()
        );
    }
}
