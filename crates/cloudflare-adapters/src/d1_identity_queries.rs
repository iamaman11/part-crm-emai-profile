use crate::d1_identity_acl::ResolvedMembershipRole;
use profile_platform_primitives::{ActorId, ClientId, GenerationId, ProfileId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result, query};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientProjection {
    client_id: ClientId,
    kind: String,
    display_name: String,
    status: String,
    version: u64,
}

impl ClientProjection {
    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileProjection {
    profile_id: ProfileId,
    status: String,
    active_generation_id: Option<GenerationId>,
    version: u64,
    linked_client_id: Option<ClientId>,
}

impl ProfileProjection {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn active_generation_id(&self) -> Option<&GenerationId> {
        self.active_generation_id.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn linked_client_id(&self) -> Option<&ClientId> {
        self.linked_client_id.as_ref()
    }
}

pub struct D1IdentityQueryRepository {
    database: D1Database,
}

impl D1IdentityQueryRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn find_visible_client(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: ResolvedMembershipRole,
        client_id: &ClientId,
    ) -> Result<Option<ClientProjection>> {
        let owner = i32::from(role == ResolvedMembershipRole::TenantOwner);
        let row = query!(
            &self.database,
            r#"
            SELECT client_id, kind, display_name, status, version
            FROM clients AS client
            WHERE client.tenant_id = ?
              AND client.client_id = ?
              AND (
                  ? = 1
                  OR EXISTS (
                      SELECT 1
                      FROM client_grants AS grant
                      WHERE grant.tenant_id = client.tenant_id
                        AND grant.client_id = client.client_id
                        AND grant.actor_id = ?
                  )
              )
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            owner,
            actor_id.as_str()
        )?
        .first::<ClientProjectionRow>(None)
        .await?;
        row.map(client_projection).transpose()
    }

    pub async fn find_visible_profile(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: ResolvedMembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileProjection>> {
        let owner = i32::from(role == ResolvedMembershipRole::TenantOwner);
        let row = query!(
            &self.database,
            r#"
            SELECT
                profile.profile_id,
                profile.status,
                profile.active_generation_id,
                profile.version,
                assignment.client_id AS linked_client_id
            FROM browser_profiles AS profile
            LEFT JOIN profile_client_assignments AS assignment
              ON assignment.tenant_id = profile.tenant_id
             AND assignment.profile_id = profile.profile_id
             AND assignment.closed_at_ms IS NULL
            WHERE profile.tenant_id = ?
              AND profile.profile_id = ?
              AND (
                  ? = 1
                  OR EXISTS (
                      SELECT 1
                      FROM profile_grants AS grant
                      WHERE grant.tenant_id = profile.tenant_id
                        AND grant.profile_id = profile.profile_id
                        AND grant.actor_id = ?
                  )
              )
            "#,
            scope.tenant_id().as_str(),
            profile_id.as_str(),
            owner,
            actor_id.as_str()
        )?
        .first::<ProfileProjectionRow>(None)
        .await?;
        row.map(profile_projection).transpose()
    }
}

#[derive(Deserialize)]
struct ClientProjectionRow {
    client_id: String,
    kind: String,
    display_name: String,
    status: String,
    version: i64,
}

#[derive(Deserialize)]
struct ProfileProjectionRow {
    profile_id: String,
    status: String,
    active_generation_id: Option<String>,
    version: i64,
    linked_client_id: Option<String>,
}

fn client_projection(row: ClientProjectionRow) -> Result<ClientProjection> {
    Ok(ClientProjection {
        client_id: ClientId::parse(row.client_id).map_err(identifier_error)?,
        kind: row.kind,
        display_name: row.display_name,
        status: row.status,
        version: positive_version(row.version)?,
    })
}

fn profile_projection(row: ProfileProjectionRow) -> Result<ProfileProjection> {
    Ok(ProfileProjection {
        profile_id: ProfileId::parse(row.profile_id).map_err(identifier_error)?,
        status: row.status,
        active_generation_id: row
            .active_generation_id
            .map(GenerationId::parse)
            .transpose()
            .map_err(identifier_error)?,
        version: positive_version(row.version)?,
        linked_client_id: row
            .linked_client_id
            .map(ClientId::parse)
            .transpose()
            .map_err(identifier_error)?,
    })
}

fn positive_version(value: i64) -> Result<u64> {
    let value = u64::try_from(value)
        .map_err(|_| Error::RustError("negative aggregate version".to_owned()))?;
    if value == 0 {
        return Err(Error::RustError("zero aggregate version".to_owned()));
    }
    Ok(value)
}

fn identifier_error(error: profile_platform_primitives::ParseOpaqueIdError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::positive_version;

    #[test]
    fn projection_versions_are_strictly_positive() {
        assert_eq!(positive_version(1).expect("one is valid"), 1);
        assert!(positive_version(0).is_err());
        assert!(positive_version(-1).is_err());
    }
}
