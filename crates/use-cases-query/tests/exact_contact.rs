use application_ports::client_contact_lookup::ContactLookupProtectionPort;
use application_ports::clients::{
    ContactExactLookupRequest, ContactProtectionPortError,
};
use application_ports::query::{
    QueryAuthorizationPort, QueryCapability, QueryPageSize, QueryPortError,
};
use application_ports::query_clients::{
    ClientContactExactMatchProjection, ClientExactContactQueryPort,
};
use client_domain::{
    ContactKind, ContactNormalizationVersion, ExactLookupToken, LookupKeyVersion,
};
use profile_platform_primitives::{
    ActorContext, ActorId, ClientId, ContactPointId, CorrelationId, TenantId, TenantScope,
};
use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use use_cases_query::{QueryApplicationError, lookup_clients_by_exact_contact};

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
}

impl QueryAuthorizationPort for FakeAuthorization {
    async fn is_query_authorized(
        &self,
        _actor: &ActorContext,
        capability: QueryCapability,
    ) -> Result<bool, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(capability, QueryCapability::Clients);
        Ok(self.allowed)
    }
}

struct FakeProtector {
    calls: Cell<u32>,
}

impl ContactLookupProtectionPort for FakeProtector {
    async fn derive_exact_lookup_candidates(
        &self,
        request: ContactExactLookupRequest<'_>,
    ) -> Result<Vec<ExactLookupToken>, ContactProtectionPortError> {
        self.calls.set(self.calls.get() + 1);
        assert!(!request.hmac_input().expose_bytes().is_empty());
        Ok(vec![ExactLookupToken::new(
            [7_u8; 32],
            LookupKeyVersion::new(1).map_err(|_| {
                ContactProtectionPortError::new(
                    application_ports::clients::ContactProtectionPortErrorClass::InternalFailure,
                )
            })?,
        )])
    }
}

struct FakeProjection {
    calls: Cell<u32>,
}

impl ClientExactContactQueryPort for FakeProjection {
    async fn find_visible_clients_by_exact_contact(
        &self,
        _actor: &ActorContext,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        _token: &ExactLookupToken,
        limit: QueryPageSize,
    ) -> Result<Vec<ClientContactExactMatchProjection>, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(kind, ContactKind::Email);
        assert_eq!(normalization_version, ContactNormalizationVersion::V1);
        assert_eq!(limit.value(), 20);
        Ok(vec![ClientContactExactMatchProjection::new(
            ClientId::parse("client_01JCONTACTQUERY").map_err(|_| {
                QueryPortError::new(application_ports::query::QueryPortErrorClass::IntegrityFailure)
            })?,
            ContactPointId::parse("contact_01JCONTACTQUERY").map_err(|_| {
                QueryPortError::new(application_ports::query::QueryPortErrorClass::IntegrityFailure)
            })?,
        )])
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JCONTACTQUERY")?),
        ActorId::parse("actor_01JCONTACTQUERY")?,
        CorrelationId::parse("corr_01JCONTACTQUERY")?,
    ))
}

#[test]
fn denied_exact_contact_lookup_never_derives_hmac_or_queries_d1()
-> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: false,
        calls: Cell::new(0),
    };
    let protector = FakeProtector { calls: Cell::new(0) };
    let projection = FakeProjection { calls: Cell::new(0) };
    let result = block_on(lookup_clients_by_exact_contact(
        &actor()?,
        &authorization,
        &protector,
        &projection,
        ContactKind::Email,
        "alice@example.com",
    ))?;
    assert!(result.is_empty());
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(protector.calls.get(), 0);
    assert_eq!(projection.calls.get(), 0);
    Ok(())
}

#[test]
fn authorized_exact_contact_lookup_reuses_hmac_pipeline_and_is_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let protector = FakeProtector { calls: Cell::new(0) };
    let projection = FakeProjection { calls: Cell::new(0) };
    let result = block_on(lookup_clients_by_exact_contact(
        &actor()?,
        &authorization,
        &protector,
        &projection,
        ContactKind::Email,
        " Alice@Example.COM ",
    ))?;
    assert_eq!(result.len(), 1);
    assert_eq!(protector.calls.get(), 1);
    assert_eq!(projection.calls.get(), 1);
    Ok(())
}

#[test]
fn malformed_contact_is_stable_invalid_input_before_hmac_or_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let protector = FakeProtector { calls: Cell::new(0) };
    let projection = FakeProjection { calls: Cell::new(0) };
    assert_eq!(
        block_on(lookup_clients_by_exact_contact(
            &actor()?,
            &authorization,
            &protector,
            &projection,
            ContactKind::Email,
            "not-an-email",
        )),
        Err(QueryApplicationError::InvalidInput)
    );
    assert_eq!(protector.calls.get(), 0);
    assert_eq!(projection.calls.get(), 0);
    Ok(())
}
