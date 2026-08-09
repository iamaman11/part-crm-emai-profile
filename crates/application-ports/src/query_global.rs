use crate::query::{QueryInputError, QueryPortError};
use crate::query_clients::ClientReadProjection;
use crate::query_mailboxes::MailboxReadProjection;
use crate::query_members::MemberReadProjection;
use crate::query_profiles::ProfileReadProjection;
use core::future::Future;
use profile_platform_primitives::{ActorContext, ActorId, ClientId, MailboxBindingId, ProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlobalSearchKey {
    Client(ClientId),
    Profile(ProfileId),
    Member(ActorId),
    Mailbox(MailboxBindingId),
}

impl GlobalSearchKey {
    pub fn parse(value: &str) -> Result<Self, QueryInputError> {
        if value.starts_with("client_") {
            return ClientId::parse(value)
                .map(Self::Client)
                .map_err(|_| QueryInputError::InvalidSearchKey);
        }
        if value.starts_with("profile_") {
            return ProfileId::parse(value)
                .map(Self::Profile)
                .map_err(|_| QueryInputError::InvalidSearchKey);
        }
        if value.starts_with("actor_") {
            return ActorId::parse(value)
                .map(Self::Member)
                .map_err(|_| QueryInputError::InvalidSearchKey);
        }
        if value.starts_with("binding_") {
            return MailboxBindingId::parse(value)
                .map(Self::Mailbox)
                .map_err(|_| QueryInputError::InvalidSearchKey);
        }
        Err(QueryInputError::InvalidSearchKey)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlobalSearchProjection {
    Client(ClientReadProjection),
    Profile(ProfileReadProjection),
    Member(MemberReadProjection),
    Mailbox(MailboxReadProjection),
}

pub trait GlobalSearchReadModelPort {
    fn search_exact(
        &self,
        actor: &ActorContext,
        key: &GlobalSearchKey,
    ) -> impl Future<Output = Result<Option<GlobalSearchProjection>, QueryPortError>>;
}

#[cfg(test)]
mod tests {
    use super::GlobalSearchKey;
    use crate::query::QueryInputError;

    #[test]
    fn global_search_accepts_only_typed_opaque_identifiers() -> Result<(), QueryInputError> {
        assert!(matches!(
            GlobalSearchKey::parse("client_01JQUERY")?,
            GlobalSearchKey::Client(_)
        ));
        assert!(matches!(
            GlobalSearchKey::parse("profile_01JQUERY")?,
            GlobalSearchKey::Profile(_)
        ));
        assert_eq!(
            GlobalSearchKey::parse("alice@example.com"),
            Err(QueryInputError::InvalidSearchKey)
        );
        Ok(())
    }
}
