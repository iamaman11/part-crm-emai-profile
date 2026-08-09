use crate::clients::{ContactExactLookupRequest, ContactProtectionPortError};
use client_domain::{
    ContactKind, ContactNormalizationVersion, ExactLookupToken,
};
use profile_platform_primitives::{ClientId, ContactPointId, TenantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactExactLookupMatch {
    client_id: ClientId,
    contact_point_id: ContactPointId,
}

impl ContactExactLookupMatch {
    #[must_use]
    pub const fn new(client_id: ClientId, contact_point_id: ContactPointId) -> Self {
        Self {
            client_id,
            contact_point_id,
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
}

#[allow(async_fn_in_trait)]
pub trait ContactLookupProtectionPort {
    async fn derive_exact_lookup_candidates(
        &self,
        request: ContactExactLookupRequest<'_>,
    ) -> Result<Vec<ExactLookupToken>, ContactProtectionPortError>;
}

#[allow(async_fn_in_trait)]
pub trait ContactExactLookupRepositoryPort {
    type Error;

    async fn find_active_contacts_by_exact_lookup(
        &self,
        scope: &TenantScope,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        token: &ExactLookupToken,
    ) -> Result<Vec<ContactExactLookupMatch>, Self::Error>;
}
