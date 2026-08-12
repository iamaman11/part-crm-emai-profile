use crate::CommandExecutionEvidence;
pub use mailbox_domain::{
    MailboxOnboarding, MailboxOnboardingAction, MailboxOnboardingStatus,
    MailboxOnboardingStatusMetadata, MailboxOnboardingVersion, MailboxProvider,
};
use profile_platform_primitives::{
    ActorContext, MailboxOnboardingId, SecretHandle, TenantScope,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingContext {
    onboarding: MailboxOnboarding,
}

impl MailboxOnboardingContext {
    #[must_use]
    pub const fn new(onboarding: MailboxOnboarding) -> Self {
        Self { onboarding }
    }

    #[must_use]
    pub const fn onboarding(&self) -> &MailboxOnboarding {
        &self.onboarding
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingWrite {
    onboarding_id: MailboxOnboardingId,
    provider: MailboxProvider,
    previous_status: Option<MailboxOnboardingStatus>,
    next_status: MailboxOnboardingStatus,
    previous_credential_handle: Option<SecretHandle>,
    next_credential_handle: Option<SecretHandle>,
    status_metadata: Option<MailboxOnboardingStatusMetadata>,
    expected_version: MailboxOnboardingVersion,
    next_version: MailboxOnboardingVersion,
    action: MailboxOnboardingAction,
    evidence: CommandExecutionEvidence,
    event_payload_json: &'static str,
}

impl MailboxOnboardingWrite {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        onboarding_id: MailboxOnboardingId,
        provider: MailboxProvider,
        previous_status: Option<MailboxOnboardingStatus>,
        next_status: MailboxOnboardingStatus,
        previous_credential_handle: Option<SecretHandle>,
        next_credential_handle: Option<SecretHandle>,
        status_metadata: Option<MailboxOnboardingStatusMetadata>,
        expected_version: MailboxOnboardingVersion,
        next_version: MailboxOnboardingVersion,
        action: MailboxOnboardingAction,
        evidence: CommandExecutionEvidence,
        event_payload_json: &'static str,
    ) -> Self {
        Self {
            onboarding_id,
            provider,
            previous_status,
            next_status,
            previous_credential_handle,
            next_credential_handle,
            status_metadata,
            expected_version,
            next_version,
            action,
            evidence,
            event_payload_json,
        }
    }

    #[must_use]
    pub const fn onboarding_id(&self) -> &MailboxOnboardingId {
        &self.onboarding_id
    }

    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn previous_status(&self) -> Option<MailboxOnboardingStatus> {
        self.previous_status
    }

    #[must_use]
    pub const fn next_status(&self) -> MailboxOnboardingStatus {
        self.next_status
    }

    #[must_use]
    pub const fn previous_credential_handle(&self) -> Option<&SecretHandle> {
        self.previous_credential_handle.as_ref()
    }

    #[must_use]
    pub const fn next_credential_handle(&self) -> Option<&SecretHandle> {
        self.next_credential_handle.as_ref()
    }

    #[must_use]
    pub const fn status_metadata(&self) -> Option<&MailboxOnboardingStatusMetadata> {
        self.status_metadata.as_ref()
    }

    #[must_use]
    pub const fn expected_version(&self) -> MailboxOnboardingVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn next_version(&self) -> MailboxOnboardingVersion {
        self.next_version
    }

    #[must_use]
    pub const fn action(&self) -> MailboxOnboardingAction {
        self.action
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub const fn event_payload_json(&self) -> &str {
        self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl MailboxOnboardingReplayReceipt {
    #[must_use]
    pub const fn new(result_code: String, result_reference: Option<String>) -> Self {
        Self {
            result_code,
            result_reference,
        }
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingReplayDecision {
    Miss,
    Replay(MailboxOnboardingReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxOnboardingPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailboxOnboardingPortError {
    class: MailboxOnboardingPortErrorClass,
}

impl MailboxOnboardingPortError {
    #[must_use]
    pub const fn new(class: MailboxOnboardingPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> MailboxOnboardingPortErrorClass {
        self.class
    }
}

impl core::fmt::Display for MailboxOnboardingPortError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("mailbox onboarding persistence operation failed")
    }
}

impl std::error::Error for MailboxOnboardingPortError {}

pub trait MailboxOnboardingApplicationPort {
    fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> impl core::future::Future<
        Output = Result<MailboxOnboardingReplayDecision, MailboxOnboardingPortError>,
    >;

    fn load_context(
        &self,
        scope: &TenantScope,
        onboarding_id: &MailboxOnboardingId,
    ) -> impl core::future::Future<
        Output = Result<Option<MailboxOnboardingContext>, MailboxOnboardingPortError>,
    >;

    fn commit(
        &self,
        actor: &ActorContext,
        write: &MailboxOnboardingWrite,
    ) -> impl core::future::Future<Output = Result<(), MailboxOnboardingPortError>>;
}
