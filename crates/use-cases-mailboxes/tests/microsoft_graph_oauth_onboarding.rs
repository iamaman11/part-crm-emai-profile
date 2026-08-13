use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::{
    MailboxOnboardingApplicationPort, MailboxOnboardingContext, MailboxOnboardingPortError,
    MailboxOnboardingPortErrorClass, MailboxOnboardingReplayDecision, MailboxOnboardingWrite,
};
use application_ports::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthAuthorizationCode, MicrosoftGraphOAuthAuthorizationUrl,
    MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthCeremonyId, MicrosoftGraphOAuthInputError,
    MicrosoftGraphOAuthProvisioningError, MicrosoftGraphOAuthProvisioningPort,
    MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthState,
};
use identity_access_domain::MembershipRole;
use mailbox_domain::{
    MailboxOnboarding, MailboxOnboardingStatus, MailboxOnboardingVersion, MailboxProvider,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AuditEventId, CorrelationId, IdempotencyKey, MailboxOnboardingId,
    OutboxEventId, SecretHandle, TenantId, TenantScope, UnixMillis,
};
use std::cell::{Cell, RefCell};
use std::future::{Future, ready};
use std::task::{Context, Poll, Waker};
use use_cases_mailboxes::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthOnboardingError, complete_microsoft_graph_oauth_callback,
    deny_microsoft_graph_oauth_callback, start_microsoft_graph_oauth_onboarding,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct FakeOnboardingPort {
    context: RefCell<Option<MailboxOnboardingContext>>,
    commit_error: Cell<Option<MailboxOnboardingPortErrorClass>>,
    commit_attempts: RefCell<Vec<MailboxOnboardingWrite>>,
}

impl FakeOnboardingPort {
    fn new(onboarding: MailboxOnboarding) -> Self {
        Self {
            context: RefCell::new(Some(MailboxOnboardingContext::new(onboarding))),
            commit_error: Cell::new(None),
            commit_attempts: RefCell::new(Vec::new()),
        }
    }

    fn fail_commit_with(&self, class: MailboxOnboardingPortErrorClass) {
        self.commit_error.set(Some(class));
    }
}

impl MailboxOnboardingApplicationPort for FakeOnboardingPort {
    fn decide_replay(
        &self,
        _actor: &ActorContext,
        _command_name: &str,
        _evidence: &CommandExecutionEvidence,
    ) -> impl Future<Output = Result<MailboxOnboardingReplayDecision, MailboxOnboardingPortError>>
    {
        ready(Ok(MailboxOnboardingReplayDecision::Miss))
    }

    fn load_context(
        &self,
        _scope: &TenantScope,
        _onboarding_id: &MailboxOnboardingId,
    ) -> impl Future<Output = Result<Option<MailboxOnboardingContext>, MailboxOnboardingPortError>>
    {
        ready(Ok(self.context.borrow().clone()))
    }

    fn commit(
        &self,
        _actor: &ActorContext,
        write: &MailboxOnboardingWrite,
    ) -> impl Future<Output = Result<(), MailboxOnboardingPortError>> {
        self.commit_attempts.borrow_mut().push(write.clone());
        ready(match self.commit_error.get() {
            Some(class) => Err(MailboxOnboardingPortError::new(class)),
            None => Ok(()),
        })
    }
}

struct FakeProvisioningPort {
    receipt: MicrosoftGraphOAuthStartReceipt,
    target: MicrosoftGraphOAuthCallbackTarget,
    completion_handle: SecretHandle,
    start_calls: Cell<usize>,
    complete_calls: Cell<usize>,
    deny_calls: Cell<usize>,
    discarded_handles: RefCell<Vec<SecretHandle>>,
}

impl FakeProvisioningPort {
    fn new(target: MicrosoftGraphOAuthCallbackTarget) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            receipt: MicrosoftGraphOAuthStartReceipt::new(
                oauth_input(
                    MicrosoftGraphOAuthCeremonyId::parse("ceremony_C3G_behavior"),
                    "invalid ceremony id",
                )?,
                oauth_input(
                    MicrosoftGraphOAuthAuthorizationUrl::parse(
                        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?state=opaque",
                    ),
                    "invalid authorization url",
                )?,
                UnixMillis::new(10_000),
            ),
            target,
            completion_handle: SecretHandle::parse("secret_C3G_new")?,
            start_calls: Cell::new(0),
            complete_calls: Cell::new(0),
            deny_calls: Cell::new(0),
            discarded_handles: RefCell::new(Vec::new()),
        })
    }
}

impl MicrosoftGraphOAuthProvisioningPort for FakeProvisioningPort {
    fn start(
        &self,
        _actor: &ActorContext,
        _onboarding_id: &MailboxOnboardingId,
        _expected_version: MailboxOnboardingVersion,
    ) -> impl Future<Output = Result<MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthProvisioningError>>
    {
        self.start_calls.set(self.start_calls.get() + 1);
        ready(Ok(self.receipt.clone()))
    }

    fn inspect(
        &self,
        _state: &MicrosoftGraphOAuthState,
    ) -> impl Future<Output = Result<MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthProvisioningError>>
    {
        ready(Ok(self.target.clone()))
    }

    fn complete(
        &self,
        _actor: &ActorContext,
        _state: &MicrosoftGraphOAuthState,
        _authorization_code: MicrosoftGraphOAuthAuthorizationCode,
    ) -> impl Future<Output = Result<SecretHandle, MicrosoftGraphOAuthProvisioningError>> {
        self.complete_calls.set(self.complete_calls.get() + 1);
        ready(Ok(self.completion_handle.clone()))
    }

    fn deny(
        &self,
        _actor: &ActorContext,
        _state: &MicrosoftGraphOAuthState,
    ) -> impl Future<Output = Result<(), MicrosoftGraphOAuthProvisioningError>> {
        self.deny_calls.set(self.deny_calls.get() + 1);
        ready(Ok(()))
    }

    fn discard(
        &self,
        _actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> impl Future<Output = Result<(), MicrosoftGraphOAuthProvisioningError>> {
        self.discarded_handles
            .borrow_mut()
            .push(secret_handle.clone());
        ready(Ok(()))
    }
}

#[test]
fn pending_and_reauth_start_are_owner_only_and_versioned() -> TestResult {
    let actor = actor("tenant_C3G_owner", "actor_C3G_owner")?;

    let pending_id = MailboxOnboardingId::parse("onboarding_C3G_pending")?;
    let pending_port = FakeOnboardingPort::new(onboarding(
        &actor,
        pending_id.clone(),
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let pending_provisioning = FakeProvisioningPort::new(callback_target(
        &actor,
        pending_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    let pending = block_on(start_microsoft_graph_oauth_onboarding(
        &actor,
        MembershipRole::TenantOwner,
        &pending_port,
        &pending_provisioning,
        pending_id,
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        pending.expected_version(),
        MailboxOnboardingVersion::INITIAL
    );
    assert_eq!(pending_provisioning.start_calls.get(), 1);

    let reauth_id = MailboxOnboardingId::parse("onboarding_C3G_reauth")?;
    let reauth_version = MailboxOnboardingVersion::new(3);
    let reauth_port = FakeOnboardingPort::new(onboarding(
        &actor,
        reauth_id.clone(),
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::ReauthRequired,
        reauth_version,
    )?);
    let reauth_provisioning =
        FakeProvisioningPort::new(callback_target(&actor, reauth_id.clone(), reauth_version))?;
    let reauth = block_on(start_microsoft_graph_oauth_onboarding(
        &actor,
        MembershipRole::TenantOwner,
        &reauth_port,
        &reauth_provisioning,
        reauth_id,
        reauth_version,
    ))?;
    assert_eq!(reauth.expected_version(), reauth_version);
    assert_eq!(reauth_provisioning.start_calls.get(), 1);

    let denied_id = MailboxOnboardingId::parse("onboarding_C3G_member")?;
    let denied_port = FakeOnboardingPort::new(onboarding(
        &actor,
        denied_id.clone(),
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let denied_provisioning = FakeProvisioningPort::new(callback_target(
        &actor,
        denied_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_graph_oauth_onboarding(
            &actor,
            MembershipRole::Member,
            &denied_port,
            &denied_provisioning,
            denied_id,
            MailboxOnboardingVersion::INITIAL,
        )),
        Err(MicrosoftGraphOAuthOnboardingError::NotFound)
    );
    assert_eq!(denied_provisioning.start_calls.get(), 0);

    let stale_id = MailboxOnboardingId::parse("onboarding_C3G_stale")?;
    let stale_port = FakeOnboardingPort::new(onboarding(
        &actor,
        stale_id.clone(),
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let stale_provisioning = FakeProvisioningPort::new(callback_target(
        &actor,
        stale_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_graph_oauth_onboarding(
            &actor,
            MembershipRole::TenantOwner,
            &stale_port,
            &stale_provisioning,
            stale_id,
            MailboxOnboardingVersion::new(2),
        )),
        Err(MicrosoftGraphOAuthOnboardingError::VersionConflict)
    );
    assert_eq!(stale_provisioning.start_calls.get(), 0);
    Ok(())
}

#[test]
fn invalid_provider_and_terminal_or_active_states_never_touch_resolver() -> TestResult {
    let actor = actor("tenant_C3G_invalid", "actor_C3G_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3G_invalid")?;

    let imap_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::Imap,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let imap_provisioning = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_graph_oauth_onboarding(
            &actor,
            MembershipRole::TenantOwner,
            &imap_port,
            &imap_provisioning,
            onboarding_id.clone(),
            MailboxOnboardingVersion::INITIAL,
        )),
        Err(MicrosoftGraphOAuthOnboardingError::InvalidState)
    );
    assert_eq!(imap_provisioning.start_calls.get(), 0);

    for status in [
        MailboxOnboardingStatus::Active,
        MailboxOnboardingStatus::Disabled,
        MailboxOnboardingStatus::ConfigError,
    ] {
        let version = MailboxOnboardingVersion::new(2);
        let port = FakeOnboardingPort::new(onboarding(
            &actor,
            onboarding_id.clone(),
            MailboxProvider::MicrosoftGraph,
            status,
            version,
        )?);
        let provisioning =
            FakeProvisioningPort::new(callback_target(&actor, onboarding_id.clone(), version))?;
        assert_eq!(
            block_on(start_microsoft_graph_oauth_onboarding(
                &actor,
                MembershipRole::TenantOwner,
                &port,
                &provisioning,
                onboarding_id.clone(),
                version,
            )),
            Err(MicrosoftGraphOAuthOnboardingError::InvalidState)
        );
        assert_eq!(provisioning.start_calls.get(), 0);
    }
    Ok(())
}

#[test]
fn callback_is_bound_to_exact_tenant_and_starter_actor() -> TestResult {
    let actor = actor("tenant_C3G_callback", "actor_C3G_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3G_callback")?;
    let state = oauth_input(
        MicrosoftGraphOAuthState::parse("state_C3G_0123456789abcdef"),
        "invalid state",
    )?;

    let foreign_target = MicrosoftGraphOAuthCallbackTarget::new(
        TenantId::parse("tenant_C3G_foreign")?,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
        actor.actor_id().clone(),
        UnixMillis::new(10_000),
    );
    let foreign_provisioning = FakeProvisioningPort::new(foreign_target.clone())?;
    assert_eq!(
        block_on(deny_microsoft_graph_oauth_callback(
            &actor,
            MembershipRole::TenantOwner,
            &foreign_provisioning,
            &foreign_target,
            &state,
        )),
        Err(MicrosoftGraphOAuthOnboardingError::NotFound)
    );
    assert_eq!(foreign_provisioning.deny_calls.get(), 0);

    let other_actor_target = MicrosoftGraphOAuthCallbackTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
        ActorId::parse("actor_C3G_other")?,
        UnixMillis::new(10_000),
    );
    let other_actor_provisioning = FakeProvisioningPort::new(other_actor_target.clone())?;
    assert_eq!(
        block_on(deny_microsoft_graph_oauth_callback(
            &actor,
            MembershipRole::TenantOwner,
            &other_actor_provisioning,
            &other_actor_target,
            &state,
        )),
        Err(MicrosoftGraphOAuthOnboardingError::NotFound)
    );
    assert_eq!(other_actor_provisioning.deny_calls.get(), 0);

    let exact_target = callback_target(&actor, onboarding_id, MailboxOnboardingVersion::INITIAL);
    let exact_provisioning = FakeProvisioningPort::new(exact_target.clone())?;
    block_on(deny_microsoft_graph_oauth_callback(
        &actor,
        MembershipRole::TenantOwner,
        &exact_provisioning,
        &exact_target,
        &state,
    ))?;
    assert_eq!(exact_provisioning.deny_calls.get(), 1);
    Ok(())
}

#[test]
fn completion_activates_exact_version_and_discards_handle_after_failed_commit() -> TestResult {
    let actor = actor("tenant_C3G_complete", "actor_C3G_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3G_complete")?;
    let target = callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    );
    let state = oauth_input(
        MicrosoftGraphOAuthState::parse("state_C3G_0123456789abcdef"),
        "invalid state",
    )?;

    let success_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let success_provisioning = FakeProvisioningPort::new(target.clone())?;
    let success = block_on(complete_microsoft_graph_oauth_callback(
        &actor,
        MembershipRole::TenantOwner,
        &success_port,
        &success_provisioning,
        &target,
        &state,
        authorization_code()?,
        evidence()?,
    ))?;
    assert_eq!(success.status(), MailboxOnboardingStatus::Active);
    assert_eq!(success.version(), MailboxOnboardingVersion::new(2));
    assert_eq!(success_provisioning.complete_calls.get(), 1);
    assert!(success_provisioning.discarded_handles.borrow().is_empty());
    let attempts = success_port.commit_attempts.borrow();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts
            .first()
            .and_then(MailboxOnboardingWrite::next_credential_handle)
            .map(SecretHandle::as_str),
        Some("secret_C3G_new")
    );
    drop(attempts);

    let failed_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id,
        MailboxProvider::MicrosoftGraph,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    failed_port.fail_commit_with(MailboxOnboardingPortErrorClass::VersionConflict);
    let failed_provisioning = FakeProvisioningPort::new(target.clone())?;
    assert_eq!(
        block_on(complete_microsoft_graph_oauth_callback(
            &actor,
            MembershipRole::TenantOwner,
            &failed_port,
            &failed_provisioning,
            &target,
            &state,
            authorization_code()?,
            evidence()?,
        )),
        Err(MicrosoftGraphOAuthOnboardingError::VersionConflict)
    );
    assert_eq!(failed_provisioning.complete_calls.get(), 1);
    let discarded = failed_provisioning.discarded_handles.borrow();
    assert_eq!(discarded.len(), 1);
    assert_eq!(
        discarded.first().map(SecretHandle::as_str),
        Some("secret_C3G_new")
    );
    Ok(())
}

fn actor(tenant_id: &str, actor_id: &str) -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse(tenant_id.to_owned())?),
        ActorId::parse(actor_id.to_owned())?,
        CorrelationId::parse("corr_C3G_behavior")?,
    ))
}

fn onboarding(
    actor: &ActorContext,
    onboarding_id: MailboxOnboardingId,
    provider: MailboxProvider,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
) -> Result<MailboxOnboarding, Box<dyn std::error::Error>> {
    let credential_handle = if status == MailboxOnboardingStatus::Pending {
        None
    } else {
        Some(SecretHandle::parse("secret_C3G_existing")?)
    };
    Ok(MailboxOnboarding::restore(
        actor.tenant_scope().tenant_id().clone(),
        onboarding_id,
        provider,
        status,
        credential_handle,
        None,
        version,
    ))
}

fn callback_target(
    actor: &ActorContext,
    onboarding_id: MailboxOnboardingId,
    version: MailboxOnboardingVersion,
) -> MicrosoftGraphOAuthCallbackTarget {
    MicrosoftGraphOAuthCallbackTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        onboarding_id,
        version,
        actor.actor_id().clone(),
        UnixMillis::new(10_000),
    )
}

fn authorization_code() -> Result<MicrosoftGraphOAuthAuthorizationCode, Box<dyn std::error::Error>> {
    oauth_input(
        MicrosoftGraphOAuthAuthorizationCode::parse("code_C3G_transient"),
        "invalid authorization code",
    )
}

fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse("idem_C3G_callback")?,
        "digest_C3G_callback",
        AuditEventId::parse("audit_C3G_callback")?,
        OutboxEventId::parse("outbox_C3G_callback")?,
        UnixMillis::new(1_000),
        UnixMillis::new(2_000),
    ))
}

fn oauth_input<T>(
    result: Result<T, MicrosoftGraphOAuthInputError>,
    message: &'static str,
) -> Result<T, Box<dyn std::error::Error>> {
    match result {
        Ok(value) => Ok(value),
        Err(_) => Err(std::io::Error::other(message).into()),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
