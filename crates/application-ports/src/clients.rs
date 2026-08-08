use crate::commands::CommandExecutionEvidence;
use client_domain::{ClientKind, ClientRecord, ClientStatus};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ActorId, AggregateVersion, ClientId, TenantScope};

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
