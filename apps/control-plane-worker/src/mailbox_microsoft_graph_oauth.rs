use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use application_ports::MailboxOnboardingVersion;
use application_ports::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthAuthorizationCode, MicrosoftGraphOAuthState,
};
use cloudflare_adapters::d1_mailbox_onboarding::D1MailboxOnboardingApplicationRepository;
use cloudflare_adapters::microsoft_graph_oauth_provisioning::CloudflareMicrosoftGraphOAuthProvisioningPort;
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::MailboxOnboardingId;
use serde::{Deserialize, Serialize};
use use_cases_mailboxes::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthOnboardingError, complete_microsoft_graph_oauth_callback,
    deny_microsoft_graph_oauth_callback, inspect_microsoft_graph_oauth_callback,
    start_microsoft_graph_oauth_onboarding,
};
use worker::{Env, Method, Request, Response, Result};

const CALLBACK_PATH: &str = "/api/v1/mailbox/microsoft-graph/oauth/callback";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartMicrosoftGraphOAuthRequest {
    expected_version: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftGraphOAuthStartReceipt {
    onboarding_id: String,
    expected_version: u64,
    ceremony_id: String,
    authorization_url: String,
    expires_at_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftGraphOAuthCallbackReceipt {
    result_code: &'static str,
    onboarding_id: String,
    onboarding_version: u64,
}

#[must_use]
pub fn is_microsoft_graph_oauth_path(path: &str) -> bool {
    if path == CALLBACK_PATH {
        return true;
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        [
            "api",
            "v1",
            "tenants",
            _,
            "mailbox-onboardings",
            _,
            "microsoft-graph-oauth"
        ]
    )
}

pub async fn dispatch(request: &mut Request, env: &Env) -> Result<Response> {
    if request.path() == CALLBACK_PATH {
        return callback(request, env).await;
    }
    start(request, env).await
}

async fn start(request: &mut Request, env: &Env) -> Result<Response> {
    if request.method() != Method::Post {
        return not_found(&correlation_hint(request));
    }
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let onboarding_id = match segments
        .get(5)
        .and_then(|value| MailboxOnboardingId::parse((*value).to_owned()).ok())
    {
        Some(value) => value,
        None => return not_found(&correlation_hint(request)),
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return not_found(&correlation_hint(request));
    };
    let body = match request.json::<StartMicrosoftGraphOAuthRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    if body.expected_version == 0 {
        return invalid_request(actor.actor().correlation_id().as_str());
    }

    let onboarding_port = onboarding_repository(env)?;
    let provisioning_port = CloudflareMicrosoftGraphOAuthProvisioningPort::new(env);
    match start_microsoft_graph_oauth_onboarding(
        actor.actor(),
        membership_role(&actor),
        &onboarding_port,
        &provisioning_port,
        onboarding_id,
        MailboxOnboardingVersion::new(body.expected_version),
    )
    .await
    {
        Ok(outcome) => json_no_store(&MicrosoftGraphOAuthStartReceipt {
            onboarding_id: outcome.onboarding_id().as_str().to_owned(),
            expected_version: outcome.expected_version().value(),
            ceremony_id: outcome.receipt().ceremony_id().as_str().to_owned(),
            authorization_url: outcome.receipt().authorization_url().as_str().to_owned(),
            expires_at_ms: outcome.receipt().expires_at().value(),
        }),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

async fn callback(request: &Request, env: &Env) -> Result<Response> {
    if request.method() != Method::Get {
        return not_found(&correlation_hint(request));
    }
    let query = match callback_query(request) {
        Ok(value) => value,
        Err(()) => return invalid_request(&correlation_hint(request)),
    };
    let state = match MicrosoftGraphOAuthState::parse(query.state) {
        Ok(value) => value,
        Err(_) => return invalid_request(&correlation_hint(request)),
    };
    let provisioning_port = CloudflareMicrosoftGraphOAuthProvisioningPort::new(env);
    let target = match inspect_microsoft_graph_oauth_callback(&provisioning_port, &state).await {
        Ok(value) => value,
        Err(error) => return operation_failure(&correlation_hint(request), error),
    };
    let Some(actor) =
        resolve_active_request_actor(request, env, Some(target.tenant_id().as_str())).await?
    else {
        return not_found(&correlation_hint(request));
    };
    let role = membership_role(&actor);

    if query.provider_error.is_some() {
        if query.authorization_code.is_some() {
            return invalid_request(actor.actor().correlation_id().as_str());
        }
        match deny_microsoft_graph_oauth_callback(
            actor.actor(),
            role,
            &provisioning_port,
            &target,
            &state,
        )
        .await
        {
            Ok(()) => {
                return json_no_store(&MicrosoftGraphOAuthCallbackReceipt {
                    result_code: "denied",
                    onboarding_id: target.onboarding_id().as_str().to_owned(),
                    onboarding_version: target.expected_version().value(),
                });
            }
            Err(error) => {
                return operation_failure(actor.actor().correlation_id().as_str(), error);
            }
        }
    }

    let Some(code) = query.authorization_code else {
        return invalid_request(actor.actor().correlation_id().as_str());
    };
    let authorization_code = match MicrosoftGraphOAuthAuthorizationCode::parse(code) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_oauth_callback(
        actor.actor(),
        target.onboarding_id(),
        state.as_str(),
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.actor().correlation_id().as_str()),
    };
    let onboarding_port = onboarding_repository(env)?;
    match complete_microsoft_graph_oauth_callback(
        actor.actor(),
        role,
        &onboarding_port,
        &provisioning_port,
        &target,
        &state,
        authorization_code,
        evidence,
    )
    .await
    {
        Ok(outcome) => json_no_store(&MicrosoftGraphOAuthCallbackReceipt {
            result_code: "activated",
            onboarding_id: outcome.onboarding_id().as_str().to_owned(),
            onboarding_version: outcome.version().value(),
        }),
        Err(error) => operation_failure(actor.actor().correlation_id().as_str(), error),
    }
}

fn onboarding_repository(env: &Env) -> Result<D1MailboxOnboardingApplicationRepository> {
    Ok(D1MailboxOnboardingApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

struct CallbackQuery {
    state: String,
    authorization_code: Option<String>,
    provider_error: Option<String>,
}

fn callback_query(request: &Request) -> core::result::Result<CallbackQuery, ()> {
    let url = request.url().map_err(|_| ())?;
    let mut state = None;
    let mut authorization_code = None;
    let mut provider_error = None;
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "state" if state.is_none() => state = Some(value.into_owned()),
            "code" if authorization_code.is_none() => authorization_code = Some(value.into_owned()),
            "error" if provider_error.is_none() => {
                let value = value.into_owned();
                if value.len() > 128 || value.chars().any(char::is_control) {
                    return Err(());
                }
                provider_error = Some(value);
            }
            "state" | "code" | "error" => return Err(()),
            _ => {}
        }
    }
    Ok(CallbackQuery {
        state: state.ok_or(())?,
        authorization_code,
        provider_error,
    })
}

fn operation_failure(
    correlation_id: &str,
    error: MicrosoftGraphOAuthOnboardingError,
) -> Result<Response> {
    let response = match error {
        MicrosoftGraphOAuthOnboardingError::NotFound => neutral_not_found(correlation_id)?,
        MicrosoftGraphOAuthOnboardingError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")?
        }
        MicrosoftGraphOAuthOnboardingError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")?
        }
        MicrosoftGraphOAuthOnboardingError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")?
        }
        MicrosoftGraphOAuthOnboardingError::Expired => {
            problem(correlation_id, 410, "invalid_state", "Expired")?
        }
        MicrosoftGraphOAuthOnboardingError::ReplayRejected => {
            problem(correlation_id, 409, "replay_rejected", "Replay Rejected")?
        }
        MicrosoftGraphOAuthOnboardingError::ProviderDenied => {
            problem(correlation_id, 409, "invalid_state", "Authorization Denied")?
        }
        MicrosoftGraphOAuthOnboardingError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        )?,
        MicrosoftGraphOAuthOnboardingError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        )?,
        MicrosoftGraphOAuthOnboardingError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")?
        }
    };
    no_store(response)
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    let response = problem(correlation_id, 400, "invalid_request", "Invalid Request")?;
    no_store(response)
}

fn not_found(correlation_id: &str) -> Result<Response> {
    let response = neutral_not_found(correlation_id)?;
    no_store(response)
}

fn json_no_store<T: Serialize>(value: &T) -> Result<Response> {
    no_store(Response::from_json(value)?)
}

fn no_store(mut response: Response) -> Result<Response> {
    response.headers_mut().set("cache-control", "no-store")?;
    response.headers_mut().set("pragma", "no-cache")?;
    response
        .headers_mut()
        .set("referrer-policy", "no-referrer")?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::{CallbackQuery, is_microsoft_graph_oauth_path};

    #[test]
    fn microsoft_graph_oauth_routes_are_exact() {
        assert!(is_microsoft_graph_oauth_path(
            "/api/v1/tenants/tenant_01/mailbox-onboardings/onboarding_01/microsoft-graph-oauth"
        ));
        assert!(is_microsoft_graph_oauth_path(
            "/api/v1/mailbox/microsoft-graph/oauth/callback"
        ));
        for path in [
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01",
            "/api/v1/mailbox/microsoft-graph/oauth/callback/extra",
            "/api/v1/tenants/tenant_01/mailbox-onboardings/onboarding_01/microsoft-graph-oauth/extra",
        ] {
            assert!(!is_microsoft_graph_oauth_path(path));
        }
    }

    #[test]
    fn callback_query_type_contains_no_public_secret_handle() {
        let value = CallbackQuery {
            state: "state_0123456789abcdef".to_owned(),
            authorization_code: None,
            provider_error: Some("access_denied".to_owned()),
        };
        assert_eq!(value.provider_error.as_deref(), Some("access_denied"));
    }
}
