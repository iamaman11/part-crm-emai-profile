use application_ports::query::{
    QueryAuthorizationPort, QueryCapability, QueryPortError, QueryPortErrorClass,
};
use application_ports::query_global::{
    GlobalSearchKey, GlobalSearchProjection, GlobalSearchReadModelPort,
};
use profile_platform_primitives::{
    ActorContext, ActorId, ClientId, CorrelationId, TenantId, TenantScope,
};
use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use use_cases_query::{QueryApplicationError, search_global_exact};

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
        assert_eq!(capability, QueryCapability::GlobalSearch);
        match self.failure {
            Some(class) => Err(QueryPortError::new(class)),
            None => Ok(self.allowed),
        }
    }
}

struct FakeGlobalSearch {
    calls: Cell<u32>,
}

impl GlobalSearchReadModelPort for FakeGlobalSearch {
    async fn search_exact(
        &self,
        _actor: &ActorContext,
        _key: &GlobalSearchKey,
    ) -> Result<Option<GlobalSearchProjection>, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(None)
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JQUERY")?),
        ActorId::parse("actor_01JQUERY")?,
        CorrelationId::parse("corr_01JQUERY")?,
    ))
}

fn key() -> Result<GlobalSearchKey, Box<dyn std::error::Error>> {
    Ok(GlobalSearchKey::Client(ClientId::parse("client_01JQUERY")?))
}

#[test]
fn denied_global_search_is_neutral_and_never_projects() -> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: false,
        calls: Cell::new(0),
        failure: None,
    };
    let projection = FakeGlobalSearch {
        calls: Cell::new(0),
    };
    let result = block_on(search_global_exact(
        &actor()?,
        &authorization,
        &projection,
        &key()?,
    ))?;
    assert!(result.is_none());
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(projection.calls.get(), 0);
    Ok(())
}

#[test]
fn authorized_global_search_projects_once() -> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
        failure: None,
    };
    let projection = FakeGlobalSearch {
        calls: Cell::new(0),
    };
    let result = block_on(search_global_exact(
        &actor()?,
        &authorization,
        &projection,
        &key()?,
    ))?;
    assert!(result.is_none());
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(projection.calls.get(), 1);
    Ok(())
}

#[test]
fn global_authorization_failure_never_projects() -> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
        failure: Some(QueryPortErrorClass::DependencyUnavailable),
    };
    let projection = FakeGlobalSearch {
        calls: Cell::new(0),
    };
    assert_eq!(
        block_on(search_global_exact(
            &actor()?,
            &authorization,
            &projection,
            &key()?,
        )),
        Err(QueryApplicationError::DependencyUnavailable)
    );
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(projection.calls.get(), 0);
    Ok(())
}
