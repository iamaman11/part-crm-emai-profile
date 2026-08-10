use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailSearchTerm,
    MailboxMessageReference, SearchClientMailboxMessagesRequest,
};
use application_ports::{
    QueryPage, QueryPageRequest, QueryPageSize, QueryPortError, QueryPortErrorClass,
};
use profile_bridge::browser_mail_query::{
    BrowserClientMailQueryAdapter, BrowserMailExecutionFencePort, BrowserMailExecutionProof,
};
use profile_bridge::browser_mail_runtime::{
    ActiveProfileExecution, BoundBrowserMailRuntime, BrowserMailAutomationContext,
    BrowserMailAutomationPort,
};
use profile_platform_primitives::{
    DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
    TenantScope, UnixMillis,
};
use session_domain::ProfileLease;
use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

const RUNTIME_SOURCE: &str = include_str!("../src/browser_mail_runtime.rs");

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

fn active_execution() -> Result<ActiveProfileExecution, Box<dyn std::error::Error>> {
    Ok(ActiveProfileExecution::new(
        ProfileLease::issue(
            TenantId::parse("tenant_01JBRMAILACC")?,
            ProfileId::parse("profile_01JBRMAILACC")?,
            SessionId::parse("session_01JBRMAILACC")?,
            DeviceId::parse("device_01JBRMAILACC")?,
            8,
            FencingToken::parse("fence_01JBRMAILACC")?,
        )?,
        GenerationId::parse("generation_01JBRMAILACC")?,
    )?)
}

fn proof(
    active: &ActiveProfileExecution,
) -> Result<BrowserMailExecutionProof, Box<dyn std::error::Error>> {
    Ok(BrowserMailExecutionProof::new(
        BrowserMailboxExecutionBinding::new(
            MailboxBindingId::parse("binding_01JBRMAILACC")?,
            active.lease().profile_id().clone(),
        ),
        active.generation_id().clone(),
        DeviceJobId::parse("devjob_01JBRMAILACC")?,
        DeviceClaimId::parse("devclaim_01JBRMAILACC")?,
        11,
        active.lease().clone(),
    )?)
}

struct CurrentFence {
    calls: Rc<Cell<u32>>,
}

impl BrowserMailExecutionFencePort for CurrentFence {
    async fn is_execution_current(
        &self,
        _proof: &BrowserMailExecutionProof,
    ) -> Result<bool, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(true)
    }
}

struct DeterministicAutomation {
    search_calls: Rc<Cell<u32>>,
}

impl BrowserMailAutomationPort for DeterministicAutomation {
    async fn search_messages(
        &self,
        context: &BrowserMailAutomationContext,
        _request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        self.search_calls.set(self.search_calls.get() + 1);
        let reference =
            MailboxMessageReference::new(context.binding_id().clone(), "browser-runtime-message-1")
                .map_err(|_| QueryPortError::new(QueryPortErrorClass::IntegrityFailure))?;
        Ok(QueryPage::new(
            vec![MailMessageSummary::new(
                reference,
                Some("Transient runtime subject".to_owned()),
                Some("runtime@example.test".to_owned()),
                UnixMillis::new(100),
            )],
            None,
        ))
    }

    async fn get_message(
        &self,
        _context: &BrowserMailAutomationContext,
        _reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        Ok(None)
    }
}

#[test]
fn accepted_phase2d_provider_contract_executes_through_bound_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let active = active_execution()?;
    let proof = proof(&active)?;
    let binding_id = proof.binding_id().clone();
    let fence_calls = Rc::new(Cell::new(0));
    let search_calls = Rc::new(Cell::new(0));
    let adapter = BrowserClientMailQueryAdapter::new(
        proof,
        CurrentFence {
            calls: Rc::clone(&fence_calls),
        },
        BoundBrowserMailRuntime::new(
            active,
            DeterministicAutomation {
                search_calls: Rc::clone(&search_calls),
            },
        ),
    );
    let request = SearchClientMailboxMessagesRequest::new(
        Some(MailSearchTerm::parse("runtime acceptance")?),
        QueryPageRequest::new(QueryPageSize::new(10)?, None),
    );
    let scope = TenantScope::new(TenantId::parse("tenant_01JBRMAILACC")?);

    let page = block_on(adapter.search_messages(&scope, &binding_id, &request))?;
    assert_eq!(page.items().len(), 1);
    assert_eq!(fence_calls.get(), 2);
    assert_eq!(search_calls.get(), 1);
    assert_eq!(page.items()[0].reference().binding_id(), &binding_id);
    Ok(())
}

fn runtime_policy_errors(source: &str) -> Vec<String> {
    let production = source.split("#[cfg(test)]").next().unwrap_or(source);
    let mut errors = Vec::new();
    for required in [
        "pub struct ActiveProfileExecution",
        "lease.status() != LeaseStatus::Active",
        "proof.coordinator_lease() == &self.lease",
        "proof.generation_id() == &self.generation_id",
        "pub struct BrowserMailAutomationContext",
        "pub trait BrowserMailAutomationPort",
        "pub struct BoundBrowserMailRuntime",
        "self.require_exact_execution(proof, scope, binding_id)?;",
        "self.require_exact_execution(proof, scope, reference.binding_id())?;",
    ] {
        if !production.contains(required) {
            errors.push(format!("missing browser runtime invariant: {required}"));
        }
    }
    for forbidden in ["std::fs", "bridge_outbox", "mailbox_job_run_commands"] {
        if production.contains(forbidden) {
            errors.push(format!(
                "browser runtime must remain transient: {forbidden}"
            ));
        }
    }
    errors
}

#[test]
fn production_browser_runtime_policy_is_storage_free_and_exact_bound() {
    let errors = runtime_policy_errors(RUNTIME_SOURCE);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn storage_leakage_negative_fixture_is_rejected() {
    let mut fixture = RUNTIME_SOURCE
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(RUNTIME_SOURCE)
        .to_owned();
    fixture.push_str("\nuse std::fs;\n");
    let errors = runtime_policy_errors(&fixture);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("browser runtime must remain transient: std::fs"))
    );
}
