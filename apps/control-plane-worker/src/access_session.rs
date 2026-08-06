use cloudflare_adapters::access_identity::{AccessJwtConfig, VerifiedExternalIdentity};
use cloudflare_adapters::access_webcrypto::{AccessJwks, verify_rs256};
use cloudflare_adapters::d1_identity_acl::{
    D1IdentityAclRepository, ResolvedActor, ResolvedMembershipRole,
};
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::{CorrelationId, TenantId, TenantScope};
use serde::Serialize;
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

pub async fn session_response(request: &Request, env: &Env) -> Result<Response> {
    let Some(resolved) = resolve_active_request_actor(request, env, None).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    Response::from_json(&ActorSessionResponse::from(&resolved))
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
    let Some(correlation_value) = request.headers().get(CORRELATION_HEADER)? else {
        return Ok(None);
    };
    let Some(token) = request.headers().get(ACCESS_TOKEN_HEADER)? else {
        return Ok(None);
    };

    let tenant_id = match TenantId::parse(tenant_value) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let correlation_id = match CorrelationId::parse(correlation_value) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let issuer = env.var(ACCESS_ISSUER_VAR)?.to_string();
    let audience = env.var(ACCESS_AUDIENCE_VAR)?.to_string();
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
    let identity = match config.accept_verified(prepared, signature_valid) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(Some(VerifiedRequestIdentity {
        scope: TenantScope::new(tenant_id),
        correlation_id,
        identity,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActorSessionResponse<'a> {
    tenant_id: &'a str,
    actor_id: &'a str,
    role: &'static str,
}

impl<'a> From<&'a ResolvedActor> for ActorSessionResponse<'a> {
    fn from(resolved: &'a ResolvedActor) -> Self {
        Self {
            tenant_id: resolved.actor().tenant_scope().tenant_id().as_str(),
            actor_id: resolved.actor().actor_id().as_str(),
            role: match resolved.role() {
                ResolvedMembershipRole::TenantOwner => "TENANT_OWNER",
                ResolvedMembershipRole::Member => "MEMBER",
            },
        }
    }
}

#[derive(Serialize)]
struct Problem<'a> {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    code: &'static str,
    correlation_id: &'a str,
}

pub fn problem(
    correlation_id: &str,
    status: u16,
    code: &'static str,
    title: &'static str,
) -> Result<Response> {
    Response::from_json(&Problem {
        problem_type: match code {
            "conflict" => "urn:part-crm:problem:conflict",
            "invalid_request" => "urn:part-crm:problem:invalid-request",
            "internal_failure" => "urn:part-crm:problem:internal-failure",
            "forbidden" => "urn:part-crm:problem:forbidden",
            _ => "urn:part-crm:problem:not-found",
        },
        title,
        status,
        code,
        correlation_id,
    })
    .map(|response| response.with_status(status))
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
    use super::problem;

    #[test]
    fn problem_type_matches_stable_code() -> Result<(), Box<dyn std::error::Error>> {
        let internal = problem("corr_problem_test", 500, "internal_failure", "Internal Failure")?;
        assert_eq!(internal.status_code(), 500);
        let forbidden = problem("corr_problem_test", 403, "forbidden", "Forbidden")?;
        assert_eq!(forbidden.status_code(), 403);
        Ok(())
    }
}
