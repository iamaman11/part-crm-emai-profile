use crate::browser_mail_query::{BrowserMailExecutionProof, BrowserMailRuntimePort};
use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
use application_ports::query_mail_provider::{
    MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use application_ports::{QueryPage, QueryPortError, QueryPortErrorClass};
use core::future::Future;
use profile_platform_primitives::{
    DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
    TenantScope,
};
use session_domain::{LeaseStatus, ProfileLease};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveProfileExecution {
    lease: ProfileLease,
    generation_id: GenerationId,
}

impl ActiveProfileExecution {
    pub fn new(lease: ProfileLease, generation_id: GenerationId) -> Result<Self, QueryPortError> {
        if lease.status() != LeaseStatus::Active {
            return Err(integrity_failure());
        }
        Ok(Self {
            lease,
            generation_id,
        })
    }

    #[must_use]
    pub const fn lease(&self) -> &ProfileLease {
        &self.lease
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    fn matches(&self, proof: &BrowserMailExecutionProof) -> bool {
        self.lease.status() == LeaseStatus::Active
            && proof.coordinator_lease() == &self.lease
            && proof.generation_id() == &self.generation_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMailAutomationContext {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    session_id: SessionId,
    device_id: DeviceId,
    coordinator_epoch: u64,
    coordinator_fencing_token: FencingToken,
    binding_id: MailboxBindingId,
    device_job_id: DeviceJobId,
    device_claim_id: DeviceClaimId,
    device_job_fence: u64,
}

impl BrowserMailAutomationContext {
    fn from_proof(proof: &BrowserMailExecutionProof) -> Self {
        let lease = proof.coordinator_lease();
        Self {
            tenant_id: lease.tenant_id().clone(),
            profile_id: lease.profile_id().clone(),
            generation_id: proof.generation_id().clone(),
            session_id: lease.session_id().clone(),
            device_id: lease.device_id().clone(),
            coordinator_epoch: lease.epoch(),
            coordinator_fencing_token: lease.fencing_token().clone(),
            binding_id: proof.binding_id().clone(),
            device_job_id: proof.device_job_id().clone(),
            device_claim_id: proof.device_claim_id().clone(),
            device_job_fence: proof.device_job_fence(),
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn coordinator_epoch(&self) -> u64 {
        self.coordinator_epoch
    }

    #[must_use]
    pub const fn coordinator_fencing_token(&self) -> &FencingToken {
        &self.coordinator_fencing_token
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
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
}

pub trait BrowserMailAutomationPort {
    fn search_messages(
        &self,
        context: &BrowserMailAutomationContext,
        request: &SearchClientMailboxMessagesRequest,
    ) -> impl Future<Output = Result<QueryPage<MailMessageSummary>, QueryPortError>>;

    fn get_message(
        &self,
        context: &BrowserMailAutomationContext,
        reference: &MailboxMessageReference,
    ) -> impl Future<Output = Result<Option<MailMessageBody>, QueryPortError>>;
}

pub struct BoundBrowserMailRuntime<A> {
    active_execution: ActiveProfileExecution,
    automation: A,
}

impl<A> BoundBrowserMailRuntime<A> {
    #[must_use]
    pub const fn new(active_execution: ActiveProfileExecution, automation: A) -> Self {
        Self {
            active_execution,
            automation,
        }
    }

    #[must_use]
    pub const fn active_execution(&self) -> &ActiveProfileExecution {
        &self.active_execution
    }

    #[must_use]
    pub const fn automation(&self) -> &A {
        &self.automation
    }

    fn require_exact_execution(
        &self,
        proof: &BrowserMailExecutionProof,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
    ) -> Result<(), QueryPortError> {
        if self.active_execution.matches(proof)
            && proof.coordinator_lease().tenant_id() == scope.tenant_id()
            && proof.binding_id() == binding_id
        {
            Ok(())
        } else {
            Err(integrity_failure())
        }
    }
}

impl<A> BrowserMailRuntimePort for BoundBrowserMailRuntime<A>
where
    A: BrowserMailAutomationPort,
{
    async fn search_messages(
        &self,
        proof: &BrowserMailExecutionProof,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        self.require_exact_execution(proof, scope, binding_id)?;
        let context = BrowserMailAutomationContext::from_proof(proof);
        self.automation.search_messages(&context, request).await
    }

    async fn get_message(
        &self,
        proof: &BrowserMailExecutionProof,
        scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        self.require_exact_execution(proof, scope, reference.binding_id())?;
        let context = BrowserMailAutomationContext::from_proof(proof);
        self.automation.get_message(&context, reference).await
    }
}

const fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveProfileExecution, BoundBrowserMailRuntime, BrowserMailAutomationContext,
        BrowserMailAutomationPort,
    };
    use crate::browser_mail_query::{BrowserMailExecutionProof, BrowserMailRuntimePort};
    use application_ports::browser_mail_execution::BrowserMailboxExecutionBinding;
    use application_ports::device_jobs::{DeviceClaimId, DeviceJobId};
    use application_ports::query_mail_provider::{
        MailMessageBody, MailMessageSummary, MailSearchTerm, MailboxMessageReference,
        SearchClientMailboxMessagesRequest,
    };
    use application_ports::{
        QueryPage, QueryPageRequest, QueryPageSize, QueryPortError, QueryPortErrorClass,
    };
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, MailboxBindingId, ProfileId, SessionId, TenantId,
        TenantScope,
    };
    use session_domain::ProfileLease;
    use std::cell::{Cell, RefCell};
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

    fn lease(session_id: &str) -> Result<ProfileLease, Box<dyn std::error::Error>> {
        Ok(ProfileLease::issue(
            TenantId::parse("tenant_01JBRMAILRUN")?,
            ProfileId::parse("profile_01JBRMAILRUN")?,
            SessionId::parse(session_id)?,
            DeviceId::parse("device_01JBRMAILRUN")?,
            4,
            FencingToken::parse("fence_01JBRMAILRUN")?,
        )?)
    }

    fn active_execution() -> Result<ActiveProfileExecution, Box<dyn std::error::Error>> {
        Ok(ActiveProfileExecution::new(
            lease("session_01JBRMAILRUN")?,
            GenerationId::parse("generation_01JBRMAILRUN")?,
        )?)
    }

    fn proof(
        lease: ProfileLease,
        generation_id: &str,
    ) -> Result<BrowserMailExecutionProof, Box<dyn std::error::Error>> {
        let profile_id = lease.profile_id().clone();
        Ok(BrowserMailExecutionProof::new(
            BrowserMailboxExecutionBinding::new(
                MailboxBindingId::parse("binding_01JBRMAILRUN")?,
                profile_id,
            ),
            GenerationId::parse(generation_id)?,
            DeviceJobId::parse("devjob_01JBRMAILRUN")?,
            DeviceClaimId::parse("devclaim_01JBRMAILRUN")?,
            7,
            lease,
        )?)
    }

    fn scope() -> Result<TenantScope, Box<dyn std::error::Error>> {
        Ok(TenantScope::new(TenantId::parse("tenant_01JBRMAILRUN")?))
    }

    fn search_request() -> Result<SearchClientMailboxMessagesRequest, Box<dyn std::error::Error>> {
        Ok(SearchClientMailboxMessagesRequest::new(
            Some(MailSearchTerm::parse("transient runtime search")?),
            QueryPageRequest::new(QueryPageSize::new(10)?, None),
        ))
    }

    #[derive(Default)]
    struct RecordingAutomation {
        search_calls: Cell<u32>,
        body_calls: Cell<u32>,
        last_context: RefCell<Option<BrowserMailAutomationContext>>,
    }

    impl BrowserMailAutomationPort for RecordingAutomation {
        async fn search_messages(
            &self,
            context: &BrowserMailAutomationContext,
            _request: &SearchClientMailboxMessagesRequest,
        ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
            self.search_calls.set(self.search_calls.get() + 1);
            self.last_context.replace(Some(context.clone()));
            Ok(QueryPage::empty())
        }

        async fn get_message(
            &self,
            context: &BrowserMailAutomationContext,
            _reference: &MailboxMessageReference,
        ) -> Result<Option<MailMessageBody>, QueryPortError> {
            self.body_calls.set(self.body_calls.get() + 1);
            self.last_context.replace(Some(context.clone()));
            Ok(None)
        }
    }

    #[test]
    fn exact_active_execution_delegates_with_fenced_context()
    -> Result<(), Box<dyn std::error::Error>> {
        let active = active_execution()?;
        let proof = proof(active.lease().clone(), active.generation_id().as_str())?;
        let binding_id = proof.binding_id().clone();
        let runtime = BoundBrowserMailRuntime::new(active, RecordingAutomation::default());

        block_on(runtime.search_messages(&proof, &scope()?, &binding_id, &search_request()?))?;
        assert_eq!(runtime.automation().search_calls.get(), 1);
        let context = runtime.automation().last_context.borrow();
        let Some(context) = context.as_ref() else {
            return Err(std::io::Error::other("automation context was not recorded").into());
        };
        assert_eq!(context.tenant_id(), proof.coordinator_lease().tenant_id());
        assert_eq!(context.profile_id(), proof.coordinator_lease().profile_id());
        assert_eq!(context.generation_id(), proof.generation_id());
        assert_eq!(context.session_id(), proof.coordinator_lease().session_id());
        assert_eq!(context.device_id(), proof.coordinator_lease().device_id());
        assert_eq!(
            context.coordinator_epoch(),
            proof.coordinator_lease().epoch()
        );
        assert_eq!(
            context.coordinator_fencing_token(),
            proof.coordinator_lease().fencing_token()
        );
        assert_eq!(context.binding_id(), proof.binding_id());
        assert_eq!(context.device_job_id(), proof.device_job_id());
        assert_eq!(context.device_claim_id(), proof.device_claim_id());
        assert_eq!(context.device_job_fence(), proof.device_job_fence());
        Ok(())
    }

    #[test]
    fn stale_session_is_rejected_before_automation() -> Result<(), Box<dyn std::error::Error>> {
        let active = active_execution()?;
        let proof = proof(
            lease("session_02JBRMAILRUN")?,
            active.generation_id().as_str(),
        )?;
        let binding_id = proof.binding_id().clone();
        let runtime = BoundBrowserMailRuntime::new(active, RecordingAutomation::default());
        let result =
            block_on(runtime.search_messages(&proof, &scope()?, &binding_id, &search_request()?));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(runtime.automation().search_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn generation_substitution_is_rejected_before_automation()
    -> Result<(), Box<dyn std::error::Error>> {
        let active = active_execution()?;
        let proof = proof(active.lease().clone(), "generation_02JBRMAILRUN")?;
        let binding_id = proof.binding_id().clone();
        let runtime = BoundBrowserMailRuntime::new(active, RecordingAutomation::default());
        let result =
            block_on(runtime.search_messages(&proof, &scope()?, &binding_id, &search_request()?));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(runtime.automation().search_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn binding_substitution_is_rejected_before_automation() -> Result<(), Box<dyn std::error::Error>>
    {
        let active = active_execution()?;
        let proof = proof(active.lease().clone(), active.generation_id().as_str())?;
        let runtime = BoundBrowserMailRuntime::new(active, RecordingAutomation::default());
        let foreign_binding = MailboxBindingId::parse("binding_02JBRMAILRUN")?;
        let result = block_on(runtime.search_messages(
            &proof,
            &scope()?,
            &foreign_binding,
            &search_request()?,
        ));
        assert_eq!(
            result.map(|_| ()),
            Err(QueryPortError::new(QueryPortErrorClass::IntegrityFailure))
        );
        assert_eq!(runtime.automation().search_calls.get(), 0);
        Ok(())
    }
}
