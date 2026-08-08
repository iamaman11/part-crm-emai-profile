use crate::commands::CommandExecutionEvidence;
use client_domain::{
    ClientKind, ClientRecord, ClientStatus, ContactKind, ContactNormalizationVersion,
    ContactProtectionVersion, EncryptedContactValue, ExactLookupHmacInput, ExactLookupToken,
    NormalizedContactValue, ProtectedContactPoint,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, ClientId, ContactPointId, TenantId, TenantScope,
};

pub trait ClientRepository {
    type Error;

    fn get_client(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, Self::Error>;

    fn save_client(
        &mut self,
        actor: &ActorContext,
        client: &ClientRecord,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientCreateWrite {
    client: ClientRecord,
    requested_display_name: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ClientCreateWrite {
    #[must_use]
    pub fn new(
        client: ClientRecord,
        requested_display_name: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            client,
            requested_display_name: requested_display_name.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn client(&self) -> &ClientRecord {
        &self.client
    }

    #[must_use]
    pub fn requested_display_name(&self) -> &str {
        &self.requested_display_name
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientLifecycleWrite {
    client: ClientRecord,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ClientLifecycleWrite {
    #[must_use]
    pub fn new(
        client: ClientRecord,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            client,
            expected_version,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn client(&self) -> &ClientRecord {
        &self.client
    }

    #[must_use]
    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientReplayReceipt {
    result_code: String,
    result_reference: Option<String>,
}

impl ClientReplayReceipt {
    #[must_use]
    pub fn new(result_code: impl Into<String>, result_reference: Option<String>) -> Self {
        Self {
            result_code: result_code.into(),
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
pub enum ClientReplayDecision {
    Miss,
    Replay(ClientReplayReceipt),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientPortErrorClass {
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientPortError {
    class: ClientPortErrorClass,
}

impl ClientPortError {
    #[must_use]
    pub const fn new(class: ClientPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ClientPortErrorClass {
        self.class
    }
}

impl fmt::Display for ClientPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ClientPortErrorClass::Conflict => "client port conflict",
            ClientPortErrorClass::IntegrityFailure => "client port integrity failure",
            ClientPortErrorClass::InternalFailure => "client port internal failure",
            ClientPortErrorClass::DependencyUnavailable => "client port dependency unavailable",
        })
    }
}

impl std::error::Error for ClientPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientReadModel {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
    status: ClientStatus,
    version: AggregateVersion,
}

impl ClientReadModel {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
        status: ClientStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            client_id,
            kind,
            display_name: display_name.into(),
            status,
            version,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn kind(&self) -> ClientKind {
        self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn status(&self) -> ClientStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientGrantRole {
    Viewer,
    Editor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientGrantWrite {
    target_actor_id: ActorId,
    client_id: ClientId,
    expected_client_version: AggregateVersion,
    role: ClientGrantRole,
    reason: String,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ClientGrantWrite {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        client_id: ClientId,
        expected_client_version: AggregateVersion,
        role: ClientGrantRole,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            target_actor_id,
            client_id,
            expected_client_version,
            role,
            reason: reason.into(),
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn target_actor_id(&self) -> &ActorId {
        &self.target_actor_id
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn expected_client_version(&self) -> AggregateVersion {
        self.expected_client_version
    }

    #[must_use]
    pub const fn role(&self) -> ClientGrantRole {
        self.role
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientGrantPortErrorClass {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientGrantPortError {
    class: ClientGrantPortErrorClass,
}

impl ClientGrantPortError {
    #[must_use]
    pub const fn new(class: ClientGrantPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ClientGrantPortErrorClass {
        self.class
    }
}

impl fmt::Display for ClientGrantPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ClientGrantPortErrorClass::NotFound => "client grant not found",
            ClientGrantPortErrorClass::VersionConflict => "client grant version conflict",
            ClientGrantPortErrorClass::InvalidState => "client grant invalid state",
            ClientGrantPortErrorClass::Conflict => "client grant conflict",
            ClientGrantPortErrorClass::IntegrityFailure => "client grant integrity failure",
            ClientGrantPortErrorClass::InternalFailure => "client grant internal failure",
            ClientGrantPortErrorClass::DependencyUnavailable => {
                "client grant dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ClientGrantPortError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactEncryptionKeyDomain {
    ClientContactDisplay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactLookupKeyDomain {
    TenantExactLookup,
}

pub struct ContactEncryptionRequest<'a> {
    tenant_id: &'a TenantId,
    contact_point_id: &'a ContactPointId,
    protection_version: ContactProtectionVersion,
    normalized_value: &'a NormalizedContactValue,
}

impl<'a> ContactEncryptionRequest<'a> {
    #[must_use]
    pub const fn new(
        tenant_id: &'a TenantId,
        contact_point_id: &'a ContactPointId,
        protection_version: ContactProtectionVersion,
        normalized_value: &'a NormalizedContactValue,
    ) -> Self {
        Self {
            tenant_id,
            contact_point_id,
            protection_version,
            normalized_value,
        }
    }

    #[must_use]
    pub const fn key_domain(&self) -> ContactEncryptionKeyDomain {
        ContactEncryptionKeyDomain::ClientContactDisplay
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        self.tenant_id
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        self.contact_point_id
    }

    #[must_use]
    pub const fn protection_version(&self) -> ContactProtectionVersion {
        self.protection_version
    }

    #[must_use]
    pub const fn normalized_value(&self) -> &NormalizedContactValue {
        self.normalized_value
    }
}

pub struct ContactExactLookupRequest<'a> {
    tenant_id: &'a TenantId,
    contact_point_id: &'a ContactPointId,
    kind: ContactKind,
    normalization_version: ContactNormalizationVersion,
    hmac_input: &'a ExactLookupHmacInput,
}

impl<'a> ContactExactLookupRequest<'a> {
    #[must_use]
    pub const fn new(
        tenant_id: &'a TenantId,
        contact_point_id: &'a ContactPointId,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        hmac_input: &'a ExactLookupHmacInput,
    ) -> Self {
        Self {
            tenant_id,
            contact_point_id,
            kind,
            normalization_version,
            hmac_input,
        }
    }

    #[must_use]
    pub const fn key_domain(&self) -> ContactLookupKeyDomain {
        ContactLookupKeyDomain::TenantExactLookup
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        self.tenant_id
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        self.contact_point_id
    }

    #[must_use]
    pub const fn kind(&self) -> ContactKind {
        self.kind
    }

    #[must_use]
    pub const fn normalization_version(&self) -> ContactNormalizationVersion {
        self.normalization_version
    }

    #[must_use]
    pub const fn hmac_input(&self) -> &ExactLookupHmacInput {
        self.hmac_input
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactProtectionPortErrorClass {
    KeyUnavailable,
    InvalidProtectedValue,
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContactProtectionPortError {
    class: ContactProtectionPortErrorClass,
}

impl ContactProtectionPortError {
    #[must_use]
    pub const fn new(class: ContactProtectionPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ContactProtectionPortErrorClass {
        self.class
    }
}

impl fmt::Display for ContactProtectionPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ContactProtectionPortErrorClass::KeyUnavailable => "contact protection key unavailable",
            ContactProtectionPortErrorClass::InvalidProtectedValue => {
                "contact protector returned invalid protected value"
            }
            ContactProtectionPortErrorClass::InternalFailure => "contact protection internal failure",
        })
    }
}

impl std::error::Error for ContactProtectionPortError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedContactWrite {
    client_id: ClientId,
    expected_client_version: AggregateVersion,
    contact: ProtectedContactPoint,
    evidence: CommandExecutionEvidence,
    event_payload_json: String,
}

impl ProtectedContactWrite {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        expected_client_version: AggregateVersion,
        contact: ProtectedContactPoint,
        evidence: CommandExecutionEvidence,
        event_payload_json: impl Into<String>,
    ) -> Self {
        Self {
            client_id,
            expected_client_version,
            contact,
            evidence,
            event_payload_json: event_payload_json.into(),
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn expected_client_version(&self) -> AggregateVersion {
        self.expected_client_version
    }

    #[must_use]
    pub const fn contact(&self) -> &ProtectedContactPoint {
        &self.contact
    }

    #[must_use]
    pub const fn evidence(&self) -> &CommandExecutionEvidence {
        &self.evidence
    }

    #[must_use]
    pub fn event_payload_json(&self) -> &str {
        &self.event_payload_json
    }
}

#[allow(async_fn_in_trait)]
pub trait ClientApplicationPort {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError>;

    async fn create_client(
        &self,
        actor: &ActorContext,
        write: &ClientCreateWrite,
    ) -> Result<(), ClientPortError>;

    async fn find_visible_client(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        client_id: &ClientId,
    ) -> Result<Option<ClientReadModel>, ClientPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ClientGrantApplicationPort {
    async fn decide_client_grant_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientGrantPortError>;

    async fn grant_client(
        &self,
        actor: &ActorContext,
        write: &ClientGrantWrite,
    ) -> Result<(), ClientGrantPortError>;

    async fn revoke_client_grant(
        &self,
        actor: &ActorContext,
        write: &ClientGrantWrite,
    ) -> Result<(), ClientGrantPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ClientLifecycleApplicationPort {
    async fn load_client_for_mutation(
        &self,
        scope: &TenantScope,
        client_id: &ClientId,
    ) -> Result<Option<ClientRecord>, ClientPortError>;

    async fn decide_client_lifecycle_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ClientReplayDecision, ClientPortError>;

    async fn persist_client_lifecycle(
        &self,
        actor: &ActorContext,
        write: &ClientLifecycleWrite,
    ) -> Result<(), ClientPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ContactProtectionPort {
    async fn encrypt_contact_display(
        &self,
        request: ContactEncryptionRequest<'_>,
    ) -> Result<EncryptedContactValue, ContactProtectionPortError>;

    async fn derive_exact_lookup_token(
        &self,
        request: ContactExactLookupRequest<'_>,
    ) -> Result<ExactLookupToken, ContactProtectionPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ProtectedClientContactRepositoryPort {
    type Error;

    async fn persist_protected_contact(
        &self,
        actor: &ActorContext,
        write: &ProtectedContactWrite,
    ) -> Result<(), Self::Error>;
}
