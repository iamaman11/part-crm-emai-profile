use crate::contact_protection::{
    ContactCryptoError, ContactNonceSource, RustCryptoContactProtection,
};
use application_ports::client_contact_lookup::{
    ContactExactLookupMatch, ContactExactLookupRepositoryPort, ContactLookupProtectionPort,
};
use application_ports::clients::{
    ClientPortError, ClientPortErrorClass, ContactExactLookupRequest, ContactProtectionPortError,
    ContactProtectionPortErrorClass,
};
use client_domain::{ContactKind, ContactNormalizationVersion, ExactLookupToken};
use profile_platform_primitives::{ClientId, ContactPointId, TenantScope};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1ContactExactLookupRepository {
    database: D1Database,
}

impl D1ContactExactLookupRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl ContactExactLookupRepositoryPort for D1ContactExactLookupRepository {
    type Error = ClientPortError;

    async fn find_active_contacts_by_exact_lookup(
        &self,
        scope: &TenantScope,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        token: &ExactLookupToken,
    ) -> Result<Vec<ContactExactLookupMatch>, Self::Error> {
        let result = query!(
            &self.database,
            r#"
            SELECT client_id, contact_point_id
            FROM client_contact_points
            WHERE tenant_id = ?
              AND kind = ?
              AND normalization_version = ?
              AND lookup_key_version = ?
              AND exact_lookup_token = ?
              AND status = 'ACTIVE'
            ORDER BY contact_point_id
            "#,
            scope.tenant_id().as_str(),
            kind.stable_code(),
            i64::from(normalization_version.value()),
            i64::from(token.key_version().value()),
            token.bytes().as_slice(),
        )
        .map_err(map_dependency_error)?
        .all()
        .await
        .map_err(map_dependency_error)?;

        result
            .results::<ExactLookupRow>()
            .map_err(map_dependency_error)?
            .into_iter()
            .map(exact_lookup_match)
            .collect()
    }
}

impl<N: ContactNonceSource> ContactLookupProtectionPort for RustCryptoContactProtection<N> {
    async fn derive_exact_lookup_candidates(
        &self,
        request: ContactExactLookupRequest<'_>,
    ) -> Result<Vec<ExactLookupToken>, ContactProtectionPortError> {
        self.derive_lookup_candidates(request.tenant_id(), request.hmac_input())
            .map_err(map_crypto_error)
    }
}

#[derive(Deserialize)]
struct ExactLookupRow {
    client_id: String,
    contact_point_id: String,
}

fn exact_lookup_match(row: ExactLookupRow) -> Result<ContactExactLookupMatch, ClientPortError> {
    let client_id = ClientId::parse(row.client_id).map_err(|_| integrity_failure())?;
    let contact_point_id =
        ContactPointId::parse(row.contact_point_id).map_err(|_| integrity_failure())?;
    Ok(ContactExactLookupMatch::new(client_id, contact_point_id))
}

fn map_dependency_error(_error: worker::Error) -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::DependencyUnavailable)
}

const fn integrity_failure() -> ClientPortError {
    ClientPortError::new(ClientPortErrorClass::IntegrityFailure)
}

fn map_crypto_error(error: ContactCryptoError) -> ContactProtectionPortError {
    let class = match error {
        ContactCryptoError::KeyUnavailable | ContactCryptoError::InvalidKeyring => {
            ContactProtectionPortErrorClass::KeyUnavailable
        }
        ContactCryptoError::InvalidProtectedValue
        | ContactCryptoError::AuthenticationFailed
        | ContactCryptoError::InvalidUtf8 => ContactProtectionPortErrorClass::InvalidProtectedValue,
        ContactCryptoError::RandomnessUnavailable | ContactCryptoError::EncryptionFailed => {
            ContactProtectionPortErrorClass::InternalFailure
        }
    };
    ContactProtectionPortError::new(class)
}

#[cfg(test)]
mod tests {
    use super::{ExactLookupRow, exact_lookup_match};

    #[test]
    fn exact_lookup_rows_restore_typed_identifiers() -> Result<(), Box<dyn std::error::Error>> {
        let restored = exact_lookup_match(ExactLookupRow {
            client_id: "client_01JLOOKUP".to_owned(),
            contact_point_id: "contact_01JLOOKUP".to_owned(),
        })?;
        assert_eq!(restored.client_id().as_str(), "client_01JLOOKUP");
        assert_eq!(restored.contact_point_id().as_str(), "contact_01JLOOKUP");
        Ok(())
    }

    #[test]
    fn malformed_exact_lookup_rows_fail_closed() {
        let result = exact_lookup_match(ExactLookupRow {
            client_id: "not a client id".to_owned(),
            contact_point_id: "contact_01JLOOKUP".to_owned(),
        });
        assert!(result.is_err());
    }
}
