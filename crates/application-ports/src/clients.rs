use client_domain::ClientRecord;
use profile_platform_primitives::{ActorContext, ClientId, TenantScope};

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
