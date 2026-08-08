use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ContactEncryptionKeyDomain, ContactEncryptionRequest, ContactExactLookupRequest,
    ContactLookupKeyDomain, ContactProtectionPort, ContactProtectionPortError,
    ContactProtectionPortErrorClass, ProtectedContactWrite,
};
use client_domain::{
    ContactKind, ContactNormalizationVersion, ContactProtectionVersion, ContactStatus,
    ProtectedContactPoint, exact_lookup_hmac_input, normalize_contact_value,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, ContactPointId};
use zeroize::Zeroize;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactApplicationError {
    NotFound,
    InvalidRequest,
    KeyUnavailable,
    IntegrityFailure,
    InternalFailure,
}

impl fmt::Display for ContactApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client contact not found",
            Self::InvalidRequest => "client contact request is invalid",
            Self::KeyUnavailable => "client contact protection key unavailable",
            Self::IntegrityFailure => "client contact protection integrity failure",
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
            &contact_point_id,
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

fn map_protection_error(error: ContactProtectionPortError) -> ContactApplicationError {
    match error.class() {
        ContactProtectionPortErrorClass::KeyUnavailable => ContactApplicationError::KeyUnavailable,
        ContactProtectionPortErrorClass::InvalidProtectedValue => {
            ContactApplicationError::IntegrityFailure
        }
        ContactProtectionPortErrorClass::InternalFailure => ContactApplicationError::InternalFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContactApplicationError, PrepareProtectedContactCommand, TransientContactValue,
        prepare_protected_contact,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::clients::{
        ContactEncryptionKeyDomain, ContactEncryptionRequest, ContactExactLookupRequest,
        ContactLookupKeyDomain, ContactProtectionPort, ContactProtectionPortError,
    };
    use client_domain::{
        ContactKind, EncryptedContactValue, EncryptionKeyVersion, ExactLookupToken, LookupKeyVersion,
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
            assert_eq!(request.key_domain(), ContactLookupKeyDomain::TenantExactLookup);
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
        assert!(seen.windows(b"tenant_01JCONTACT".len()).any(|window| window == b"tenant_01JCONTACT"));
        Ok(())
    }
}
