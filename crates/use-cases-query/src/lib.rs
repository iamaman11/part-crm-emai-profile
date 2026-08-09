#![forbid(unsafe_code)]

use application_ports::query::{
    QueryAuthorizationPort, QueryCapability, QueryPage, QueryPageRequest, QueryPortError,
    QueryPortErrorClass,
};
use application_ports::query_clients::{ClientReadModelPort, ClientReadProjection};
use application_ports::query_mail::{ClientMailEligibilityProjection, MailReadModelPort};
use application_ports::query_mailboxes::{MailboxReadModelPort, MailboxReadProjection};
use application_ports::query_members::{MemberReadModelPort, MemberReadProjection};
use application_ports::query_profiles::{ProfileReadModelPort, ProfileReadProjection};
use core::fmt;
use profile_platform_primitives::{ActorContext, ClientId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryApplicationError {
    IntegrityFailure,
    DependencyUnavailable,
}

impl fmt::Display for QueryApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IntegrityFailure => "query application integrity failure",
            Self::DependencyUnavailable => "query application dependency unavailable",
        })
    }
}

impl std::error::Error for QueryApplicationError {}

pub async fn list_clients<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    page: &QueryPageRequest,
) -> Result<QueryPage<ClientReadProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: ClientReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::Clients).await? {
        return Ok(QueryPage::empty());
    }
    projection
        .list_clients(actor, page)
        .await
        .map_err(map_port_error)
}

pub async fn list_profiles<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    page: &QueryPageRequest,
) -> Result<QueryPage<ProfileReadProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: ProfileReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::Profiles).await? {
        return Ok(QueryPage::empty());
    }
    projection
        .list_profiles(actor, page)
        .await
        .map_err(map_port_error)
}

pub async fn list_members<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    page: &QueryPageRequest,
) -> Result<QueryPage<MemberReadProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: MemberReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::Members).await? {
        return Ok(QueryPage::empty());
    }
    projection
        .list_members(actor, page)
        .await
        .map_err(map_port_error)
}

pub async fn list_mailboxes<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    page: &QueryPageRequest,
) -> Result<QueryPage<MailboxReadProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: MailboxReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::Mailboxes).await? {
        return Ok(QueryPage::empty());
    }
    projection
        .list_mailboxes(actor, page)
        .await
        .map_err(map_port_error)
}

pub async fn list_client_mail_eligibility<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    client_id: &ClientId,
    page: &QueryPageRequest,
) -> Result<QueryPage<ClientMailEligibilityProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: MailReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::Mail).await? {
        return Ok(QueryPage::empty());
    }
    projection
        .list_eligible_mailboxes_for_client(actor, client_id, page)
        .await
        .map_err(map_port_error)
}

async fn authorize<A: QueryAuthorizationPort>(
    actor: &ActorContext,
    authorization: &A,
    capability: QueryCapability,
) -> Result<bool, QueryApplicationError> {
    authorization
        .is_query_authorized(actor, capability)
        .await
        .map_err(map_port_error)
}

fn map_port_error(error: QueryPortError) -> QueryApplicationError {
    match error.class() {
        QueryPortErrorClass::IntegrityFailure => QueryApplicationError::IntegrityFailure,
        QueryPortErrorClass::DependencyUnavailable => QueryApplicationError::DependencyUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{QueryApplicationError, list_clients};
    use application_ports::query::{
        QueryAuthorizationPort, QueryCapability, QueryPage, QueryPageRequest, QueryPageSize,
        QueryPortError, QueryPortErrorClass,
    };
    use application_ports::query_clients::{ClientReadModelPort, ClientReadProjection};
    use client_domain::{ClientKind, ClientStatus};
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, ClientId, CorrelationId, TenantId, TenantScope,
    };
    use std::cell::Cell;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakeAuthorization {
        allowed: bool,
        calls: Cell<u32>,
        failure: Option<QueryPortErrorClass>,
    }

    impl QueryAuthorizationPort for FakeAuthorization {
        async fn is_query_authorized(
            &self,
            _actor: &ActorContext,
            capability: QueryCapability,
        ) -> Result<bool, QueryPortError> {
            self.calls.set(self.calls.get() + 1);
            assert_eq!(capability, QueryCapability::Clients);
            match self.failure {
                Some(class) => Err(QueryPortError::new(class)),
                None => Ok(self.allowed),
            }
        }
    }

    struct FakeClients {
        calls: Cell<u32>,
    }

    impl ClientReadModelPort for FakeClients {
        async fn list_clients(
            &self,
            _actor: &ActorContext,
            _page: &QueryPageRequest,
        ) -> Result<QueryPage<ClientReadProjection>, QueryPortError> {
            self.calls.set(self.calls.get() + 1);
            let client_id = ClientId::parse("client_01JQUERY")
                .map_err(|_| QueryPortError::new(QueryPortErrorClass::IntegrityFailure))?;
            Ok(QueryPage::new(
                vec![ClientReadProjection::new(
                    client_id,
                    ClientKind::Person,
                    "Visible Client",
                    ClientStatus::Active,
                    AggregateVersion::INITIAL,
                )],
                None,
            ))
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JQUERY")?),
            ActorId::parse("actor_01JQUERY")?,
            CorrelationId::parse("corr_01JQUERY")?,
        ))
    }

    fn page() -> Result<QueryPageRequest, Box<dyn std::error::Error>> {
        Ok(QueryPageRequest::new(QueryPageSize::new(25)?, None))
    }

    #[test]
    fn denied_query_is_neutral_and_never_touches_projection() -> Result<(), Box<dyn std::error::Error>> {
        let authorization = FakeAuthorization { allowed: false, calls: Cell::new(0), failure: None };
        let projection = FakeClients { calls: Cell::new(0) };
        let result = block_on(list_clients(&actor()?, &authorization, &projection, &page()?))?;
        assert!(result.items().is_empty());
        assert_eq!(authorization.calls.get(), 1);
        assert_eq!(projection.calls.get(), 0);
        Ok(())
    }

    #[test]
    fn authorized_query_projects_only_after_live_check() -> Result<(), Box<dyn std::error::Error>> {
        let authorization = FakeAuthorization { allowed: true, calls: Cell::new(0), failure: None };
        let projection = FakeClients { calls: Cell::new(0) };
        let result = block_on(list_clients(&actor()?, &authorization, &projection, &page()?))?;
        assert_eq!(result.items().len(), 1);
        assert_eq!(authorization.calls.get(), 1);
        assert_eq!(projection.calls.get(), 1);
        Ok(())
    }

    #[test]
    fn authorization_failure_never_touches_projection() -> Result<(), Box<dyn std::error::Error>> {
        let authorization = FakeAuthorization {
            allowed: true,
            calls: Cell::new(0),
            failure: Some(QueryPortErrorClass::DependencyUnavailable),
        };
        let projection = FakeClients { calls: Cell::new(0) };
        assert_eq!(
            block_on(list_clients(&actor()?, &authorization, &projection, &page()?)),
            Err(QueryApplicationError::DependencyUnavailable)
        );
        assert_eq!(projection.calls.get(), 0);
        Ok(())
    }
}
