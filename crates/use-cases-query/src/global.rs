use super::{QueryApplicationError, authorize, map_port_error};
use application_ports::query::{QueryAuthorizationPort, QueryCapability};
use application_ports::query_global::{
    GlobalSearchKey, GlobalSearchProjection, GlobalSearchReadModelPort,
};
use profile_platform_primitives::ActorContext;

pub async fn search_global_exact<A, P>(
    actor: &ActorContext,
    authorization: &A,
    projection: &P,
    key: &GlobalSearchKey,
) -> Result<Option<GlobalSearchProjection>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    P: GlobalSearchReadModelPort,
{
    if !authorize(actor, authorization, QueryCapability::GlobalSearch).await? {
        return Ok(None);
    }
    projection
        .search_exact(actor, key)
        .await
        .map_err(map_port_error)
}
