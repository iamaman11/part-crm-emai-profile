use capability_policy::EffectiveProfile;
use cloudflare_adapters::access_identity::{AccessJwtConfig, VerifiedExternalIdentity};
use cloudflare_adapters::access_webcrypto::{AccessJwks, verify_rs256};
use cloudflare_adapters::d1_identity_acl::{
    D1IdentityAclRepository, ResolvedActor, ResolvedMembershipRole,
};
use control_plane_contract::D1_CATALOG_BINDING;
use control_plane_contract::public_api::{
    ActorSession, PROBLEM_CONTENT_TYPE, ProblemPayload, TenantContextProjection,
    TenantContextsProjection, problem_type_for_code,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{CorrelationId, TenantId, TenantScope};
use worker::{Date, Env, Error, Fetch, Request, Response, Result, Url};

const ACCESS_TOKEN_HEADER: &str = "Cf-Access-Jwt-Assertion";
const TENANT_HEADER: &str = "X-Tenant-Id";
const CORRELATION_HEADER: &str = "X-Correlation-Id";
const ACCESS_ISSUER_VAR: &str = "ACCESS_ISSUER";
const ACCESS_AUDIENCE_VAR: &str = "ACCESS_AUDIENCE";

pub struct VerifiedRequestIdentity {
    scope: TenantScope,
    correlation_id: CorrelationId,
    identity: VerifiedExternalIdentity,
}

impl VerifiedRequestIdentity {
    #[must_use]
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    #[must_use]
    pub const fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    #[must_use]
    pub const fn identity(&self) -> &VerifiedExternalIdentity {
        &self.identity
    }
}

pub async fn session_response(
    request: &Request,
    env: &Env,
    profile: &EffectiveProfile,
) -> Result<Response> {
    let Some(resolved) = resolve_active_request_actor(request, env, None).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    Response::from_json(&ActorSession {
        tenant_id: resolved
            .actor()
            .tenant_scope()
            .tenant_id()
            .as_str()
            .to_owned(),
        actor_id: resolved.actor().actor_id().as_str().to_owned(),
        role: match resolved.role() {
            ResolvedMembershipRole::TenantOwner => "TENANT_OWNER",
            ResolvedMembershipRole::Member => "MEMBER",
        }
        .to_owned(),
        profile_id: profile.profile_id.id().to_owned(),
        profile_digest: profile.semantic_digest.to_hex(),
        capabilities: profile.capabilities.enabled_ids(),
    })
}

pub async fn tenant_contexts_response(request: &Request, env: &Env) -> Result<Response> {
    let correlation = correlation_hint(request);
    let (identity, correlation_id) = match verify_human_identity(request, env).await {
        Ok(Some(verified)) => verified,
        Ok(None) => return neutral_not_found(&correlation),
        Err(_) => return dependency_unavailable(&correlation),
    };
    let database = match env.d1(D1_CATALOG_BINDING) {
        Ok(database) => database,
        Err(_) => return dependency_unavailable(correlation_id.as_str()),
    };
    let contexts = match D1IdentityAclRepository::new(database)
        .active_tenant_contexts(&identity)
        .await
    {
        Ok(contexts) => contexts,
        Err(error) => return tenant_context_repository_failure(correlation_id.as_str(), error),
    };
    Response::from_json(&TenantContextsProjection {
        tenants: contexts
            .into_iter()
            .map(|context| TenantContextProjection {
                tenant_id: context.tenant_id().as_str().to_owned(),
                display_name: context.display_name().to_owned(),
                actor_id: context.actor_id().as_str().to_owned(),
                role: match context.role() {
                    ResolvedMembershipRole::TenantOwner => "TENANT_OWNER",
                    ResolvedMembershipRole::Member => "MEMBER",
                }
                .to_owned(),
            })
            .collect(),
    })
}

fn tenant_context_repository_failure(correlation_id: &str, error: Error) -> Result<Response> {
    match error {
        Error::RustError(_) => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        _ => dependency_unavailable(correlation_id),
    }
}

fn dependency_unavailable(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        503,
        "dependency_unavailable",
        "Dependency Unavailable",
    )
}

pub async fn resolve_active_request_actor(
    request: &Request,
    env: &Env,
    path_tenant_id: Option<&str>,
) -> Result<Option<ResolvedActor>> {
    let Some(verified) = verify_request_identity(request, env, path_tenant_id).await? else {
        return Ok(None);
    };
    D1IdentityAclRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .resolve_active_actor(
            verified.scope().clone(),
            verified.identity(),
            verified.correlation_id().clone(),
        )
        .await
}

#[must_use]
pub const fn membership_role(resolved: &ResolvedActor) -> MembershipRole {
    match resolved.role() {
        ResolvedMembershipRole::TenantOwner => MembershipRole::TenantOwner,
        ResolvedMembershipRole::Member => MembershipRole::Member,
    }
}

pub async fn verify_request_identity(
    request: &Request,
    env: &Env,
    path_tenant_id: Option<&str>,
) -> Result<Option<VerifiedRequestIdentity>> {
    let tenant_value = match path_tenant_id {
        Some(value) => value.to_owned(),
        None => {
            let Some(value) = request.headers().get(TENANT_HEADER)? else {
                return Ok(None);
            };
            value
        }
    };
    let tenant_id = match TenantId::parse(tenant_value) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some((identity, correlation_id)) = verify_human_identity(request, env).await? else {
        return Ok(None);
    };

    Ok(Some(VerifiedRequestIdentity {
        scope: TenantScope::new(tenant_id),
        correlation_id,
        identity,
    }))
}

async fn verify_human_identity(
    request: &Request,
    env: &Env,
) -> Result<Option<(VerifiedExternalIdentity, CorrelationId)>> {
    let Some(correlation_value) = request.headers().get(CORRELATION_HEADER)? else {
        return Ok(None);
    };
    let correlation_id = match CorrelationId::parse(correlation_value) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(identity) = verify_access_assertion(request, env, ACCESS_AUDIENCE_VAR).await? else {
        return Ok(None);
    };
    Ok(Some((identity, correlation_id)))
}

/// Verify a Cloudflare Access assertion against one explicit audience variable.
///
/// Human and machine ingress share this cryptographic owner, but callers choose distinct audiences
/// and perform their own principal resolution after signature/issuer/time validation succeeds.
pub(crate) async fn verify_access_assertion(
    request: &Request,
    env: &Env,
    audience_var: &str,
) -> Result<Option<VerifiedExternalIdentity>> {
    let Some(token) = request.headers().get(ACCESS_TOKEN_HEADER)? else {
        return Ok(None);
    };
    let issuer = env.var(ACCESS_ISSUER_VAR)?.to_string();
    let audience = env.var(audience_var)?.to_string();
    let config = AccessJwtConfig::new(issuer.clone(), audience)
        .map_err(|error| Error::RustError(error.to_string()))?;
    let now_epoch_seconds = Date::now().as_millis() / 1000;
    let prepared = match config.prepare(&token, now_epoch_seconds) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let certs_url = Url::parse(&format!(
        "{}/cdn-cgi/access/certs",
        issuer.trim_end_matches('/')
    ))
    .map_err(|error| Error::RustError(error.to_string()))?;
    let mut response = Fetch::Url(certs_url).send().await?;
    if response.status_code() != 200 {
        return Err(Error::RustError(format!(
            "Access JWKS endpoint returned {}",
            response.status_code()
        )));
    }
    let jwks: AccessJwks = response.json().await?;
    let Some(key) = jwks.matching_key(prepared.key_id()) else {
        return Ok(None);
    };
    let signature_valid = verify_rs256(&prepared, key).await?;
    match config.accept_verified(prepared, signature_valid) {
        Ok(identity) => Ok(Some(identity)),
        Err(_) => Ok(None),
    }
}

pub fn problem(
    correlation_id: &str,
    status: u16,
    code: &'static str,
    title: &'static str,
) -> Result<Response> {
    let mut response = Response::from_json(&ProblemPayload {
        problem_type: problem_type_for_code(code).to_owned(),
        title: title.to_owned(),
        status,
        code: code.to_owned(),
        correlation_id: correlation_id.to_owned(),
    })?
    .with_status(status);
    response
        .headers_mut()
        .set("content-type", PROBLEM_CONTENT_TYPE)?;
    Ok(response)
}

pub fn neutral_not_found(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 404, "not_found", "Not Found")
}

pub fn correlation_hint(request: &Request) -> String {
    request
        .headers()
        .get(CORRELATION_HEADER)
        .ok()
        .flatten()
        .unwrap_or_else(|| "corr_unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        ACCESS_AUDIENCE_VAR, PROBLEM_CONTENT_TYPE, problem_type_for_code,
        tenant_context_repository_failure,
    };
    use worker::Error;

    #[test]
    fn human_access_audience_remains_explicit_and_separate_from_machine_ingress() {
        assert_eq!(ACCESS_AUDIENCE_VAR, "ACCESS_AUDIENCE");
        assert_ne!(ACCESS_AUDIENCE_VAR, "BRIDGE_ACCESS_AUDIENCE");
    }

    #[test]
    fn tenant_context_repository_integrity_failures_do_not_escape_as_raw_worker_errors() {
        let response = tenant_context_repository_failure(
            "corr_01JTENANTCTX",
            Error::RustError("invalid membership role".to_owned()),
        )
        .expect("problem response");
        assert_eq!(response.status_code(), 500);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .expect("content type"),
            Some(PROBLEM_CONTENT_TYPE.to_owned())
        );
    }

    #[test]
    fn every_stable_problem_code_has_its_own_type() {
        let cases = [
            ("not_found", "urn:part-crm:problem:not-found"),
            ("forbidden", "urn:part-crm:problem:forbidden"),
            ("invalid_request", "urn:part-crm:problem:invalid-request"),
            ("invalid_state", "urn:part-crm:problem:invalid-state"),
            ("version_conflict", "urn:part-crm:problem:version-conflict"),
            ("lease_conflict", "urn:part-crm:problem:lease-conflict"),
            ("replay_rejected", "urn:part-crm:problem:replay-rejected"),
            (
                "dependency_unavailable",
                "urn:part-crm:problem:dependency-unavailable",
            ),
            (
                "integrity_failure",
                "urn:part-crm:problem:integrity-failure",
            ),
            ("internal_failure", "urn:part-crm:problem:internal-failure"),
            ("conflict", "urn:part-crm:problem:conflict"),
        ];
        for (code, expected) in cases {
            assert_eq!(problem_type_for_code(code), expected);
        }
        assert_eq!(
            problem_type_for_code("unknown_code"),
            "urn:part-crm:problem:internal-failure"
        );
    }

    #[test]
    fn problem_response_media_type_is_stable_without_wasm_runtime() {
        assert_eq!(PROBLEM_CONTENT_TYPE, "application/problem+json");
    }
}
