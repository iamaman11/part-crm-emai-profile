use application_ports::mailbox_client_associations::{
    MailboxClientAssociationApplicationPort, MailboxClientAssociationPortError,
    MailboxClientAssociationPortErrorClass, MailboxClientAssociationVersion,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};

use crate::client_associations::{
    MailboxClientAssociationOperationError, authorize_mailbox_client_association,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociationDetails {
    binding_id: MailboxBindingId,
    client_id: Option<ClientId>,
    relationship_version: MailboxClientAssociationVersion,
    mailbox_executable: bool,
}

impl MailboxClientAssociationDetails {
    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn client_id(&self) -> Option<&ClientId> {
        self.client_id.as_ref()
    }

    #[must_use]
    pub const fn relationship_version(&self) -> MailboxClientAssociationVersion {
        self.relationship_version
    }

    #[must_use]
    pub const fn mailbox_executable(&self) -> bool {
        self.mailbox_executable
    }
}

pub async fn get_mailbox_client_association<P: MailboxClientAssociationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    binding_id: &MailboxBindingId,
) -> Result<MailboxClientAssociationDetails, MailboxClientAssociationOperationError> {
    authorize_mailbox_client_association(role)?;
    let context = port
        .load_context(actor.tenant_scope(), binding_id, None)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxClientAssociationOperationError::NotFound)?;
    let association = context.association();
    Ok(MailboxClientAssociationDetails {
        binding_id: association.binding_id().clone(),
        client_id: association.client_id().cloned(),
        relationship_version: association.version(),
        mailbox_executable: context.mailbox_executable(),
    })
}

const fn map_port_error(
    error: MailboxClientAssociationPortError,
) -> MailboxClientAssociationOperationError {
    match error.class() {
        MailboxClientAssociationPortErrorClass::NotFound => {
            MailboxClientAssociationOperationError::NotFound
        }
        MailboxClientAssociationPortErrorClass::VersionConflict => {
            MailboxClientAssociationOperationError::VersionConflict
        }
        MailboxClientAssociationPortErrorClass::InvalidState => {
            MailboxClientAssociationOperationError::InvalidState
        }
        MailboxClientAssociationPortErrorClass::Conflict => {
            MailboxClientAssociationOperationError::Conflict
        }
        MailboxClientAssociationPortErrorClass::IntegrityFailure => {
            MailboxClientAssociationOperationError::IntegrityFailure
        }
        MailboxClientAssociationPortErrorClass::InternalFailure => {
            MailboxClientAssociationOperationError::InternalFailure
        }
        MailboxClientAssociationPortErrorClass::DependencyUnavailable => {
            MailboxClientAssociationOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MailboxClientAssociationDetails;
    use application_ports::mailbox_client_associations::MailboxClientAssociationVersion;
    use profile_platform_primitives::{ClientId, MailboxBindingId};

    #[test]
    fn projection_can_represent_never_associated_state_without_credentials()
    -> Result<(), Box<dyn std::error::Error>> {
        let details = MailboxClientAssociationDetails {
            binding_id: MailboxBindingId::parse("mailbox_01JASSOCIATION")?,
            client_id: None,
            relationship_version: MailboxClientAssociationVersion::NEVER_ASSOCIATED,
            mailbox_executable: true,
        };
        assert_eq!(details.relationship_version().value(), 0);
        assert_eq!(details.client_id(), None);
        assert!(details.mailbox_executable());
        let client = ClientId::parse("client_01JASSOCIATION")?;
        assert_ne!(details.client_id(), Some(&client));
        Ok(())
    }
}
