use super::{QueryApplicationError, authorize, map_port_error};
use application_ports::client_contact_lookup::ContactLookupProtectionPort;
use application_ports::clients::{
    ContactExactLookupRequest, ContactProtectionPortError, ContactProtectionPortErrorClass,
};
use application_ports::query::{QueryAuthorizationPort, QueryCapability, QueryPageSize};
use application_ports::query_clients::{
    ClientContactExactMatchProjection, ClientExactContactQueryPort,
};
use client_domain::{
    ContactKind, ContactNormalizationVersion, exact_lookup_hmac_input, normalize_contact_value,
};
use profile_platform_primitives::ActorContext;

const MAX_EXACT_CONTACT_MATCHES: u16 = 20;
const MAX_LOOKUP_KEY_CANDIDATES: usize = 4;

pub async fn lookup_clients_by_exact_contact<A, H, P>(
    actor: &ActorContext,
    authorization: &A,
    protector: &H,
    projection: &P,
    kind: ContactKind,
    raw_value: &str,
) -> Result<Vec<ClientContactExactMatchProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    H: ContactLookupProtectionPort,
    P: ClientExactContactQueryPort,
{
    if !authorize(actor, authorization, QueryCapability::Clients).await? {
        return Ok(Vec::new());
    }

    let normalization_version = ContactNormalizationVersion::V1;
    let normalized = normalize_contact_value(kind, normalization_version, raw_value)
        .map_err(|_| QueryApplicationError::InvalidInput)?;
    let hmac_input = exact_lookup_hmac_input(
        actor.tenant_scope().tenant_id(),
        kind,
        normalization_version,
        &normalized,
    );
    let candidates = protector
        .derive_exact_lookup_candidates(ContactExactLookupRequest::new(
            actor.tenant_scope().tenant_id(),
            kind,
            normalization_version,
            &hmac_input,
        ))
        .await
        .map_err(map_protection_error)?;
    if candidates.is_empty() || candidates.len() > MAX_LOOKUP_KEY_CANDIDATES {
        return Err(QueryApplicationError::IntegrityFailure);
    }

    let limit = QueryPageSize::new(MAX_EXACT_CONTACT_MATCHES)
        .map_err(|_| QueryApplicationError::IntegrityFailure)?;
    let mut matches = Vec::new();
    for token in &candidates {
        let projected = projection
            .find_visible_clients_by_exact_contact(actor, kind, normalization_version, token, limit)
            .await
            .map_err(map_port_error)?;
        for item in projected {
            if matches.len() >= usize::from(MAX_EXACT_CONTACT_MATCHES) {
                return Ok(matches);
            }
            if !matches
                .iter()
                .any(|existing: &ClientContactExactMatchProjection| {
                    existing.client_id() == item.client_id()
                        && existing.contact_point_id() == item.contact_point_id()
                })
            {
                matches.push(item);
            }
        }
    }
    Ok(matches)
}

fn map_protection_error(error: ContactProtectionPortError) -> QueryApplicationError {
    match error.class() {
        ContactProtectionPortErrorClass::InvalidProtectedValue => {
            QueryApplicationError::IntegrityFailure
        }
        ContactProtectionPortErrorClass::KeyUnavailable
        | ContactProtectionPortErrorClass::InternalFailure => {
            QueryApplicationError::DependencyUnavailable
        }
    }
}
