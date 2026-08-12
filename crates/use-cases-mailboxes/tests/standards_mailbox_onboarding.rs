use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_onboarding::{
    MailboxOnboardingApplicationPort, MailboxOnboardingContext, MailboxOnboardingPortError,
    MailboxOnboardingPortErrorClass, MailboxOnboardingReplayDecision, MailboxOnboardingWrite,
};
use application_ports::standards_mailbox_onboarding::{
    MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthAuthorizationUrl,
    MicrosoftStandardsOAuthCallbackTarget, MicrosoftStandardsOAuthCeremonyId,
    MicrosoftStandardsOAuthStartReceipt, MicrosoftStandardsOAuthState, StandardsMailEndpoint,
    StandardsMailProtocol, StandardsMailTransportSecurity, StandardsMailboxAuthenticationMode,
    StandardsMailboxInputError, StandardsMailboxPassword, StandardsMailboxProvisioningError,
    StandardsMailboxProvisioningPort, StandardsMailboxProvisioningReceipt,
    StandardsMailboxUsername, StandardsPasswordMailboxConfiguration,
    StandardsPasswordProtocolCredential,
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
use use_cases_mailboxes::standards_mailbox_onboarding::{
    StandardsMailboxOnboardingError, complete_microsoft_standards_oauth_callback,
    deny_microsoft_standards_oauth_callback, provision_password_standards_mailbox,
    start_microsoft_standards_oauth,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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
    start_receipt: MicrosoftStandardsOAuthStartReceipt,
    target: MicrosoftStandardsOAuthCallbackTarget,
    receipt: RefCell<StandardsMailboxProvisioningReceipt>,
    provision_calls: Cell<usize>,
    start_calls: Cell<usize>,
    complete_calls: Cell<usize>,
    deny_calls: Cell<usize>,
    discarded_handles: RefCell<Vec<SecretHandle>>,
}

impl FakeProvisioningPort {
    fn new(target: MicrosoftStandardsOAuthCallbackTarget) -> TestResult<Self> {
        Ok(Self {
            start_receipt: MicrosoftStandardsOAuthStartReceipt::new(
                input(
                    MicrosoftStandardsOAuthCeremonyId::parse("ceremony_C3_behavior"),
                    "invalid ceremony id",
                )?,
                input(
                    MicrosoftStandardsOAuthAuthorizationUrl::parse(
                        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?state=opaque",
                    ),
                    "invalid authorization url",
                )?,
                UnixMillis::new(10_000),
            ),
            target,
            receipt: RefCell::new(provisioning_receipt(
                "secret_C3_new",
                StandardsMailboxAuthenticationMode::Password,
                true,
                true,
            )?),
            provision_calls: Cell::new(0),
            start_calls: Cell::new(0),
            complete_calls: Cell::new(0),
            deny_calls: Cell::new(0),
            discarded_handles: RefCell::new(Vec::new()),
        })
    }

    fn set_receipt(
        &self,
        mode: StandardsMailboxAuthenticationMode,
        imap_ready: bool,
        smtp_ready: bool,
    ) -> TestResult {
        self.receipt.replace(provisioning_receipt(
            "secret_C3_new",
            mode,
            imap_ready,
            smtp_ready,
        )?);
        Ok(())
    }
}

impl StandardsMailboxProvisioningPort for FakeProvisioningPort {
    fn provision_password(
        &self,
        _actor: &ActorContext,
        _onboarding_id: &MailboxOnboardingId,
        _expected_version: MailboxOnboardingVersion,
        _idempotency_key: &IdempotencyKey,
        _configuration: StandardsPasswordMailboxConfiguration,
    ) -> impl Future<
        Output = Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError>,
    > {
        self.provision_calls.set(self.provision_calls.get() + 1);
        ready(Ok(self.receipt.borrow().clone()))
    }

    fn start_microsoft_oauth(
        &self,
        _actor: &ActorContext,
        _onboarding_id: &MailboxOnboardingId,
        _expected_version: MailboxOnboardingVersion,
    ) -> impl Future<
        Output = Result<MicrosoftStandardsOAuthStartReceipt, StandardsMailboxProvisioningError>,
    > {
        self.start_calls.set(self.start_calls.get() + 1);
        ready(Ok(self.start_receipt.clone()))
    }

    fn inspect_microsoft_oauth(
        &self,
        _state: &MicrosoftStandardsOAuthState,
    ) -> impl Future<
        Output = Result<MicrosoftStandardsOAuthCallbackTarget, StandardsMailboxProvisioningError>,
    > {
        ready(Ok(self.target.clone()))
    }

    fn complete_microsoft_oauth(
        &self,
        _actor: &ActorContext,
        _state: &MicrosoftStandardsOAuthState,
        _authorization_code: MicrosoftStandardsOAuthAuthorizationCode,
    ) -> impl Future<
        Output = Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError>,
    > {
        self.complete_calls.set(self.complete_calls.get() + 1);
        ready(Ok(self.receipt.borrow().clone()))
    }

    fn deny_microsoft_oauth(
        &self,
        _actor: &ActorContext,
        _state: &MicrosoftStandardsOAuthState,
    ) -> impl Future<Output = Result<(), StandardsMailboxProvisioningError>> {
        self.deny_calls.set(self.deny_calls.get() + 1);
        ready(Ok(()))
    }

    fn discard(
        &self,
        _actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> impl Future<Output = Result<(), StandardsMailboxProvisioningError>> {
        self.discarded_handles
            .borrow_mut()
            .push(secret_handle.clone());
        ready(Ok(()))
    }
}

#[test]
fn password_pending_and_reauth_activate_with_only_an_opaque_handle() -> TestResult {
    let actor = actor("tenant_C3_password", "actor_C3_owner")?;
    for (suffix, status, version) in [
        (
            "pending",
            MailboxOnboardingStatus::Pending,
            MailboxOnboardingVersion::INITIAL,
        ),
        (
            "reauth",
            MailboxOnboardingStatus::ReauthRequired,
            MailboxOnboardingVersion::new(4),
        ),
    ] {
        let onboarding_id = MailboxOnboardingId::parse(format!("onboarding_C3_{suffix}"))?;
        let onboarding_port = FakeOnboardingPort::new(onboarding(
            &actor,
            onboarding_id.clone(),
            MailboxProvider::Imap,
            status,
            version,
        )?);
        let provisioning =
            FakeProvisioningPort::new(callback_target(&actor, onboarding_id.clone(), version))?;
        let outcome = block_on(provision_password_standards_mailbox(
            &actor,
            MembershipRole::TenantOwner,
            &onboarding_port,
            &provisioning,
            onboarding_id,
            version,
            password_configuration()?,
            evidence(&format!("{suffix}_password"))?,
        ))?;
        assert_eq!(outcome.status(), MailboxOnboardingStatus::Active);
        assert_eq!(
            outcome.version(),
            MailboxOnboardingVersion::new(version.value() + 1)
        );
        assert_eq!(
            outcome.authentication_mode(),
            StandardsMailboxAuthenticationMode::Password
        );
        assert!(outcome.imap_read_search_ready());
        assert!(outcome.smtp_send_ready());
        assert_eq!(provisioning.provision_calls.get(), 1);
        assert!(provisioning.discarded_handles.borrow().is_empty());
        let commits = onboarding_port.commit_attempts.borrow();
        assert_eq!(commits.len(), 1);
        assert_eq!(
            commits
                .first()
                .and_then(MailboxOnboardingWrite::next_credential_handle)
                .map(SecretHandle::as_str),
            Some("secret_C3_new")
        );
    }
    Ok(())
}

#[test]
fn owner_provider_state_and_version_preconditions_fail_before_resolver() -> TestResult {
    let actor = actor("tenant_C3_guard", "actor_C3_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3_guard")?;

    let member_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::Imap,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let member_resolver = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_standards_oauth(
            &actor,
            MembershipRole::Member,
            &member_port,
            &member_resolver,
            onboarding_id.clone(),
            MailboxOnboardingVersion::INITIAL,
        )),
        Err(StandardsMailboxOnboardingError::NotFound)
    );
    assert_eq!(member_resolver.start_calls.get(), 0);

    let stale_resolver = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_standards_oauth(
            &actor,
            MembershipRole::TenantOwner,
            &member_port,
            &stale_resolver,
            onboarding_id.clone(),
            MailboxOnboardingVersion::new(2),
        )),
        Err(StandardsMailboxOnboardingError::VersionConflict)
    );
    assert_eq!(stale_resolver.start_calls.get(), 0);

    let gmail_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::GmailApi,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let gmail_resolver = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(start_microsoft_standards_oauth(
            &actor,
            MembershipRole::TenantOwner,
            &gmail_port,
            &gmail_resolver,
            onboarding_id.clone(),
            MailboxOnboardingVersion::INITIAL,
        )),
        Err(StandardsMailboxOnboardingError::InvalidState)
    );
    assert_eq!(gmail_resolver.start_calls.get(), 0);

    for status in [
        MailboxOnboardingStatus::Active,
        MailboxOnboardingStatus::Disabled,
        MailboxOnboardingStatus::ConfigError,
    ] {
        let version = MailboxOnboardingVersion::new(3);
        let port = FakeOnboardingPort::new(onboarding(
            &actor,
            onboarding_id.clone(),
            MailboxProvider::Imap,
            status,
            version,
        )?);
        let resolver =
            FakeProvisioningPort::new(callback_target(&actor, onboarding_id.clone(), version))?;
        assert_eq!(
            block_on(start_microsoft_standards_oauth(
                &actor,
                MembershipRole::TenantOwner,
                &port,
                &resolver,
                onboarding_id.clone(),
                version,
            )),
            Err(StandardsMailboxOnboardingError::InvalidState)
        );
        assert_eq!(resolver.start_calls.get(), 0);
    }
    Ok(())
}

#[test]
fn microsoft_callback_is_exactly_tenant_actor_bound_and_can_activate() -> TestResult {
    let actor = actor("tenant_C3_callback", "actor_C3_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3_callback")?;
    let state = input(
        MicrosoftStandardsOAuthState::parse("state_C3_0123456789abcdef"),
        "invalid OAuth state",
    )?;

    let foreign_target = MicrosoftStandardsOAuthCallbackTarget::new(
        TenantId::parse("tenant_C3_foreign")?,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
        actor.actor_id().clone(),
        UnixMillis::new(10_000),
    );
    let foreign_resolver = FakeProvisioningPort::new(foreign_target.clone())?;
    assert_eq!(
        block_on(deny_microsoft_standards_oauth_callback(
            &actor,
            MembershipRole::TenantOwner,
            &foreign_resolver,
            &foreign_target,
            &state,
        )),
        Err(StandardsMailboxOnboardingError::NotFound)
    );
    assert_eq!(foreign_resolver.deny_calls.get(), 0);

    let other_actor_target = MicrosoftStandardsOAuthCallbackTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
        ActorId::parse("actor_C3_other")?,
        UnixMillis::new(10_000),
    );
    let other_actor_resolver = FakeProvisioningPort::new(other_actor_target.clone())?;
    assert_eq!(
        block_on(deny_microsoft_standards_oauth_callback(
            &actor,
            MembershipRole::TenantOwner,
            &other_actor_resolver,
            &other_actor_target,
            &state,
        )),
        Err(StandardsMailboxOnboardingError::NotFound)
    );
    assert_eq!(other_actor_resolver.deny_calls.get(), 0);

    let target = callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    );
    let onboarding_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id,
        MailboxProvider::Imap,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let resolver = FakeProvisioningPort::new(target.clone())?;
    resolver.set_receipt(
        StandardsMailboxAuthenticationMode::MicrosoftOAuth2,
        true,
        true,
    )?;
    let outcome = block_on(complete_microsoft_standards_oauth_callback(
        &actor,
        MembershipRole::TenantOwner,
        &onboarding_port,
        &resolver,
        &target,
        &state,
        authorization_code()?,
        evidence("microsoft_complete")?,
    ))?;
    assert_eq!(outcome.status(), MailboxOnboardingStatus::Active);
    assert_eq!(
        outcome.authentication_mode(),
        StandardsMailboxAuthenticationMode::MicrosoftOAuth2
    );
    assert_eq!(resolver.complete_calls.get(), 1);
    assert!(resolver.discarded_handles.borrow().is_empty());
    Ok(())
}

#[test]
fn invalid_readiness_or_failed_c1_commit_discards_resolver_handle() -> TestResult {
    let actor = actor("tenant_C3_discard", "actor_C3_owner")?;
    let onboarding_id = MailboxOnboardingId::parse("onboarding_C3_discard")?;

    let invalid_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::Imap,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    let invalid_resolver = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    invalid_resolver.set_receipt(StandardsMailboxAuthenticationMode::Password, true, false)?;
    assert_eq!(
        block_on(provision_password_standards_mailbox(
            &actor,
            MembershipRole::TenantOwner,
            &invalid_port,
            &invalid_resolver,
            onboarding_id.clone(),
            MailboxOnboardingVersion::INITIAL,
            password_configuration()?,
            evidence("bad_readiness")?,
        )),
        Err(StandardsMailboxOnboardingError::IntegrityFailure)
    );
    assert!(invalid_port.commit_attempts.borrow().is_empty());
    assert_eq!(invalid_resolver.discarded_handles.borrow().len(), 1);

    let failed_port = FakeOnboardingPort::new(onboarding(
        &actor,
        onboarding_id.clone(),
        MailboxProvider::Imap,
        MailboxOnboardingStatus::Pending,
        MailboxOnboardingVersion::INITIAL,
    )?);
    failed_port.fail_commit_with(MailboxOnboardingPortErrorClass::VersionConflict);
    let failed_resolver = FakeProvisioningPort::new(callback_target(
        &actor,
        onboarding_id.clone(),
        MailboxOnboardingVersion::INITIAL,
    ))?;
    assert_eq!(
        block_on(provision_password_standards_mailbox(
            &actor,
            MembershipRole::TenantOwner,
            &failed_port,
            &failed_resolver,
            onboarding_id,
            MailboxOnboardingVersion::INITIAL,
            password_configuration()?,
            evidence("failed_commit")?,
        )),
        Err(StandardsMailboxOnboardingError::VersionConflict)
    );
    let discarded = failed_resolver.discarded_handles.borrow();
    assert_eq!(discarded.len(), 1);
    assert_eq!(
        discarded.first().map(SecretHandle::as_str),
        Some("secret_C3_new")
    );
    Ok(())
}

fn actor(tenant_id: &str, actor_id: &str) -> TestResult<ActorContext> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse(tenant_id.to_owned())?),
        ActorId::parse(actor_id.to_owned())?,
        CorrelationId::parse("corr_C3_behavior")?,
    ))
}

fn onboarding(
    actor: &ActorContext,
    onboarding_id: MailboxOnboardingId,
    provider: MailboxProvider,
    status: MailboxOnboardingStatus,
    version: MailboxOnboardingVersion,
) -> TestResult<MailboxOnboarding> {
    let credential_handle = if status == MailboxOnboardingStatus::Pending {
        None
    } else {
        Some(SecretHandle::parse("secret_C3_existing")?)
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
) -> MicrosoftStandardsOAuthCallbackTarget {
    MicrosoftStandardsOAuthCallbackTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        onboarding_id,
        version,
        actor.actor_id().clone(),
        UnixMillis::new(10_000),
    )
}

fn password_configuration() -> TestResult<StandardsPasswordMailboxConfiguration> {
    Ok(input(
        StandardsPasswordMailboxConfiguration::new(
            StandardsPasswordProtocolCredential::new(
                input(
                    StandardsMailEndpoint::parse(
                        StandardsMailProtocol::Imap,
                        "imap.example.com",
                        993,
                        StandardsMailTransportSecurity::ImplicitTls,
                    ),
                    "invalid IMAP endpoint",
                )?,
                input(
                    StandardsMailboxUsername::parse("user@example.com"),
                    "invalid IMAP username",
                )?,
                input(
                    StandardsMailboxPassword::parse("imap-secret"),
                    "invalid IMAP password",
                )?,
            ),
            StandardsPasswordProtocolCredential::new(
                input(
                    StandardsMailEndpoint::parse(
                        StandardsMailProtocol::Smtp,
                        "smtp.example.com",
                        587,
                        StandardsMailTransportSecurity::StartTls,
                    ),
                    "invalid SMTP endpoint",
                )?,
                input(
                    StandardsMailboxUsername::parse("user@example.com"),
                    "invalid SMTP username",
                )?,
                input(
                    StandardsMailboxPassword::parse("smtp-secret"),
                    "invalid SMTP password",
                )?,
            ),
        ),
        "invalid standards mailbox configuration",
    )?)
}

fn provisioning_receipt(
    handle: &str,
    mode: StandardsMailboxAuthenticationMode,
    imap_ready: bool,
    smtp_ready: bool,
) -> TestResult<StandardsMailboxProvisioningReceipt> {
    Ok(StandardsMailboxProvisioningReceipt::new(
        SecretHandle::parse(handle)?,
        mode,
        imap_ready,
        smtp_ready,
    ))
}

fn authorization_code() -> TestResult<MicrosoftStandardsOAuthAuthorizationCode> {
    input(
        MicrosoftStandardsOAuthAuthorizationCode::parse("code_C3_transient"),
        "invalid authorization code",
    )
}

fn evidence(suffix: &str) -> TestResult<CommandExecutionEvidence> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse(format!("idem_C3_{suffix}"))?,
        format!("digest_C3_{suffix}"),
        AuditEventId::parse(format!("audit_C3_{suffix}"))?,
        OutboxEventId::parse(format!("outbox_C3_{suffix}"))?,
        UnixMillis::new(1_000),
        UnixMillis::new(2_000),
    ))
}

fn input<T>(result: Result<T, StandardsMailboxInputError>, message: &'static str) -> TestResult<T> {
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
