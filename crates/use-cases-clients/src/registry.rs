use application_ports::client_registry::{
    ClientRegistryHistoryProjection, ClientRegistryListItem, ClientRegistryProjectionError,
    ClientRegistryProjectionErrorClass, ClientRegistryProjectionPort,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ClientId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientRegistryQueryError {
    NotFound,
    IntegrityFailure,
    DependencyUnavailable,
}

impl fmt::Display for ClientRegistryQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client registry resource not found",
            Self::IntegrityFailure => "client registry projection integrity failure",
            Self::DependencyUnavailable => "client registry projection dependency unavailable",
        })
    }
}

impl std::error::Error for ClientRegistryQueryError {}

pub async fn list_visible_clients<P: ClientRegistryProjectionPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
) -> Result<Vec<ClientRegistryListItem>, ClientRegistryQueryError> {
    port.list_visible_clients(actor.tenant_scope(), actor.actor_id(), role)
        .await
        .map_err(map_projection_error)
}

pub async fn get_visible_client_history<P: ClientRegistryProjectionPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    client_id: &ClientId,
) -> Result<ClientRegistryHistoryProjection, ClientRegistryQueryError> {
    port.load_visible_client_history(actor.tenant_scope(), actor.actor_id(), role, client_id)
        .await
        .map_err(map_projection_error)?
        .ok_or(ClientRegistryQueryError::NotFound)
}

fn map_projection_error(error: ClientRegistryProjectionError) -> ClientRegistryQueryError {
    match error.class() {
        ClientRegistryProjectionErrorClass::IntegrityFailure => {
            ClientRegistryQueryError::IntegrityFailure
        }
        ClientRegistryProjectionErrorClass::DependencyUnavailable => {
            ClientRegistryQueryError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientRegistryQueryError, get_visible_client_history, list_visible_clients};
    use application_ports::client_registry::{
        ClientRegistryHistoryProjection, ClientRegistryListItem, ClientRegistryProjectionError,
        ClientRegistryProjectionErrorClass, ClientRegistryProjectionPort,
    };
    use client_domain::{ClientKind, ClientStatus};
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, ClientId, CorrelationId, TenantId, TenantScope,
    };
    use std::cell::Cell;
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

    struct FakePort {
        visible: bool,
        fail: Cell<Option<ClientRegistryProjectionErrorClass>>,
    }

    impl ClientRegistryProjectionPort for FakePort {
        async fn list_visible_clients(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _role: MembershipRole,
        ) -> Result<Vec<ClientRegistryListItem>, ClientRegistryProjectionError> {
            if let Some(class) = self.fail.get() {
                return Err(ClientRegistryProjectionError::new(class));
            }
            Ok(if self.visible {
                vec![ClientRegistryListItem::new(
                    ClientId::parse("client_01JREGISTRY").expect("valid test client identifier"),
                    ClientKind::Person,
                    "Registry Client",
                    ClientStatus::Active,
                    AggregateVersion::INITIAL,
                )]
            } else {
                Vec::new()
            })
        }

        async fn load_visible_client_history(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _role: MembershipRole,
            _client_id: &ClientId,
        ) -> Result<Option<ClientRegistryHistoryProjection>, ClientRegistryProjectionError>
        {
            if let Some(class) = self.fail.get() {
                return Err(ClientRegistryProjectionError::new(class));
            }
            Ok(self
                .visible
                .then(|| ClientRegistryHistoryProjection::new(Vec::new(), Vec::new(), Vec::new())))
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JREGISTRY")?),
            ActorId::parse("actor_01JREGISTRY")?,
            CorrelationId::parse("corr_01JREGISTRY")?,
        ))
    }

    #[test]
    fn invisible_history_is_neutral_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort {
            visible: false,
            fail: Cell::new(None),
        };
        let result = block_on(get_visible_client_history(
            &actor()?,
            MembershipRole::Member,
            &port,
            &ClientId::parse("client_01JREGISTRY")?,
        ));
        assert_eq!(result, Err(ClientRegistryQueryError::NotFound));
        Ok(())
    }

    #[test]
    fn visible_list_is_returned_without_authorization_reconstruction_in_transport()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort {
            visible: true,
            fail: Cell::new(None),
        };
        let items = block_on(list_visible_clients(
            &actor()?,
            MembershipRole::Member,
            &port,
        ))?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].client_id().as_str(), "client_01JREGISTRY");
        Ok(())
    }

    #[test]
    fn projection_failures_keep_integrity_and_dependency_classes()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort {
            visible: true,
            fail: Cell::new(Some(ClientRegistryProjectionErrorClass::IntegrityFailure)),
        };
        assert_eq!(
            block_on(list_visible_clients(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
            )),
            Err(ClientRegistryQueryError::IntegrityFailure)
        );
        port.fail.set(Some(
            ClientRegistryProjectionErrorClass::DependencyUnavailable,
        ));
        assert_eq!(
            block_on(list_visible_clients(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
            )),
            Err(ClientRegistryQueryError::DependencyUnavailable)
        );
        Ok(())
    }
}
