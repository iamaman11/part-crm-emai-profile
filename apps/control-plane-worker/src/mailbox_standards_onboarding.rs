use crate::{command_evidence, parse_request_evidence, resolve_active_request_actor};
use application_ports::standards_mailbox_onboarding::{
    MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthState, StandardsMailEndpoint,
    StandardsMailProtocol, StandardsMailTransportSecurity, StandardsMailboxPassword,
    StandardsMailboxUsername, StandardsPasswordMailboxConfiguration,
    StandardsPasswordProtocolCredential,
};
use cloudflare_adapters::d1_mailbox_onboarding::D1MailboxOnboardingApplicationPort;
use cloudflare_adapters::standards_mailbox_provisioning::CloudflareStandardsMailboxProvisioningPort;
use control_plane_contract::RouteClass;
use identity_access_domain::MembershipRole;
use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{MailboxOnboardingId, TenantId};
use serde::{Deserialize, Serialize};
use use_cases_mailboxes::standards_mailbox_onboarding::{
    StandardsMailboxActivationOutcome, StandardsMailboxOnboardingError,
    complete_microsoft_standards_oauth_callback, deny_microsoft_standards_oauth_callback,
    inspect_microsoft_standards_oauth_callback, provision_password_standards_mailbox,
    start_microsoft_standards_oauth,
};
use worker::{Env, Error, Request, Response, Result, Url};
use worker_shared::{ProblemDetails, ProblemType, StatusCode};

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

pub(crate) fn is_request(path: &str) -> bool {
    path == MICROSOFT_CALLBACK_PATH
        || path.ends_with(PASSWORD_SUFFIX)
        || path.ends_with(MICROSOFT_START_SUFFIX)
}

pub(crate) async fn handle(
    mut request: Request,
    env: &Env,
    route: RouteClass,
) -> Result<Response> {
    if route != RouteClass::MailboxBindingResourceApi {
        return problem(
            StatusCode::NOT_FOUND,
            ProblemType::NotFound,
            "standards mailbox onboarding route not found",
        );
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
    problem(
        StatusCode::NOT_FOUND,
        ProblemType::NotFound,
        "standards mailbox onboarding route not found",
    )
}

async fn provision_password(request: &mut Request, env: &Env) -> Result<Response> {
    let (tenant_id, onboarding_id) = match onboarding_target(request.path(), PASSWORD_SUFFIX) {
        Ok(value) => value,
        Err(_) => {
            return problem(
                StatusCode::NOT_FOUND,
                ProblemType::NotFound,
                "standards mailbox onboarding target not found",
            );
        }
    };
    let request_evidence = parse_request_evidence(request)?;
    let resolved = resolve_active_request_actor(env, &tenant_id, &request_evidence).await?;
    let role = resolved.membership().role();
    let actor = resolved.into_actor();
    let body: ProvisionPasswordRequestDto = match request.json().await {
        Ok(value) => value,
        Err(_) => {
            return problem(
                StatusCode::BAD_REQUEST,
                ProblemType::InvalidRequest,
                "invalid standards mailbox password request",
            );
        }
    };
    let expected_version = MailboxOnboardingVersion::new(body.expected_version);
    if expected_version.value() == 0 {
        return problem(
            StatusCode::BAD_REQUEST,
            ProblemType::InvalidRequest,
            "invalid mailbox onboarding version",
        );
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
        Err(_) => return invalid_configuration(),
    };
    let imap_username = match StandardsMailboxUsername::parse(body.imap.username) {
        Ok(value) => value,
        Err(_) => return invalid_configuration(),
    };
    let imap_password = match StandardsMailboxPassword::parse(body.imap.password) {
        Ok(value) => value,
        Err(_) => return invalid_configuration(),
    };
    let smtp_endpoint = match StandardsMailEndpoint::parse(
        StandardsMailProtocol::Smtp,
        body.smtp.host,
        body.smtp.port,
        smtp_transport.application(),
    ) {
        Ok(value) => value,
        Err(_) => return invalid_configuration(),
    };
    let smtp_username = match StandardsMailboxUsername::parse(body.smtp.username) {
        Ok(value) => value,
        Err(_) => return invalid_configuration(),
    };
    let smtp_password = match StandardsMailboxPassword::parse(body.smtp.password) {
        Ok(value) => value,
        Err(_) => return invalid_configuration(),
    };
    let evidence = match command_evidence::from_standards_password_onboarding(
        request,
        &actor,
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
        Err(_) => {
            return problem(
                StatusCode::BAD_REQUEST,
                ProblemType::InvalidRequest,
                "missing or invalid idempotency key",
            );
        }
    };
    let configuration = StandardsPasswordMailboxConfiguration::new(
        StandardsPasswordProtocolCredential::new(imap_endpoint, imap_username, imap_password),
        StandardsPasswordProtocolCredential::new(smtp_endpoint, smtp_username, smtp_password),
    )
    .map_err(|_| Error::RustError("validated protocol pairing was rejected".to_owned()))?;
    let database = env.d1("PROFILE_DB")?;
    let onboarding_port = D1MailboxOnboardingApplicationPort::new(&database);
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    match provision_password_standards_mailbox(
        &actor,
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
        Err(error) => onboarding_problem(error),
    }
}

async fn start_microsoft(request: &mut Request, env: &Env) -> Result<Response> {
    let (tenant_id, onboarding_id) = match onboarding_target(request.path(), MICROSOFT_START_SUFFIX) {
        Ok(value) => value,
        Err(_) => {
            return problem(
                StatusCode::NOT_FOUND,
                ProblemType::NotFound,
                "standards mailbox onboarding target not found",
            );
        }
    };
    let request_evidence = parse_request_evidence(request)?;
    let resolved = resolve_active_request_actor(env, &tenant_id, &request_evidence).await?;
    let role = resolved.membership().role();
    let actor = resolved.into_actor();
    let body: StartMicrosoftOAuthRequestDto = match request.json().await {
        Ok(value) => value,
        Err(_) => {
            return problem(
                StatusCode::BAD_REQUEST,
                ProblemType::InvalidRequest,
                "invalid Microsoft standards OAuth start request",
            );
        }
    };
    let expected_version = MailboxOnboardingVersion::new(body.expected_version);
    if expected_version.value() == 0 {
        return problem(
            StatusCode::BAD_REQUEST,
            ProblemType::InvalidRequest,
            "invalid mailbox onboarding version",
        );
    }
    let database = env.d1("PROFILE_DB")?;
    let onboarding_port = D1MailboxOnboardingApplicationPort::new(&database);
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    match start_microsoft_standards_oauth(
        &actor,
        role,
        &onboarding_port,
        &provisioning_port,
        onboarding_id,
        expected_version,
    )
    .await
    {
        Ok(outcome) => response(&MicrosoftStartReceiptDto {
            onboarding_id: outcome.onboarding_id().as_str().to_owned(),
            expected_version: outcome.expected_version().value(),
            authentication_mode: "MICROSOFT_OAUTH2",
            ceremony_id: outcome.receipt().ceremony_id().as_str().to_owned(),
            authorization_url: outcome.receipt().authorization_url().as_str().to_owned(),
            expires_at_ms: outcome.receipt().expires_at().value(),
        }),
        Err(error) => onboarding_problem(error),
    }
}

async fn callback(request: &Request, env: &Env) -> Result<Response> {
    let url = request.url()?;
    let state_raw = query_parameter(&url, "state").ok_or_else(|| {
        Error::RustError("standards mailbox OAuth callback state missing".to_owned())
    })?;
    let state = MicrosoftStandardsOAuthState::parse(state_raw.clone())
        .map_err(|_| Error::RustError("invalid standards mailbox OAuth state".to_owned()))?;
    let provisioning_port = CloudflareStandardsMailboxProvisioningPort::new(env);
    let target = match inspect_microsoft_standards_oauth_callback(&provisioning_port, &state).await {
        Ok(value) => value,
        Err(error) => return onboarding_problem(error),
    };
    let request_evidence = parse_request_evidence(request)?;
    let resolved = resolve_active_request_actor(env, target.tenant_id(), &request_evidence).await?;
    let role = resolved.membership().role();
    let actor = resolved.into_actor();

    if query_parameter(&url, "error").is_some() {
        return match deny_microsoft_standards_oauth_callback(
            &actor,
            role,
            &provisioning_port,
            &target,
            &state,
        )
        .await
        {
            Ok(()) => response(&ActivationReceiptDto {
                result_code: "denied",
                onboarding_id: target.onboarding_id().as_str().to_owned(),
                onboarding_version: target.expected_version().value(),
                authentication_mode: "MICROSOFT_OAUTH2",
                imap_read_search_ready: false,
                smtp_send_ready: false,
            }),
            Err(error) => onboarding_problem(error),
        };
    }

    let code_raw = query_parameter(&url, "code").ok_or_else(|| {
        Error::RustError("standards mailbox OAuth callback code missing".to_owned())
    })?;
    let authorization_code = MicrosoftStandardsOAuthAuthorizationCode::parse(code_raw)
        .map_err(|_| Error::RustError("invalid standards mailbox OAuth code".to_owned()))?;
    let evidence = command_evidence::from_standards_oauth_callback(
        &actor,
        target.onboarding_id(),
        &state_raw,
    )?;
    let database = env.d1("PROFILE_DB")?;
    let onboarding_port = D1MailboxOnboardingApplicationPort::new(&database);
    match complete_microsoft_standards_oauth_callback(
        &actor,
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
        Err(error) => onboarding_problem(error),
    }
}

fn activation_response(
    outcome: &StandardsMailboxActivationOutcome,
    result_code: &'static str,
) -> Result<Response> {
    response(&ActivationReceiptDto {
        result_code,
        onboarding_id: outcome.onboarding_id().as_str().to_owned(),
        onboarding_version: outcome.version().value(),
        authentication_mode: outcome.authentication_mode().public_value(),
        imap_read_search_ready: outcome.imap_read_search_ready(),
        smtp_send_ready: outcome.smtp_send_ready(),
    })
}

fn onboarding_target(path: &str, suffix: &str) -> Result<(TenantId, MailboxOnboardingId)> {
    if !path.ends_with(suffix) {
        return Err(Error::RustError("standards mailbox path mismatch".to_owned()));
    }
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let ["api", "v1", "tenants", tenant, "mailbox-onboardings", onboarding, "imap-smtp", _] =
        segments.as_slice()
    else {
        return Err(Error::RustError("standards mailbox path shape invalid".to_owned()));
    };
    let tenant_id =
        TenantId::parse((*tenant).to_owned()).map_err(|error| Error::RustError(error.to_string()))?;
    let onboarding_id = MailboxOnboardingId::parse((*onboarding).to_owned())
        .map_err(|error| Error::RustError(error.to_string()))?;
    Ok((tenant_id, onboarding_id))
}

fn query_parameter(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn invalid_configuration() -> Result<Response> {
    problem(
        StatusCode::BAD_REQUEST,
        ProblemType::InvalidRequest,
        "invalid encrypted IMAP/SMTP configuration",
    )
}

fn onboarding_problem(error: StandardsMailboxOnboardingError) -> Result<Response> {
    let (status, problem_type, detail) = match error {
        StandardsMailboxOnboardingError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemType::NotFound,
            "standards mailbox onboarding target not found",
        ),
        StandardsMailboxOnboardingError::VersionConflict => (
            StatusCode::CONFLICT,
            ProblemType::VersionConflict,
            "standards mailbox onboarding version conflict",
        ),
        StandardsMailboxOnboardingError::InvalidState
        | StandardsMailboxOnboardingError::Conflict => (
            StatusCode::CONFLICT,
            ProblemType::InvalidState,
            "standards mailbox onboarding conflict",
        ),
        StandardsMailboxOnboardingError::Expired => (
            StatusCode::GONE,
            ProblemType::InvalidState,
            "standards mailbox OAuth ceremony expired",
        ),
        StandardsMailboxOnboardingError::ReplayRejected => (
            StatusCode::CONFLICT,
            ProblemType::ReplayRejected,
            "standards mailbox OAuth callback replay rejected",
        ),
        StandardsMailboxOnboardingError::ProviderDenied => (
            StatusCode::BAD_REQUEST,
            ProblemType::InvalidRequest,
            "standards mailbox provider denied authorization",
        ),
        StandardsMailboxOnboardingError::DependencyUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            ProblemType::DependencyUnavailable,
            "standards mailbox dependency unavailable",
        ),
        StandardsMailboxOnboardingError::IntegrityFailure => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ProblemType::IntegrityFailure,
            "standards mailbox integrity failure",
        ),
        StandardsMailboxOnboardingError::InternalFailure => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ProblemType::InternalFailure,
            "standards mailbox internal failure",
        ),
    };
    problem(status, problem_type, detail)
}

fn response<T: Serialize>(value: &T) -> Result<Response> {
    let response = Response::from_json(value)?.with_status(StatusCode::OK.as_u16());
    secure_response(response)
}

fn problem(status: StatusCode, problem_type: ProblemType, detail: &str) -> Result<Response> {
    let response = Response::from_json(&ProblemDetails::new(problem_type, detail))?
        .with_status(status.as_u16());
    secure_response(response)
}

fn secure_response(response: Response) -> Result<Response> {
    let headers = response.headers();
    headers.set("cache-control", "no-store")?;
    headers.set("pragma", "no-cache")?;
    headers.set("referrer-policy", "no-referrer")?;
    Ok(response)
}
