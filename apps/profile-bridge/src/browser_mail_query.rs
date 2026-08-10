use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use application_ports::{QueryPage, QueryPortError, QueryPortErrorClass};
use core::future::Future;
use profile_platform_primitives::{GenerationId, MailboxBindingId, TenantScope};
use session_domain::{LeaseStatus, ProfileLease};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailExecutionProof {
    execution_binding: BrowserMailboxExecutionBinding,
    generation_id: GenerationId,
    device_job_id: DeviceJobId,
    device_claim_id: DeviceClaimId,
    device_job_fence: u64,
    coordinator_lease: ProfileLease,
}

impl BrowserMailExecutionProof {
    pub fn new(
        execution_binding: BrowserMailboxExecutionBinding,
        generation_id: GenerationId,
        device_job_id: DeviceJobId,
        device_claim_id: DeviceClaimId,
        device_job_fence: u64,
        coordinator_lease: ProfileLease,
    ) -> Result<Self, BrowserMailExecutionProofError> {
        if device_job_fence == 0
            || coordinator_lease.status() != LeaseStatus::Active
            || coordinator_lease.profile_id() != execution_binding.profile_id()
        {
            return Err(BrowserMailExecutionProofError::InvalidExecutionProof);
        }
        Ok(Self {
            execution_binding,
            generation_id,
            device_job_id,
            device_claim_id,
            device_job_fence,
            coordinator_lease,
        })
    }

    #[must_use]
    pub const fn execution_binding(&self) -> &BrowserMailboxExecutionBinding {
        &self.execution_binding
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        self.execution_binding.binding_id()
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn device_job_id(&self) -> &DeviceJobId {
        &self.device_job_id
    }

    #[must_use]
    pub const fn device_claim_id(&self) -> &DeviceClaimId {
        &self.device_claim_id
    }

    #[must_use]
    pub const fn device_job_fence(&self) -> u64 {
        self.device_job_fence
    }

    #[must_use]
    pub const fn coordinator_lease(&self) -> &ProfileLease {
        &self.coordinator_lease
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserMailExecutionProofError {
    InvalidExecutionProof,
}

impl core::fmt::Display for BrowserMailExecutionProofError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("browser mail execution proof is invalid")
    }
}

impl std::error::Error for BrowserMailExecutionProofError {}

pub trait BrowserMailExecutionFencePort {
    fn is_execution_current(
        &self,
        proof: &BrowserMailExecutionProof,
    ) -> impl Future<Output = Result<bool, QueryPortError>>;
}

pub trait BrowserMailRuntimePort {
    fn search_messages(
        &self,
        proof: &BrowserMailExecutionProof,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        request: &SearchClientMailboxMessagesRequest,
    ) -> impl Future<Output = Result<QueryPage<MailMessageSummary>, QueryPortError>>;

    fn get_message(
        &self,
        proof: &BrowserMailExecutionProof,
        scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> impl Future<Output = Result<Option<MailMessageBody>, QueryPortError>>;
}

pub struct BrowserClientMailQueryAdapter<F, R> {
    proof: BrowserMailExecutionProof,
    fence: F,
    runtime: R,
}

impl<F, R> BrowserClientMailQueryAdapter<F, R> {
    #[must_use]
    pub const fn new(proof: BrowserMailExecutionProof, fence: F, runtime: R) -> Self {
        Self {
            proof,
            fence,
            runtime,
        }
    }

    #[must_use]
    pub const fn proof(&self) -> &BrowserMailExecutionProof {
        &self.proof
    }
}

impl<F, R> BrowserClientMailQueryAdapter<F, R>
where
    F: BrowserMailExecutionFencePort,
{
    async fn require_current_execution(&self) -> Result<(), QueryPortError> {
        if self.fence.is_execution_current(&self.proof).await? {
            Ok(())
        } else {
            Err(integrity_failure())
        }
    }

    fn require_bound_scope(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<(), QueryPortError> {
        if self.proof.coordinator_lease().tenant_id() == scope.tenant_id()
            && self.proof.binding_id() == binding_id
        {
            Ok(())
        } else {
            Err(integrity_failure())
        }
    }
}

impl<F, R> ClientMailProviderQueryPort for BrowserClientMailQueryAdapter<F, R>
where
    F: BrowserMailExecutionFencePort,
    R: BrowserMailRuntimePort,
{
    async fn search_messages(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        self.require_bound_scope(scope, binding_id)?;
        self.require_current_execution().await?;
        let page = self
            .runtime
            .search_messages(&self.proof, scope, binding_id, request)
            .await?;
        if page.items().len() > usize::from(request.page().limit().value())
            || page
                .items()
                .iter()
                .any(|item| item.reference().binding_id() != binding_id)
        {
            return Err(integrity_failure());
        }
        self.require_current_execution().await?;
        Ok(page)
    }

    async fn get_message(
        &self,
        scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        self.require_bound_scope(scope, reference.binding_id())?;
        self.require_current_execution().await?;
        let body = self
            .runtime
            .get_message(&self.proof, scope, reference)
            .await?;
        if body.as_ref().is_some_and(|message| {
            message.summary().reference().binding_id() != self.proof.binding_id()
        }) {
            return Err(integrity_failure());
        }
        self.require_current_execution().await?;
        Ok(body)
    }
}

const fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserClientMailQueryAdapter, BrowserMailExecutionFencePort, BrowserMailExecutionProof,
        BrowserMailExecutionProofError, BrowserMailRuntimePort,
    };
    use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
    use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
    use application_ports::query_mail_provider::{
        ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailSearchTerm,
        MailboxMessageReference, SearchClientMailboxMessagesRequest,
    };
    use application_ports::{
        QueryPage, QueryPageRequest, QueryPageSize, QueryPortError, QueryPortErrorClass,
    };
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
        TenantScope, UnixMillis,
    };
    use session_domain::ProfileLease;
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

    fn proof() -> Result<BrowserMailExecutionProof, Box<dyn std::error::Error>> {
        let profile_id = ProfileId::parse("profile_01JBRMAIL")?;
        Ok(BrowserMailExecutionProof::new(
            BrowserMailboxExecutionBinding::new(
                MailboxBindingId::parse("binding_01JBRMAIL")?,
                profile_id.clone(),
            ),
            GenerationId::parse("generation_01JBRMAIL")?,
            DeviceJobId::parse("devjob_01JBRMAIL")?,
            DeviceClaimId::parse("devclaim_01JBRMAIL")?,
            7,
            ProfileLease::issue(
                TenantId::parse("tenant_01JBRMAIL")?,
                profile_id,
                SessionId::parse("session_01JBRMAIL")?,
                DeviceId::parse("device_01JBRMAIL")?,
                4,
                FencingToken::parse("fence_01JBRMAIL")?,
            )?,
        )?)
    }

    fn scope() -> Result<TenantScope, Box<dyn std::error::Error>> {
        Ok(TenantScope::new(TenantId::parse("tenant_01JBRMAIL")?))
    }

    fn search_request() -> Result<SearchClientMailboxMessagesRequest, Box<dyn std::error::Error>> {
        Ok(SearchClientMailboxMessagesRequest::new(
            Some(MailSearchTerm::parse("transient search term")?),
            QueryPageRequest::new(QueryPageSize::new(10)?, None),
        ))
    }

    struct CountingFence {
        calls: Cell<u32>,
        valid_checks: u32,
    }

    impl BrowserMailExecutionFencePort for CountingFence {
        async fn is_execution_current(
            &self,
            _proof: &BrowserMailExecutionProof,
        ) -> Result<bool, QueryPortError> {
            let next = self.calls.get() + 1;
            self.calls.set(next);
            Ok(next <= self.valid_checks)
        }
    }

    struct SyntheticBrowserRuntime {
        search_calls: Cell<u32>,
        body_calls: Cell<u32>,
        returned_binding: MailboxBindingId,
    }

    impl BrowserMailRuntimePort for SyntheticBrowserRuntime {
        async fn search_messages(
            &self,
            _proof: &BrowserMailExecutionProof,
            _scope: &TenantScope,
            _binding_id: &MailboxBindingId,
            _request: &SearchClientMailboxMessagesRequest,
        ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
            self.search_calls.set(self.search_calls.get() + 1);
            let reference = MailboxMessageReference::new(
                self.returned_binding.clone(),
                "browser-provider-message-1",
            )
            .map_err(|_| QueryPortError::new(QueryPortErrorClass::IntegrityFailure))?;
            Ok(QueryPage::new(
                vec![MailMessageSummary::new(
                    reference,
                    Some("Transient subject".to_owned()),
                    Some("sender@example.test".to_owned()),
                    UnixMillis::new(100),
                )],
                None,
            ))
        }

        async fn get_message(
            &self,
            _proof: &BrowserMailExecutionProof,
            _scope: &TenantScope,
            reference: &MailboxMessageReference,
        ) -> Result<Option<MailMessageBody>, QueryPortError> {
            self.body_calls.set(self.body_calls.get() + 1);
            let summary = MailMessageSummary::new(
                MailboxMessageReference::new(
                    self.returned_binding.clone(),
                    reference.provider_reference(),
                )
                .map_err(|_| QueryPortError::new(QueryPortErrorClass::IntegrityFailure))?,
                Some("Transient subject".to_owned()),
                Some("sender@example.test".to_owned()),
                UnixMillis::new(100),
            );
            Ok(Some(
                MailMessageBody::new(summary, Some("transient body".to_owned()), None)
                    .map_err(|_| QueryPortError::new(QueryPortErrorClass::IntegrityFailure))?,
            ))
        }
    }

    fn runtime(binding_id: MailboxBindingId) -> SyntheticBrowserRuntime {
        SyntheticBrowserRuntime {
            search_calls: Cell::new(0),
            body_calls: Cell::new(0),
            returned_binding: binding_id,
        }
    }

    #[test]
    fn execution_binding_profile_must_match_coordinator_lease()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = BrowserMailExecutionProof::new(
            BrowserMailboxExecutionBinding::new(
                MailboxBindingId::parse("binding_01JBRMAIL")?,
                ProfileId::parse("profile_02JBRMAIL")?,
            ),
            GenerationId::parse("generation_01JBRMAIL")?,
            DeviceJobId::parse("devjob_01JBRMAIL")?,
            DeviceClaimId::parse("devclaim_01JBRMAIL")?,
            7,
            ProfileLease::issue(
                TenantId::parse("tenant_01JBRMAIL")?,
                ProfileId::parse("profile_01JBRMAIL")?,
                SessionId::parse("session_01JBRMAIL")?,
                DeviceId::parse("device_01JBRMAIL")?,
                4,
                FencingToken::parse("fence_01JBRMAIL")?,
            )?,
        );
        assert_eq!(
            result.map(|_| ()),
            Err(BrowserMailExecutionProofError::InvalidExecutionProof)
        );
        Ok(())
    }

    #[test]
    fn browser_mail_search_is_fenced_before_and_after_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let proof = proof()?;
        let binding_id = proof.binding_id().clone();
        let adapter = BrowserClientMailQueryAdapter::new(
            proof,
            CountingFence {
                calls: Cell::new(0),
                valid_checks: 2,
            },
            runtime(binding_id.clone()),
        );
        let page = block_on(adapter.search_messages(&scope()?, &binding_id, &search_request()?))?;
        assert_eq!(page.items().len(), 1);
        assert_eq!(adapter.fence.calls.get(), 2);
        assert_eq!(adapter.runtime.search_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn stale_post_runtime_fence_discards_search_result() -> Result<(), Box<dyn std::error::Error>> {
        let proof = proof()?;
        let binding_id = proof.binding_id().clone();
        let adapter = BrowserClientMailQueryAdapter::new(
            proof,
            CountingFence {
                calls: Cell::new(0),
                valid_checks: 1,
            },
            runtime(binding_id.clone()),
        );
        let result = block_on(adapter.search_messages(&scope()?, &binding_id, &search_request()?));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(adapter.runtime.search_calls.get(), 1);
        assert_eq!(adapter.fence.calls.get(), 2);
        Ok(())
    }

    #[test]
    fn stale_post_runtime_fence_discards_message_body() -> Result<(), Box<dyn std::error::Error>> {
        let proof = proof()?;
        let binding_id = proof.binding_id().clone();
        let reference = MailboxMessageReference::new(binding_id.clone(), "browser-message-1")?;
        let adapter = BrowserClientMailQueryAdapter::new(
            proof,
            CountingFence {
                calls: Cell::new(0),
                valid_checks: 1,
            },
            runtime(binding_id),
        );
        let result = block_on(adapter.get_message(&scope()?, &reference));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(adapter.runtime.body_calls.get(), 1);
        assert_eq!(adapter.fence.calls.get(), 2);
        Ok(())
    }

    #[test]
    fn binding_substitution_is_rejected_before_runtime() -> Result<(), Box<dyn std::error::Error>> {
        let proof = proof()?;
        let correct_binding = proof.binding_id().clone();
        let adapter = BrowserClientMailQueryAdapter::new(
            proof,
            CountingFence {
                calls: Cell::new(0),
                valid_checks: 2,
            },
            runtime(correct_binding),
        );
        let foreign = MailboxBindingId::parse("binding_02JBRMAIL")?;
        let result = block_on(adapter.search_messages(&scope()?, &foreign, &search_request()?));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(adapter.runtime.search_calls.get(), 0);
        assert_eq!(adapter.fence.calls.get(), 0);
        Ok(())
    }

    #[test]
    fn provider_binding_substitution_is_rejected_before_return()
    -> Result<(), Box<dyn std::error::Error>> {
        let proof = proof()?;
        let binding_id = proof.binding_id().clone();
        let adapter = BrowserClientMailQueryAdapter::new(
            proof,
            CountingFence {
                calls: Cell::new(0),
                valid_checks: 2,
            },
            runtime(MailboxBindingId::parse("binding_02JBRMAIL")?),
        );
        let result = block_on(adapter.search_messages(&scope()?, &binding_id, &search_request()?));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(adapter.runtime.search_calls.get(), 1);
        assert_eq!(adapter.fence.calls.get(), 1);
        Ok(())
    }
}
