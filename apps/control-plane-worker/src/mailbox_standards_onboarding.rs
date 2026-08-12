use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use application_ports::MailboxOnboardingVersion;
use application_ports::standards_mailbox_onboarding::{
    MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthState, StandardsMailEndpoint,
    StandardsMailProtocol, StandardsMailTransportSecurity, StandardsMailboxPassword,
    StandardsMailboxUsername, StandardsPasswordMailboxConfiguration,
    StandardsPasswordProtocolCredential,
};
use cloudflare_adapters::d1_mailbox_onboarding::D1MailboxOnboardingApplicationRepository;
use cloudflare_adapters::standards_mailbox_provisioning::CloudflareStandardsMailboxProvisioningPort;
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use profile_platform_primitives::{MailboxOnboardingId, TenantId};
use serde::{Deserialize, Serialize};
use use_cases_mailboxes::standards_mailbox_onboarding::{
    StandardsMailboxActivationOutcome, StandardsMailboxOnboardingError,
    complete_microsoft_standards_oauth_callback, deny_microsoft_standards_oauth_callback,
    inspect_microsoft_standards_oauth_callback, provision_password_standards_mailbox,
    start_microsoft_standards_oauth,
};
use worker::{Env, Error, Request, Response, Result, Url};

const PASSWORD_SUFFIX: &str = "/imap-smtp/password";
const MICROSOFT_START_SUFFIX: &str = "/imap-smtp/microsoft-oauth";
const MICROSOFT_CALLBACK_PATH: &str = "/api/v1/mailbox/imap-smtp/microsoft-oauth/callback";

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TransportSecurityDto {
    ImplicitTls,
    Starttls,
}

impl TransportSecurityDto {
    const fn application(self) -> StandardsMailTransportSecurity {
        match self {
            Self::ImplicitTls => StandardsMailTransportSecurity::ImplicitTls,
            Self::Starttls => StandardsMailTransportSecurity::StartTls,
        }
    }

    const fn evidence_value(self) -> &'static str {
        match self {
            Self::ImplicitTls => "IMPLICIT_TLS",
            Self::Starttls => "STARTTLS",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PasswordProtocolDto {
    host: String,
    port: u16,
    transport_security: TransportSecurityDto,
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisionPasswordRequestDto {
    expected_version: u64,
    imap: PasswordProtocolDto,
    smtp: PasswordProtocolDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartMicrosoftOAuthRequestDto {
    expected_version: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivationReceiptDto {
    result_code: &'static str,
    onboarding_id: String,
    onboarding_version: u64,
    authentication_mode: &'static str,
    imap_read_search_ready: bool,
    smtp_send_ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftStartReceiptDto {
    onboarding_id: String,
    expected_version: u64,
    authentication_mode: &'static str,
    ceremony_id: String,
    authorization_url: String,
    expires_at_ms: u64,
}

struct CallbackQuery {
    state: String,
    authorization_code: Option<String>,
    provider_error: Option<String>,
}

#[must_use]
pub(crate) fn is_request(path: &str) -> bool {
    if path == MICROSOFT_CALLBACK_PATH {
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
            "imap-smtp",
            "password" | "microsoft-oauth"
        ]
    )
}

pub(crate) async fn handle(mut request: Request, env: &Env, route: RouteClass) -> Result<Response> {
    if route != RouteClass::MailboxBindingResourceApi {
        return not_found(&correlation_hint(&request));
    }
    let path = request.path();
    if path == MICROSOFT_CALLBACK_PATH {
        return callback(&request, env).await;
    }
    if path.ends_with(PASSWORD_SUFFIX) {
        return provision_password(&mut request, env).await;
    }
    if path.ends_with(MICROSOFT_START_SUFFIX) {
        return start_microsoft(&mut request, env).await;
    }
    not_found(&correlation_hint(&request))
}

async fn provision_password(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let (tenant_id, onboarding_id) = match onboarding_target(&path, PASSWORD_SUFFIX) {
        Ok(value) => value,
        Err(()) => return not_found(&correlation_hint(request)),
    };
    let Some(resolved) =
        resolve_active_request_actor(request, env, Some(tenant_id.as_str())).await?
    else {
        return not_found(&correlation_hint(request));
    };
    let role = membership_role(&resolved);
    let actor = resolved.actor();
    let body: ProvisionPasswordRequestDto = match request.json().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_version = MailboxOnboardingVersion::new(body.expected_version);
    if expected_version.value() == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }

    let imap_transport = body.imap.transport_security;
    let smtp_transport = body.smtp.transport_security;
    let imap_endpoint = match StandardsMailEndpoint::parse(
        StandardsMailProtocol::Imap,
        body.imap.host,
        body.imap.port,
        imap_transport.application(),
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let imap_username = match StandardsMailboxUsername::parse(body.imap.username) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let imap_password = match StandardsMailboxPassword::parse(body.imap.password) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let smtp_endpoint = match StandardsMailEndpoint::parse(
        StandardsMailProtocol::Smtp,
        body.smtp.host,
        body.smtp.port,
        smtp_transport.application(),
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let smtp_username = match StandardsMailboxUsername::parse(body.smtp.username) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let smtp_password = match StandardsMailboxPassword::parse(body.smtp.password) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_standards_password_onboarding(
        request,
        actor,
        &onboarding_id,
        expected_version.value(),
        imap_endpoint.host(),
        imap_endpoint.port(),
        imap_transport.evidence_value(),
        imap_username.as_str(),
        smtp_endpoint.host(),
        smtp_endpoint.port(),
        smtp_transport.evidence_value(),
        smtp_username.as_str(),
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let configuration = StandardsPasswordMailboxConfiguration::new(
        StandardsPasswordProtocolCredential::new(imap_endpoint, imap_username, imap_password),
        StandardsPasswordProtocolCredential::new(smtp_endpoint, smtp_username, smtp_password),
    )
    .map_err(|_| Error::RustError("validated protocol pairing was rejected".to_owned()))?;
    let onboarding_port = onboarding_repository(env)?;
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    match provision_password_standards_mailbox(
        actor,
        role,
        &onboarding_port,
        &provisioning_port,
        onboarding_id,
        expected_version,
        configuration,
        evidence,
    )
    .await
    {
        Ok(outcome) => activation_response(&outcome, "activated"),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn start_microsoft(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let (tenant_id, onboarding_id) = match onboarding_target(&path, MICROSOFT_START_SUFFIX) {
        Ok(value) => value,
        Err(()) => return not_found(&correlation_hint(request)),
    };
    let Some(resolved) =
        resolve_active_request_actor(request, env, Some(tenant_id.as_str())).await?
    else {
        return not_found(&correlation_hint(request));
    };
    let role = membership_role(&resolved);
    let actor = resolved.actor();
    let body: StartMicrosoftOAuthRequestDto = match request.json().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_version = MailboxOnboardingVersion::new(body.expected_version);
    if expected_version.value() == 0 {
        return invalid_request(actor.correlation_id().as_str());
    }
    let onboarding_port = onboarding_repository(env)?;
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    match start_microsoft_standards_oauth(
        actor,
        role,
        &onboarding_port,
        &provisioning_port,
        onboarding_id,
        expected_version,
    )
    .await
    {
        Ok(outcome) => json_no_store(&MicrosoftStartReceiptDto {
            onboarding_id: outcome.onboarding_id().as_str().to_owned(),
            expected_version: outcome.expected_version().value(),
            authentication_mode: "MICROSOFT_OAUTH2",
            ceremony_id: outcome.receipt().ceremony_id().as_str().to_owned(),
            authorization_url: outcome.receipt().authorization_url().as_str().to_owned(),
            expires_at_ms: outcome.receipt().expires_at().value(),
        }),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn callback(request: &Request, env: &Env) -> Result<Response> {
    let query = match callback_query(request) {
        Ok(value) => value,
        Err(()) => return invalid_request(&correlation_hint(request)),
    };
    let state = match MicrosoftStandardsOAuthState::parse(query.state.clone()) {
        Ok(value) => value,
        Err(_) => return invalid_request(&correlation_hint(request)),
    };
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    let target = match inspect_microsoft_standards_oauth_callback(&provisioning_port, &state).await
    {
        Ok(value) => value,
        Err(error) => return operation_failure(&correlation_hint(request), error),
    };
    let Some(resolved) =
        resolve_active_request_actor(request, env, Some(target.tenant_id().as_str())).await?
    else {
        return not_found(&correlation_hint(request));
    };
    let role = membership_role(&resolved);
    let actor = resolved.actor();

    if query.provider_error.is_some() {
        if query.authorization_code.is_some() {
            return invalid_request(actor.correlation_id().as_str());
        }
        return match deny_microsoft_standards_oauth_callback(
            actor,
            role,
            &provisioning_port,
            &target,
            &state,
        )
        .await
        {
            Ok(()) => json_no_store(&ActivationReceiptDto {
                result_code: "denied",
                onboarding_id: target.onboarding_id().as_str().to_owned(),
                onboarding_version: target.expected_version().value(),
                authentication_mode: "MICROSOFT_OAUTH2",
                imap_read_search_ready: false,
                smtp_send_ready: false,
            }),
            Err(error) => operation_failure(actor.correlation_id().as_str(), error),
        };
    }

    let Some(code) = query.authorization_code else {
        return invalid_request(actor.correlation_id().as_str());
    };
    let authorization_code = match MicrosoftStandardsOAuthAuthorizationCode::parse(code) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let evidence = match command_evidence::from_standards_oauth_callback(
        actor,
        target.onboarding_id(),
        &query.state,
    ) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let onboarding_port = onboarding_repository(env)?;
    match complete_microsoft_standards_oauth_callback(
        actor,
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
        Ok(outcome) => activation_response(&outcome, "activated"),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn onboarding_repository(env: &Env) -> Result<D1MailboxOnboardingApplicationRepository> {
    Ok(D1MailboxOnboardingApplicationRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

fn activation_response(
    outcome: &StandardsMailboxActivationOutcome,
    result_code: &'static str,
) -> Result<Response> {
    json_no_store(&ActivationReceiptDto {
        result_code,
        onboarding_id: outcome.onboarding_id().as_str().to_owned(),
        onboarding_version: outcome.version().value(),
        authentication_mode: outcome.authentication_mode().public_value(),
        imap_read_search_ready: outcome.imap_read_search_ready(),
        smtp_send_ready: outcome.smtp_send_ready(),
    })
}

fn onboarding_target(
    path: &str,
    suffix: &str,
) -> core::result::Result<(TenantId, MailboxOnboardingId), ()> {
    if !path.ends_with(suffix) {
        return Err(());
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let [
        "api",
        "v1",
        "tenants",
        tenant,
        "mailbox-onboardings",
        onboarding,
        "imap-smtp",
        operation,
    ] = segments.as_slice()
    else {
        return Err(());
    };
    let expected_operation = if suffix == PASSWORD_SUFFIX {
        "password"
    } else if suffix == MICROSOFT_START_SUFFIX {
        "microsoft-oauth"
    } else {
        return Err(());
    };
    if operation != &expected_operation {
        return Err(());
    }
    let tenant_id = TenantId::parse((*tenant).to_owned()).map_err(|_| ())?;
    let onboarding_id = MailboxOnboardingId::parse((*onboarding).to_owned()).map_err(|_| ())?;
    Ok((tenant_id, onboarding_id))
}

fn callback_query(request: &Request) -> core::result::Result<CallbackQuery, ()> {
    let url = request.url().map_err(|_| ())?;
    parse_callback_query(&url)
}

fn parse_callback_query(url: &Url) -> core::result::Result<CallbackQuery, ()> {
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
    error: StandardsMailboxOnboardingError,
) -> Result<Response> {
    let response = match error {
        StandardsMailboxOnboardingError::NotFound => neutral_not_found(correlation_id)?,
        StandardsMailboxOnboardingError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")?
        }
        StandardsMailboxOnboardingError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")?
        }
        StandardsMailboxOnboardingError::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")?
        }
        StandardsMailboxOnboardingError::Expired => {
            problem(correlation_id, 410, "invalid_state", "Expired")?
        }
        StandardsMailboxOnboardingError::ReplayRejected => {
            problem(correlation_id, 409, "replay_rejected", "Replay Rejected")?
        }
        StandardsMailboxOnboardingError::ProviderDenied => {
            problem(correlation_id, 409, "invalid_state", "Authorization Denied")?
        }
        StandardsMailboxOnboardingError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        )?,
        StandardsMailboxOnboardingError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        )?,
        StandardsMailboxOnboardingError::InternalFailure => {
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
    use super::{MICROSOFT_CALLBACK_PATH, is_request, parse_callback_query};
    use worker::Url;

    #[test]
    fn standards_onboarding_routes_are_exact() {
        assert!(is_request(
            "/api/v1/tenants/tenant_01/mailbox-onboardings/onboarding_01/imap-smtp/password"
        ));
        assert!(is_request(
            "/api/v1/tenants/tenant_01/mailbox-onboardings/onboarding_01/imap-smtp/microsoft-oauth"
        ));
        assert!(is_request(MICROSOFT_CALLBACK_PATH));
        for path in [
            "/api/v1/tenants/tenant_01/mailboxes/mailbox_01",
            "/api/v1/mailbox/imap-smtp/microsoft-oauth/callback/extra",
            "/api/v1/tenants/tenant_01/mailbox-onboardings/onboarding_01/imap-smtp/password/extra",
        ] {
            assert!(!is_request(path));
        }
    }

    #[test]
    fn callback_query_rejects_duplicate_security_parameters()
    -> Result<(), Box<dyn std::error::Error>> {
        let valid = Url::parse(
            "https://example.invalid/api/v1/mailbox/imap-smtp/microsoft-oauth/callback?state=state_0123456789abcdef&code=code",
        )?;
        assert!(parse_callback_query(&valid).is_ok());
        for query in [
            "state=state_0123456789abcdef&state=other&code=code",
            "state=state_0123456789abcdef&code=one&code=two",
            "state=state_0123456789abcdef&error=denied&error=again",
        ] {
            let url = Url::parse(&format!("https://example.invalid/callback?{query}"))?;
            assert!(parse_callback_query(&url).is_err());
        }
        Ok(())
    }
}
