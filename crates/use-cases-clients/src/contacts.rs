use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ArchiveContactWrite, ClientPortError, ClientPortErrorClass, ClientReplayDecision,
    ClientReplayReceipt, ContactEncryptionRequest, ContactExactLookupRequest, ContactProtectionPort,
    ContactProtectionPortError, ContactProtectionPortErrorClass, ProtectedClientContactRepositoryPort,
    ProtectedContactWrite,
};
use client_domain::{
    ClientStatus, ContactKind, ContactNormalizationVersion, ContactProtectionVersion, ContactStatus,
    ProtectedContactPoint, exact_lookup_hmac_input, normalize_contact_value,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, ContactPointId};
use zeroize::Zeroize;

const CLIENT_CONTACT_UPSERT_COMMAND: &str = "client.contact_upsert";
const CLIENT_CONTACT_ARCHIVE_COMMAND: &str = "client.contact_archive";
const EVENT_PAYLOAD: &str = "{}";

pub struct TransientContactValue {
    value: String,
}

impl TransientContactValue {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for TransientContactValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TransientContactValue([REDACTED])")
    }
}

impl Drop for TransientContactValue {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

pub struct PrepareProtectedContactCommand {
    client_id: ClientId,
    contact_point_id: ContactPointId,
    expected_client_version: AggregateVersion,
    kind: ContactKind,
    value: TransientContactValue,
    evidence: CommandExecutionEvidence,
}

impl PrepareProtectedContactCommand {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        contact_point_id: ContactPointId,
        expected_client_version: AggregateVersion,
        kind: ContactKind,
        value: TransientContactValue,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            client_id,
            contact_point_id,
            expected_client_version,
            kind,
            value,
            evidence,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        &self.contact_point_id
    }

    #[must_use]
    pub const fn expected_client_version(&self) -> AggregateVersion {
        self.expected_client_version
    }

    #[must_use]
    pub const fn kind(&self) -> ContactKind {
        self.kind
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }
}

impl fmt::Debug for PrepareProtectedContactCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrepareProtectedContactCommand")
            .field("client_id", &self.client_id)
            .field("contact_point_id", &self.contact_point_id)
            .field("expected_client_version", &self.expected_client_version)
            .field("kind", &self.kind)
            .field("value", &"[REDACTED]")
            .field("evidence", &"[COMMAND_EVIDENCE]")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveContactCommand {
    client_id: ClientId,
    contact_point_id: ContactPointId,
    expected_client_version: AggregateVersion,
    kind: ContactKind,
    evidence: CommandExecutionEvidence,
}

impl ArchiveContactCommand {
    #[must_use]
    pub const fn new(
        client_id: ClientId,
        contact_point_id: ContactPointId,
        expected_client_version: AggregateVersion,
        kind: ContactKind,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            client_id,
            contact_point_id,
            expected_client_version,
            kind,
            evidence,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        &self.contact_point_id
    }

    #[must_use]
    pub const fn expected_client_version(&self) -> AggregateVersion {
        self.expected_client_version
    }

    #[must_use]
    pub const fn kind(&self) -> ContactKind {
        self.kind
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContactMutationOutcome {
    Applied {
        client_id: ClientId,
        contact_point_id: ContactPointId,
        client_version: AggregateVersion,
    },
    Replayed(ClientReplayReceipt),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactApplicationError {
    NotFound,
    InvalidRequest,
    VersionConflict,
    InvalidState,
    Conflict,
    KeyUnavailable,
    IntegrityFailure,
    DependencyUnavailable,
    InternalFailure,
}

impl fmt::Display for ContactApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client contact not found",
            Self::InvalidRequest => "client contact request is invalid",
            Self::VersionConflict => "client contact version conflict",
            Self::InvalidState => "client contact invalid state",
            Self::Conflict => "client contact conflict",
            Self::KeyUnavailable => "client contact protection key unavailable",
            Self::IntegrityFailure => "client contact protection integrity failure",
            Self::DependencyUnavailable => "client contact dependency unavailable",
            Self::InternalFailure => "client contact application internal failure",
        })
    }
}

impl std::error::Error for ContactApplicationError {}

pub fn authorize_contact_mutation(role: MembershipRole) -> Result<(), ContactApplicationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ContactApplicationError::NotFound)
    }
}

pub async fn execute_upsert_contact<P, R>(
    actor: &ActorContext,
    role: MembershipRole,
    protector: &P,
    repository: &R,
    command: PrepareProtectedContactCommand,
) -> Result<ContactMutationOutcome, ContactApplicationError>
where
    P: ContactProtectionPort,
    R: ProtectedClientContactRepositoryPort<Error = ClientPortError>,
{
    authorize_contact_mutation(role)?;
    match repository
        .decide_client_contact_replay(actor, CLIENT_CONTACT_UPSERT_COMMAND, command.evidence())
        .await
        .map_err(map_repository_error)?
    {
        ClientReplayDecision::Replay(receipt) => {
            return Ok(ContactMutationOutcome::Replayed(receipt));
        }
        ClientReplayDecision::Conflict => return Err(ContactApplicationError::Conflict),
        ClientReplayDecision::Miss => {}
    }

    let current = repository
        .load_client_for_contact_mutation(actor.tenant_scope(), command.client_id())
        .await
        .map_err(map_repository_error)?
        .ok_or(ContactApplicationError::NotFound)?;
    validate_client_for_contact(&current, command.expected_client_version())?;

    let client_id = command.client_id().clone();
    let contact_point_id = command.contact_point_id().clone();
    let expected_version = command.expected_client_version();
    let write = prepare_protected_contact(actor, role, protector, command).await?;

    if let Err(error) = repository.persist_protected_contact(actor, &write).await {
        if error.class() == ClientPortErrorClass::Conflict {
            match repository
                .decide_client_contact_replay(actor, CLIENT_CONTACT_UPSERT_COMMAND, write.evidence())
                .await
                .map_err(map_repository_error)?
            {
                ClientReplayDecision::Replay(receipt) => {
                    return Ok(ContactMutationOutcome::Replayed(receipt));
                }
                ClientReplayDecision::Conflict | ClientReplayDecision::Miss => {}
            }
        }
        return Err(map_repository_error(error));
    }

    Ok(ContactMutationOutcome::Applied {
        client_id,
        contact_point_id,
        client_version: expected_version
            .next()
            .map_err(|_| ContactApplicationError::InternalFailure)?,
    })
}

pub async fn execute_archive_contact<R>(
    actor: &ActorContext,
    role: MembershipRole,
    repository: &R,
    command: ArchiveContactCommand,
) -> Result<ContactMutationOutcome, ContactApplicationError>
where
    R: ProtectedClientContactRepositoryPort<Error = ClientPortError>,
{
    authorize_contact_mutation(role)?;
    match repository
        .decide_client_contact_replay(actor, CLIENT_CONTACT_ARCHIVE_COMMAND, command.evidence())
        .await
        .map_err(map_repository_error)?
    {
        ClientReplayDecision::Replay(receipt) => {
            return Ok(ContactMutationOutcome::Replayed(receipt));
        }
        ClientReplayDecision::Conflict => return Err(ContactApplicationError::Conflict),
        ClientReplayDecision::Miss => {}
    }

    let current = repository
        .load_client_for_contact_mutation(actor.tenant_scope(), command.client_id())
        .await
        .map_err(map_repository_error)?
        .ok_or(ContactApplicationError::NotFound)?;
    validate_client_for_contact(&current, command.expected_client_version())?;

    let client_id = command.client_id().clone();
    let contact_point_id = command.contact_point_id().clone();
    let expected_version = command.expected_client_version();
    let write = ArchiveContactWrite::new(
        client_id.clone(),
        contact_point_id.clone(),
        command.kind(),
        expected_version,
        command.evidence().clone(),
        EVENT_PAYLOAD,
    );

    if let Err(error) = repository.archive_contact(actor, &write).await {
        if error.class() == ClientPortErrorClass::Conflict {
            match repository
                .decide_client_contact_replay(actor, CLIENT_CONTACT_ARCHIVE_COMMAND, write.evidence())
                .await
                .map_err(map_repository_error)?
            {
                ClientReplayDecision::Replay(receipt) => {
                    return Ok(ContactMutationOutcome::Replayed(receipt));
                }
                ClientReplayDecision::Conflict | ClientReplayDecision::Miss => {}
            }
        }
        return Err(map_repository_error(error));
    }

    Ok(ContactMutationOutcome::Applied {
        client_id,
        contact_point_id,
        client_version: expected_version
            .next()
            .map_err(|_| ContactApplicationError::InternalFailure)?,
    })
}

pub async fn prepare_protected_contact<P: ContactProtectionPort>(
    actor: &ActorContext,
    role: MembershipRole,
    protector: &P,
    command: PrepareProtectedContactCommand,
) -> Result<ProtectedContactWrite, ContactApplicationError> {
    authorize_contact_mutation(role)?;

    let PrepareProtectedContactCommand {
        client_id,
        contact_point_id,
        expected_client_version,
        kind,
        value,
        evidence,
    } = command;
    let normalization_version = ContactNormalizationVersion::V1;
    let protection_version = ContactProtectionVersion::V1;
    let normalized = normalize_contact_value(kind, normalization_version, value.expose())
        .map_err(|_| ContactApplicationError::InvalidRequest)?;
    let hmac_input = exact_lookup_hmac_input(
        actor.tenant_scope().tenant_id(),
        kind,
        normalization_version,
        &normalized,
    );

    let encrypted = protector
        .encrypt_contact_display(ContactEncryptionRequest::new(
            actor.tenant_scope().tenant_id(),
            &contact_point_id,
            protection_version,
            &normalized,
        ))
        .await
        .map_err(map_protection_error)?;
    let exact_lookup = protector
        .derive_exact_lookup_token(ContactExactLookupRequest::new(
            actor.tenant_scope().tenant_id(),
            kind,
            normalization_version,
            &hmac_input,
        ))
        .await
        .map_err(map_protection_error)?;

    let protected = ProtectedContactPoint::new(
        contact_point_id,
        kind,
        ContactStatus::Active,
        normalization_version,
        protection_version,
        encrypted,
        exact_lookup,
    );
    Ok(ProtectedContactWrite::new(
        client_id,
        expected_client_version,
        protected,
        evidence,
        EVENT_PAYLOAD,
    ))
}

fn validate_client_for_contact(
    client: &client_domain::ClientRecord,
    expected_version: AggregateVersion,
) -> Result<(), ContactApplicationError> {
    if client.version() != expected_version {
        return Err(ContactApplicationError::VersionConflict);
    }
    if client.status() != ClientStatus::Active {
        return Err(ContactApplicationError::InvalidState);
    }
    Ok(())
}

fn map_protection_error(error: ContactProtectionPortError) -> ContactApplicationError {
    match error.class() {
        ContactProtectionPortErrorClass::KeyUnavailable => ContactApplicationError::KeyUnavailable,
        ContactProtectionPortErrorClass::InvalidProtectedValue => {
            ContactApplicationError::IntegrityFailure
        }
        ContactProtectionPortErrorClass::InternalFailure => {
            ContactApplicationError::InternalFailure
        }
    }
}

fn map_repository_error(error: ClientPortError) -> ContactApplicationError {
    match error.class() {
        ClientPortErrorClass::NotFound => ContactApplicationError::NotFound,
        ClientPortErrorClass::VersionConflict => ContactApplicationError::VersionConflict,
        ClientPortErrorClass::InvalidState => ContactApplicationError::InvalidState,
        ClientPortErrorClass::Conflict => ContactApplicationError::Conflict,
        ClientPortErrorClass::IntegrityFailure => ContactApplicationError::IntegrityFailure,
        ClientPortErrorClass::InternalFailure => ContactApplicationError::InternalFailure,
        ClientPortErrorClass::DependencyUnavailable => ContactApplicationError::DependencyUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveContactCommand, ContactApplicationError, ContactMutationOutcome,
        PrepareProtectedContactCommand, TransientContactValue, execute_archive_contact,
        execute_upsert_contact, prepare_protected_contact,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::clients::{
        ArchiveContactWrite, ClientPortError, ClientPortErrorClass, ClientReplayDecision,
        ClientReplayReceipt, ContactEncryptionKeyDomain, ContactEncryptionRequest,
        ContactExactLookupRequest, ContactLookupKeyDomain, ContactProtectionPort,
        ContactProtectionPortError, ProtectedClientContactRepositoryPort, ProtectedContactWrite,
    };
    use client_domain::{
        ClientKind, ClientRecord, ClientStatus, ContactKind, EncryptedContactValue,
        EncryptionKeyVersion, ExactLookupToken, LookupKeyVersion,
    };
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, ContactPointId,
        CorrelationId, IdempotencyKey, OutboxEventId, TenantId, TenantScope, UnixMillis,
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
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakeProtector {
        encrypt_calls: Cell<u32>,
        lookup_calls: Cell<u32>,
        normalized_seen: RefCell<Option<String>>,
        lookup_input_seen: RefCell<Option<Vec<u8>>>,
    }

    impl FakeProtector {
        fn new() -> Self {
            Self {
                encrypt_calls: Cell::new(0),
                lookup_calls: Cell::new(0),
                normalized_seen: RefCell::new(None),
                lookup_input_seen: RefCell::new(None),
            }
        }
    }

    impl ContactProtectionPort for FakeProtector {
        async fn encrypt_contact_display(
            &self,
            request: ContactEncryptionRequest<'_>,
        ) -> Result<EncryptedContactValue, ContactProtectionPortError> {
            self.encrypt_calls.set(self.encrypt_calls.get() + 1);
            assert_eq!(
                request.key_domain(),
                ContactEncryptionKeyDomain::ClientContactDisplay
            );
            self.normalized_seen
                .replace(Some(request.normalized_value().expose().to_owned()));
            EncryptedContactValue::new(
                vec![1, 2, 3, 4],
                vec![9, 8, 7],
                EncryptionKeyVersion::new(1).map_err(|_| {
                    ContactProtectionPortError::new(
                        application_ports::clients::ContactProtectionPortErrorClass::InternalFailure,
                    )
                })?,
            )
            .map_err(|_| {
                ContactProtectionPortError::new(
                    application_ports::clients::ContactProtectionPortErrorClass::InternalFailure,
                )
            })
        }

        async fn derive_exact_lookup_token(
            &self,
            request: ContactExactLookupRequest<'_>,
        ) -> Result<ExactLookupToken, ContactProtectionPortError> {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            assert_eq!(
                request.key_domain(),
                ContactLookupKeyDomain::TenantExactLookup
            );
            self.lookup_input_seen
                .replace(Some(request.hmac_input().expose_bytes().to_vec()));
            let key_version = LookupKeyVersion::new(1).map_err(|_| {
                ContactProtectionPortError::new(
                    application_ports::clients::ContactProtectionPortErrorClass::InternalFailure,
                )
            })?;
            Ok(ExactLookupToken::new([7_u8; 32], key_version))
        }
    }

    #[derive(Default)]
    struct FakeRepository {
        replay: RefCell<Vec<ClientReplayDecision>>,
        load_calls: Cell<u32>,
        persist_calls: Cell<u32>,
        archive_calls: Cell<u32>,
        current: RefCell<Option<ClientRecord>>,
        persist_error: Cell<Option<ClientPortErrorClass>>,
        archive_error: Cell<Option<ClientPortErrorClass>>,
    }

    impl FakeRepository {
        fn with_current(current: ClientRecord) -> Self {
            Self {
                current: RefCell::new(Some(current)),
                ..Self::default()
            }
        }

        fn push_replay(&self, decision: ClientReplayDecision) {
            self.replay.borrow_mut().push(decision);
        }

        fn next_replay(&self) -> ClientReplayDecision {
            if self.replay.borrow().is_empty() {
                ClientReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            }
        }
    }

    impl ProtectedClientContactRepositoryPort for FakeRepository {
        type Error = ClientPortError;

        async fn load_client_for_contact_mutation(
            &self,
            _scope: &TenantScope,
            _client_id: &ClientId,
        ) -> Result<Option<ClientRecord>, Self::Error> {
            self.load_calls.set(self.load_calls.get() + 1);
            Ok(self.current.borrow().clone())
        }

        async fn decide_client_contact_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, Self::Error> {
            Ok(self.next_replay())
        }

        async fn persist_protected_contact(
            &self,
            _actor: &ActorContext,
            _write: &ProtectedContactWrite,
        ) -> Result<(), Self::Error> {
            self.persist_calls.set(self.persist_calls.get() + 1);
            match self.persist_error.get() {
                Some(class) => Err(ClientPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn archive_contact(
            &self,
            _actor: &ActorContext,
            _write: &ArchiveContactWrite,
        ) -> Result<(), Self::Error> {
            self.archive_calls.set(self.archive_calls.get() + 1);
            match self.archive_error.get() {
                Some(class) => Err(ClientPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JCONTACT")?),
            ActorId::parse("actor_01JCONTACT")?,
            CorrelationId::parse("corr_01JCONTACT")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JCONTACT")?,
            "digest_01JCONTACT",
            AuditEventId::parse("audit_01JCONTACT")?,
            OutboxEventId::parse("outbox_01JCONTACT")?,
            UnixMillis::new(10),
            UnixMillis::new(20),
        ))
    }

    fn command(value: &str) -> Result<PrepareProtectedContactCommand, Box<dyn std::error::Error>> {
        Ok(PrepareProtectedContactCommand::new(
            ClientId::parse("client_01JCONTACT")?,
            ContactPointId::parse("contact_01JCONTACT")?,
            AggregateVersion::INITIAL,
            ContactKind::Email,
            TransientContactValue::new(value),
            evidence()?,
        ))
    }

    fn archive_command() -> Result<ArchiveContactCommand, Box<dyn std::error::Error>> {
        Ok(ArchiveContactCommand::new(
            ClientId::parse("client_01JCONTACT")?,
            ContactPointId::parse("contact_01JCONTACT")?,
            AggregateVersion::INITIAL,
            ContactKind::Email,
            evidence()?,
        ))
    }

    fn active_client() -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::restore(
            TenantId::parse("tenant_01JCONTACT")?,
            ClientId::parse("client_01JCONTACT")?,
            AggregateVersion::INITIAL,
            ClientKind::Person,
            "Person",
            ClientStatus::Active,
        )?)
    }

    fn replay_receipt() -> ClientReplayReceipt {
        ClientReplayReceipt::new("contact_saved", Some("contact_01JCONTACT".to_owned()))
    }

    #[test]
    fn authorization_stops_before_plaintext_processing_or_protection()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let result = block_on(prepare_protected_contact(
            &actor()?,
            MembershipRole::Member,
            &protector,
            command("Person@Example.COM")?,
        ));
        assert!(matches!(result, Err(ContactApplicationError::NotFound)));
        assert_eq!(protector.encrypt_calls.get(), 0);
        assert_eq!(protector.lookup_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn plaintext_is_transient_and_persistence_write_is_protected_only()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let write = block_on(prepare_protected_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &protector,
            command(" Person@Example.COM ")?,
        ))?;
        assert_eq!(
            protector.normalized_seen.borrow().as_deref(),
            Some("person@example.com")
        );
        assert_eq!(write.contact().kind(), ContactKind::Email);
        assert_eq!(write.contact().display_value().ciphertext(), &[1, 2, 3, 4]);
        assert_eq!(write.contact().exact_lookup().bytes(), &[7_u8; 32]);
        assert!(!format!("{write:?}").contains("Person@Example.COM"));
        Ok(())
    }

    #[test]
    fn lookup_input_is_tenant_and_domain_bound_before_protector()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let _write = block_on(prepare_protected_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &protector,
            command("person@example.com")?,
        ))?;
        let seen = protector.lookup_input_seen.borrow();
        let seen = seen.as_deref().ok_or("missing lookup input")?;
        assert!(seen.starts_with(b"client-contact-exact-lookup\0v1\0"));
        assert!(
            seen.windows(b"tenant_01JCONTACT".len())
                .any(|window| window == b"tenant_01JCONTACT")
        );
        Ok(())
    }

    #[test]
    fn exact_replay_short_circuits_before_load_and_crypto()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let repository = FakeRepository::with_current(active_client()?);
        repository.push_replay(ClientReplayDecision::Replay(replay_receipt()));
        let outcome = block_on(execute_upsert_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &protector,
            &repository,
            command("person@example.com")?,
        ))?;
        assert!(matches!(outcome, ContactMutationOutcome::Replayed(_)));
        assert_eq!(repository.load_calls.get(), 0);
        assert_eq!(repository.persist_calls.get(), 0);
        assert_eq!(protector.encrypt_calls.get(), 0);
        assert_eq!(protector.lookup_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn version_conflict_stops_before_crypto_and_persist()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let mut current = active_client()?;
        current.rename("Renamed")?;
        let repository = FakeRepository::with_current(current);
        let result = block_on(execute_upsert_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &protector,
            &repository,
            command("person@example.com")?,
        ));
        assert!(matches!(result, Err(ContactApplicationError::VersionConflict)));
        assert_eq!(repository.persist_calls.get(), 0);
        assert_eq!(protector.encrypt_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn post_write_conflict_resolves_exact_replay_without_second_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let protector = FakeProtector::new();
        let repository = FakeRepository::with_current(active_client()?);
        repository.persist_error.set(Some(ClientPortErrorClass::Conflict));
        repository.push_replay(ClientReplayDecision::Miss);
        repository.push_replay(ClientReplayDecision::Replay(replay_receipt()));
        let outcome = block_on(execute_upsert_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &protector,
            &repository,
            command("person@example.com")?,
        ))?;
        assert!(matches!(outcome, ContactMutationOutcome::Replayed(_)));
        assert_eq!(repository.persist_calls.get(), 1);
        assert_eq!(protector.encrypt_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn archive_is_application_owned_and_replay_neutral()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository = FakeRepository::with_current(active_client()?);
        repository.push_replay(ClientReplayDecision::Miss);
        let outcome = block_on(execute_archive_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &repository,
            archive_command()?,
        ))?;
        assert!(matches!(outcome, ContactMutationOutcome::Applied { .. }));
        assert_eq!(repository.archive_calls.get(), 1);

        let replay_repository = FakeRepository::with_current(active_client()?);
        replay_repository.push_replay(ClientReplayDecision::Replay(ClientReplayReceipt::new(
            "contact_archived",
            Some("contact_01JCONTACT".to_owned()),
        )));
        let replay = block_on(execute_archive_contact(
            &actor()?,
            MembershipRole::TenantOwner,
            &replay_repository,
            archive_command()?,
        ))?;
        assert!(matches!(replay, ContactMutationOutcome::Replayed(_)));
        assert_eq!(replay_repository.load_calls.get(), 0);
        assert_eq!(replay_repository.archive_calls.get(), 0);
        Ok(())
    }
}