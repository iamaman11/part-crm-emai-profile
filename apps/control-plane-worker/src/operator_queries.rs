use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use application_ports::query_mailboxes::{MailboxBindingStatus, MailboxProvider};
use application_ports::query_profiles::ProfileStatus;
use application_ports::{QueryCursor, QueryPageRequest, QueryPageSize};
use cloudflare_adapters::d1_query::D1QueryRepository;
use control_plane_contract::RouteClass;
use control_plane_contract::operator_query_api::{
    MailboxListItemDto, MailboxListPageDto, MemberListItemDto, MemberListPageDto,
    ProfileListItemDto, ProfileListPageDto,
};
use identity_access_domain::{MembershipRole, MembershipStatus};
use use_cases_query::{QueryApplicationError, list_mailboxes, list_members, list_profiles};
use worker::{Env, Method, Request, Response, Result};

const DEFAULT_PAGE_SIZE: u16 = 50;

pub async fn dispatch(route: RouteClass, request: &Request, env: &Env) -> Result<Response> {
    if request.method() != Method::Get {
        return neutral_not_found(&correlation_hint(request));
    }
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let page = match page_request(request) {
        Ok(value) => value,
        Err(()) => {
            return problem(
                actor.actor().correlation_id().as_str(),
                400,
                "invalid_request",
                "Invalid Request",
            );
        }
    };
    let repository = D1QueryRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);

    match route {
        RouteClass::ProfileCollectionApi => {
            let result = list_profiles(&repository, &repository, actor.actor(), &page).await;
            match result {
                Ok(page) => Response::from_json(&ProfileListPageDto {
                    profiles: page
                        .items()
                        .iter()
                        .map(|profile| ProfileListItemDto {
                            profile_id: profile.profile_id().as_str().to_owned(),
                            status: profile_status(profile.status()).to_owned(),
                            version: profile.version().value(),
                            linked_client_id: profile
                                .linked_client_id()
                                .map(|value| value.as_str().to_owned()),
                            active_generation_id: profile
                                .active_generation_id()
                                .map(|value| value.as_str().to_owned()),
                        })
                        .collect(),
                    next_cursor: page.next_cursor().map(|value| value.as_str().to_owned()),
                }),
                Err(error) => query_failure(actor.actor().correlation_id().as_str(), error),
            }
        }
        RouteClass::MembershipCollectionApi => {
            let result = list_members(&repository, &repository, actor.actor(), &page).await;
            match result {
                Ok(page) => Response::from_json(&MemberListPageDto {
                    members: page
                        .items()
                        .iter()
                        .map(|member| MemberListItemDto {
                            actor_id: member.actor_id().as_str().to_owned(),
                            role: membership_role(member.role()).to_owned(),
                            status: membership_status(member.status()).to_owned(),
                        })
                        .collect(),
                    next_cursor: page.next_cursor().map(|value| value.as_str().to_owned()),
                }),
                Err(error) => query_failure(actor.actor().correlation_id().as_str(), error),
            }
        }
        RouteClass::MailboxBindingCollectionApi => {
            let result = list_mailboxes(&repository, &repository, actor.actor(), &page).await;
            match result {
                Ok(page) => Response::from_json(&MailboxListPageDto {
                    mailboxes: page
                        .items()
                        .iter()
                        .map(|mailbox| MailboxListItemDto {
                            binding_id: mailbox.binding_id().as_str().to_owned(),
                            provider: mailbox_provider(mailbox.provider()).to_owned(),
                            status: mailbox_status(mailbox.status()).to_owned(),
                            version: mailbox.version().value(),
                        })
                        .collect(),
                    next_cursor: page.next_cursor().map(|value| value.as_str().to_owned()),
                }),
                Err(error) => query_failure(actor.actor().correlation_id().as_str(), error),
            }
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

fn page_request(request: &Request) -> Result<QueryPageRequest, ()> {
    let url = request.url().map_err(|_| ())?;
    let mut limit = DEFAULT_PAGE_SIZE;
    let mut cursor = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "limit" => {
                limit = value.parse::<u16>().map_err(|_| ())?;
            }
            "cursor" => {
                if cursor.is_some() {
                    return Err(());
                }
                cursor = Some(QueryCursor::parse(value.into_owned()).map_err(|_| ())?);
            }
            _ => return Err(()),
        }
    }
    let limit = QueryPageSize::new(limit).map_err(|_| ())?;
    Ok(QueryPageRequest::new(limit, cursor))
}

fn query_failure(correlation_id: &str, error: QueryApplicationError) -> Result<Response> {
    match error {
        QueryApplicationError::InvalidInput => {
            problem(correlation_id, 400, "invalid_request", "Invalid Request")
        }
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

const fn profile_status(status: ProfileStatus) -> &'static str {
    match status {
        ProfileStatus::Draft => "DRAFT",
        ProfileStatus::Quarantined => "QUARANTINED",
        ProfileStatus::Ready => "READY",
        ProfileStatus::InUse => "IN_USE",
        ProfileStatus::DirtyLocal => "DIRTY_LOCAL",
        ProfileStatus::Syncing => "SYNCING",
        ProfileStatus::Suspended => "SUSPENDED",
        ProfileStatus::Deleting => "DELETING",
        ProfileStatus::Deleted => "DELETED",
    }
}

const fn membership_role(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::TenantOwner => "TENANT_OWNER",
        MembershipRole::Member => "MEMBER",
    }
}

const fn membership_status(status: MembershipStatus) -> &'static str {
    match status {
        MembershipStatus::Active => "ACTIVE",
        MembershipStatus::Suspended => "SUSPENDED",
        MembershipStatus::Revoked => "REVOKED",
    }
}

const fn mailbox_provider(provider: MailboxProvider) -> &'static str {
    provider.storage_value()
}

const fn mailbox_status(status: MailboxBindingStatus) -> &'static str {
    status.storage_value()
}

#[cfg(test)]
mod tests {
    use super::{
        mailbox_provider, mailbox_status, membership_role, membership_status, profile_status,
    };
    use application_ports::query_mailboxes::{MailboxBindingStatus, MailboxProvider};
    use application_ports::query_profiles::ProfileStatus;
    use identity_access_domain::{MembershipRole, MembershipStatus};

    #[test]
    fn public_query_enums_use_canonical_wire_values() {
        assert_eq!(profile_status(ProfileStatus::DirtyLocal), "DIRTY_LOCAL");
        assert_eq!(membership_role(MembershipRole::TenantOwner), "TENANT_OWNER");
        assert_eq!(membership_status(MembershipStatus::Revoked), "REVOKED");
        assert_eq!(
            mailbox_provider(MailboxProvider::BrowserFallback),
            "BROWSER_FALLBACK"
        );
        assert_eq!(
            mailbox_status(MailboxBindingStatus::AuthRequired),
            "AUTH_REQUIRED"
        );
    }
}
