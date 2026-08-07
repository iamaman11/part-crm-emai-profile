use crate::error::ApplicationError;
use application_ports::CommandExecutionEvidence;
use application_ports::profiles::{
    ProfileApplicationPort, ProfileCreateWrite, ProfilePortError, ProfilePortErrorClass,
    ProfileReadModel, ProfileReplayDecision, ProfileReplayReceipt,
};
use contracts::ProblemCode;
use core::fmt;
use identity_access_domain::{
    AuthorizationDecision, Membership, MembershipRole, ProfileCapability, ProfileGrant,
    authorize_profile,
};
use profile_domain::{BrowserProfile, ProfileStatus};
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, DeviceId, ProfileId};

const PROFILE_CREATE_COMMAND: &str = "profile.create";
const PROFILE_CREATED_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCreateProfileCommand {
    profile_id: ProfileId,
    evidence: CommandExecutionEvidence,
}

impl ExecuteCreateProfileCommand {
    #[must_use]
    pub const fn new(profile_id: ProfileId, evidence: CommandExecutionEvidence) -> Self {
        Self {
            profile_id,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ProfileMutationOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDetails {
    profile_id: ProfileId,
    status: ProfileStatus,
    version: AggregateVersion,
    linked_client_id: Option<ClientId>,
}

impl ProfileDetails {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn status(&self) -> ProfileStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn linked_client_id(&self) -> Option<&ClientId> {
        self.linked_client_id.as_ref()
    }
}

impl From<ProfileReadModel> for ProfileDetails {
    fn from(value: ProfileReadModel) -> Self {
        Self {
            profile_id: value.profile_id().clone(),
            status: value.status(),
            version: value.version(),
            linked_client_id: value.linked_client_id().cloned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileOperationError {
    NotFound,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ProfileOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "profile not found",
            Self::Conflict => "profile command conflict",
            Self::IntegrityFailure => "profile data integrity failure",
            Self::InternalFailure => "profile application internal failure",
            Self::DependencyUnavailable => "profile dependency unavailable",
        })
    }
}

impl std::error::Error for ProfileOperationError {}

pub fn authorize_profile_create(role: MembershipRole) -> Result<(), ProfileOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ProfileOperationError::NotFound)
    }
}

pub async fn execute_create_profile<P: ProfileApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteCreateProfileCommand,
) -> Result<ProfileMutationOutcome, ProfileOperationError> {
    authorize_profile_create(role)?;

    let profile =
        BrowserProfile::create(actor.tenant_scope().tenant_id().clone(), command.profile_id);

    match port
        .decide_replay(actor, PROFILE_CREATE_COMMAND, &command.evidence)
        .await
        .map_err(map_profile_port_error)?
    {
        ProfileReplayDecision::Miss => {}
        ProfileReplayDecision::Replay(receipt) => {
            return Ok(replay_outcome(&profile, &receipt));
        }
        ProfileReplayDecision::Conflict => return Err(ProfileOperationError::Conflict),
    }

    let write = ProfileCreateWrite::new(profile, command.evidence, PROFILE_CREATED_EVENT_PAYLOAD);
    match port.create_profile(actor, &write).await {
        Ok(()) => Ok(ProfileMutationOutcome {
            result_code: "created".to_owned(),
            resource_id: write.profile().profile_id().as_str().to_owned(),
            aggregate_version: AggregateVersion::INITIAL,
            replayed: false,
        }),
        Err(error) if error.class() == ProfilePortErrorClass::Conflict => {
            match port
                .decide_replay(actor, PROFILE_CREATE_COMMAND, write.evidence())
                .await
                .map_err(map_profile_port_error)?
            {
                ProfileReplayDecision::Replay(receipt) => {
                    Ok(replay_outcome(write.profile(), &receipt))
                }
                ProfileReplayDecision::Miss | ProfileReplayDecision::Conflict => {
                    Err(ProfileOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_profile_port_error(error)),
    }
}

pub async fn get_visible_profile<P: ProfileApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    profile_id: &ProfileId,
) -> Result<ProfileDetails, ProfileOperationError> {
    port.find_visible_profile(actor.tenant_scope(), actor.actor_id(), role, profile_id)
        .await
        .map_err(map_profile_port_error)?
        .map(ProfileDetails::from)
        .ok_or(ProfileOperationError::NotFound)
}

fn replay_outcome(
    profile: &BrowserProfile,
    receipt: &ProfileReplayReceipt,
) -> ProfileMutationOutcome {
    ProfileMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(profile.profile_id().as_str())
            .to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn map_profile_port_error(error: ProfilePortError) -> ProfileOperationError {
    match error.class() {
        ProfilePortErrorClass::Conflict => ProfileOperationError::Conflict,
        ProfilePortErrorClass::IntegrityFailure => ProfileOperationError::IntegrityFailure,
        ProfilePortErrorClass::InternalFailure => ProfileOperationError::InternalFailure,
        ProfilePortErrorClass::DependencyUnavailable => {
            ProfileOperationError::DependencyUnavailable
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenProfileCommand {
    profile_id: ProfileId,
    device_id: DeviceId,
}

impl OpenProfileCommand {
    #[must_use]
    pub const fn new(profile_id: ProfileId, device_id: DeviceId) -> Self {
        Self {
            profile_id,
            device_id,
        }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenProfileDecision {
    profile_id: ProfileId,
    device_id: DeviceId,
}

impl OpenProfileDecision {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

pub fn decide_open_profile(
    actor: &ActorContext,
    membership: &Membership,
    grant: Option<&ProfileGrant>,
    profile: &BrowserProfile,
    command: OpenProfileCommand,
) -> Result<OpenProfileDecision, ApplicationError> {
    if actor.tenant_scope().tenant_id() != profile.tenant_id()
        || command.profile_id() != profile.profile_id()
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    if authorize_profile(
        actor,
        membership,
        profile.profile_id(),
        grant,
        ProfileCapability::Operate,
    ) != AuthorizationDecision::Allowed
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    if profile.status() != ProfileStatus::Ready || profile.active_generation_id().is_none() {
        return Err(ApplicationError::new(ProblemCode::InvalidState));
    }

    Ok(OpenProfileDecision {
        profile_id: command.profile_id,
        device_id: command.device_id,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ExecuteCreateProfileCommand, OpenProfileCommand, ProfileOperationError,
        authorize_profile_create, decide_open_profile, execute_create_profile, get_visible_profile,
    };
    use crate::error::ApplicationError;
    use application_ports::CommandExecutionEvidence;
    use application_ports::profiles::{
        ProfileApplicationPort, ProfileCreateWrite, ProfilePortError, ProfilePortErrorClass,
        ProfileReadModel, ProfileReplayDecision, ProfileReplayReceipt,
    };
    use contracts::ProblemCode;
    use identity_access_domain::{
        Membership, MembershipRole, MembershipStatus, ProfileGrant, ProfileGrantRole,
    };
    use profile_domain::{
        BrowserProfile, GenerationVerification, ProfileGeneration, ProfileStatus,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, CorrelationId, DeviceId,
        GenerationId, IdempotencyKey, OutboxEventId, ProfileId, TenantId, TenantScope, UnixMillis,
    };
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakeProfilePort {
        replay: RefCell<Vec<ProfileReplayDecision>>,
        replay_calls: Cell<u32>,
        create_error: Cell<Option<ProfilePortErrorClass>>,
        create_calls: Cell<u32>,
        visible: RefCell<Option<ProfileReadModel>>,
    }

    impl FakeProfilePort {
        fn new(replay: Vec<ProfileReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_calls: Cell::new(0),
                create_error: Cell::new(None),
                create_calls: Cell::new(0),
                visible: RefCell::new(None),
            }
        }

        fn next_replay(&self) -> ProfileReplayDecision {
            let mut replay = self.replay.borrow_mut();
            if replay.is_empty() {
                ProfileReplayDecision::Miss
            } else {
                replay.remove(0)
            }
        }
    }

    impl ProfileApplicationPort for FakeProfilePort {
        async fn decide_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ProfileReplayDecision, ProfilePortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(self.next_replay())
        }

        async fn create_profile(
            &self,
            _actor: &ActorContext,
            _write: &ProfileCreateWrite,
        ) -> Result<(), ProfilePortError> {
            self.create_calls.set(self.create_calls.get() + 1);
            match self.create_error.get() {
                Some(class) => Err(ProfilePortError::new(class)),
                None => Ok(()),
            }
        }

        async fn find_visible_profile(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _role: MembershipRole,
            _profile_id: &ProfileId,
        ) -> Result<Option<ProfileReadModel>, ProfilePortError> {
            Ok(self.visible.borrow().clone())
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JPROFILEAPP")?),
            ActorId::parse("actor_01JPROFILEAPP")?,
            CorrelationId::parse("corr_01JPROFILEAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JPROFILEAPP")?,
            "digest_01JPROFILEAPP",
            AuditEventId::parse("audit_01JPROFILEAPP")?,
            OutboxEventId::parse("outbox_01JPROFILEAPP")?,
            UnixMillis::new(10),
            UnixMillis::new(20),
        ))
    }

    fn create_command() -> Result<ExecuteCreateProfileCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteCreateProfileCommand::new(
            ProfileId::parse("profile_01JPROFILEAPP")?,
            evidence()?,
        ))
    }

    #[test]
    fn create_authorization_is_disclosure_neutral() {
        assert_eq!(
            authorize_profile_create(MembershipRole::TenantOwner),
            Ok(())
        );
        assert_eq!(
            authorize_profile_create(MembershipRole::Member),
            Err(ProfileOperationError::NotFound)
        );
    }

    #[test]
    fn owner_create_writes_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeProfilePort::new(vec![ProfileReplayDecision::Miss]);
        let outcome = block_on(execute_create_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert_eq!(outcome.result_code(), "created");
        assert!(!outcome.replayed());
        assert_eq!(outcome.aggregate_version(), AggregateVersion::INITIAL);
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.create_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn member_create_is_neutral_and_never_reads_replay_or_writes()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeProfilePort::new(vec![ProfileReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_create_profile(
                &actor()?,
                MembershipRole::Member,
                &port,
                create_command()?,
            )),
            Err(ProfileOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_write_and_preserves_receipt() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeProfilePort::new(vec![ProfileReplayDecision::Replay(
            ProfileReplayReceipt::new("created", Some("profile_existing".to_owned())),
        )]);
        let outcome = block_on(execute_create_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "profile_existing");
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn concurrent_conflict_replays_only_after_exact_recheck()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeProfilePort::new(vec![
            ProfileReplayDecision::Miss,
            ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                "created",
                Some("profile_01JPROFILEAPP".to_owned()),
            )),
        ]);
        port.create_error.set(Some(ProfilePortErrorClass::Conflict));
        let outcome = block_on(execute_create_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.create_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn visible_query_returns_typed_profile_view() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeProfilePort::new(Vec::new());
        port.visible.replace(Some(ProfileReadModel::new(
            ProfileId::parse("profile_01JPROFILEAPP")?,
            ProfileStatus::Ready,
            AggregateVersion::new(4)?,
            Some(ClientId::parse("client_01JPROFILEAPP")?),
        )));
        let profile_id = ProfileId::parse("profile_01JPROFILEAPP")?;
        let details = block_on(get_visible_profile(
            &actor()?,
            MembershipRole::Member,
            &port,
            &profile_id,
        ))?;
        assert_eq!(details.profile_id(), &profile_id);
        assert_eq!(details.status(), ProfileStatus::Ready);
        assert_eq!(details.version().value(), 4);
        assert_eq!(
            details.linked_client_id().map(ClientId::as_str),
            Some("client_01JPROFILEAPP")
        );
        Ok(())
    }

    struct Fixture {
        actor: ActorContext,
        membership: Membership,
        grant: ProfileGrant,
        profile: BrowserProfile,
    }

    fn fixture(role: ProfileGrantRole) -> Result<Fixture, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JUSECASE")?;
        let actor_id = ActorId::parse("actor_01JUSECASE")?;
        let profile_id = ProfileId::parse("profile_01JUSECASE")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            actor_id.clone(),
            CorrelationId::parse("corr_01JUSECASE")?,
        );
        let membership = Membership::new(
            tenant_id.clone(),
            actor_id.clone(),
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        let grant = ProfileGrant::new(tenant_id.clone(), actor_id, profile_id.clone(), role);
        let mut profile = BrowserProfile::create(tenant_id.clone(), profile_id.clone());
        let generation = ProfileGeneration::new(
            tenant_id,
            profile_id,
            GenerationId::parse("generation_01JUSECASE")?,
            GenerationVerification::Verified,
        );
        profile.activate_generation(&generation)?;
        Ok(Fixture {
            actor,
            membership,
            grant,
            profile,
        })
    }

    #[test]
    fn operator_can_open_profile_with_verified_active_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Operator)?;
        let decision = decide_open_profile(
            &fixture.actor,
            &fixture.membership,
            Some(&fixture.grant),
            &fixture.profile,
            OpenProfileCommand::new(
                fixture.profile.profile_id().clone(),
                DeviceId::parse("device_01JUSECASE")?,
            ),
        )?;
        assert_eq!(decision.profile_id(), fixture.profile.profile_id());
        assert!(fixture.profile.active_generation_id().is_some());
        Ok(())
    }

    #[test]
    fn viewer_cannot_open_profile() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Viewer)?;
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &fixture.profile,
                OpenProfileCommand::new(
                    fixture.profile.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::NotFound))
        );
        Ok(())
    }

    #[test]
    fn profile_without_active_generation_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture(ProfileGrantRole::Operator)?;
        let draft = BrowserProfile::create(
            fixture.profile.tenant_id().clone(),
            fixture.profile.profile_id().clone(),
        );
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &draft,
                OpenProfileCommand::new(
                    draft.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::InvalidState))
        );
        Ok(())
    }

    #[test]
    fn non_ready_profile_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = fixture(ProfileGrantRole::Operator)?;
        fixture.profile.transition(ProfileStatus::InUse)?;
        assert_eq!(
            decide_open_profile(
                &fixture.actor,
                &fixture.membership,
                Some(&fixture.grant),
                &fixture.profile,
                OpenProfileCommand::new(
                    fixture.profile.profile_id().clone(),
                    DeviceId::parse("device_01JUSECASE")?,
                ),
            ),
            Err(ApplicationError::new(ProblemCode::InvalidState))
        );
        Ok(())
    }
}
