use crate::canonical::{canonical_json, sha256_hex};
use crate::release::v3_dto::{MAX_RELEASE_SET_V3_BYTES, ReleaseSetV3Dto};
use opsctl_core::release as core;
use serde::Serialize;
use std::fmt::{Display, Formatter};

pub const RELEASE_SET_V3_ID_PREFIX: &str = "release-set-v3-sha256-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSetV3OutputErrorKind {
    Serialization,
    Canonicalization,
    ByteBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSetV3OutputError {
    kind: ReleaseSetV3OutputErrorKind,
    message: String,
}

impl ReleaseSetV3OutputError {
    fn new(kind: ReleaseSetV3OutputErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ReleaseSetV3OutputErrorKind {
        self.kind
    }
}

impl Display for ReleaseSetV3OutputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReleaseSetV3OutputError {}

/// External Release Set v3 document projection.
///
/// `identity` is flattened intentionally: durable v3 bytes keep the same root field inventory as
/// the semantic identity DTO and add only the derived `release_set_id`. The identifier itself is
/// excluded from the content-address scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseSetV3OutputDto {
    pub release_set_id: String,
    #[serde(flatten)]
    pub identity: ReleaseSetV3Dto,
}

/// Canonical external representation produced from one already-validated pure semantic result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedReleaseSetV3 {
    pub release_set_id: String,
    pub canonical_identity_bytes: Vec<u8>,
    pub canonical_document_bytes: Vec<u8>,
}

/// Render the pure Release Set v3 result through the single external identity boundary.
///
/// No semantic validation is reimplemented here. The pure core has already decided the Release
/// Set meaning; this adapter converts it to the versioned DTO, canonicalizes the identity bytes,
/// derives the SHA-256 content address, then canonicalizes the durable external document.
pub fn render_release_set_v3(
    release_set: &core::ReleaseSetV3,
) -> Result<RenderedReleaseSetV3, ReleaseSetV3OutputError> {
    let identity = ReleaseSetV3Dto::from(release_set);
    let canonical_identity_bytes = canonical_bytes(&identity, "Release Set v3 identity")?;
    let release_set_id = format!(
        "{RELEASE_SET_V3_ID_PREFIX}{}",
        sha256_hex(&canonical_identity_bytes)
    );
    let output = ReleaseSetV3OutputDto {
        release_set_id: release_set_id.clone(),
        identity,
    };
    let canonical_document_bytes = canonical_bytes(&output, "Release Set v3 document")?;

    Ok(RenderedReleaseSetV3 {
        release_set_id,
        canonical_identity_bytes,
        canonical_document_bytes,
    })
}

fn canonical_bytes<T: Serialize>(
    value: &T,
    label: &str,
) -> Result<Vec<u8>, ReleaseSetV3OutputError> {
    let value = serde_json::to_value(value).map_err(|error| {
        ReleaseSetV3OutputError::new(
            ReleaseSetV3OutputErrorKind::Serialization,
            format!("cannot serialize {label} DTO: {error}"),
        )
    })?;
    let canonical = canonical_json(&value).map_err(|error| {
        ReleaseSetV3OutputError::new(
            ReleaseSetV3OutputErrorKind::Canonicalization,
            format!("cannot canonicalize {label}: {error}"),
        )
    })?;
    let bytes = canonical.into_bytes();
    if bytes.len() > MAX_RELEASE_SET_V3_BYTES {
        return Err(ReleaseSetV3OutputError::new(
            ReleaseSetV3OutputErrorKind::ByteBudget,
            format!(
                "{label} exceeds byte budget: observed={} max={MAX_RELEASE_SET_V3_BYTES}",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        RELEASE_SET_V3_ID_PREFIX, ReleaseSetV3OutputErrorKind, canonical_bytes,
        render_release_set_v3,
    };
    use crate::canonical::{canonical_json, parse_strict_json, sha256_hex};
    use crate::release::v3_dto::{
        ArtifactIdentityDto, BuildProvenanceIdentityDto, ContractsIdentityDto,
        MAX_RELEASE_SET_V3_BYTES, ProtocolIdentityDto, ProvenanceFileIdentityDto,
        RELEASE_SET_V3_SCHEMA_VERSION, ReleaseComponentIdentityDto, ReleaseSetSourceDto,
        ReleaseSetV3Dto, RuntimeCompatibilityIdentityDto, SchemaCompatibilityWindowDto,
        SchemaIdentityDto,
    };
    use opsctl_core::release as core;
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn digest(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn component(
        component_id: &str,
        path: &str,
        sha256: &str,
        size_bytes: u64,
    ) -> ReleaseComponentIdentityDto {
        ReleaseComponentIdentityDto {
            component_id: component_id.to_owned(),
            release_id: format!("{component_id}-release-v1"),
            source_commit_sha: "1111111111111111111111111111111111111111".to_owned(),
            artifact_path: path.to_owned(),
            artifact_sha256: sha256.to_owned(),
            artifact_size_bytes: size_bytes,
            component_manifest_sha256: digest('e'),
        }
    }

    fn schema_window(component: &str) -> SchemaCompatibilityWindowDto {
        SchemaCompatibilityWindowDto {
            database_component: component.to_owned(),
            target_schema_revision: "0001".to_owned(),
            supported_schema_min: "0001".to_owned(),
            supported_schema_max: "0002".to_owned(),
            migration_history_digest: digest('c'),
            compatibility_policy_digest: digest('d'),
        }
    }

    fn valid_model() -> Result<core::ReleaseSetV3, String> {
        let control_sha = digest('1');
        let resolver_sha = digest('2');
        let runtime_sha = digest('3');
        let mut components = BTreeMap::new();
        components.insert(
            "control_plane".to_owned(),
            component(
                "control_plane",
                "components/control-plane.tar",
                &control_sha,
                11,
            ),
        );
        components.insert(
            "runtime_bundle".to_owned(),
            component(
                "runtime_bundle",
                "components/runtime-bundle.tar",
                &runtime_sha,
                13,
            ),
        );
        components.insert(
            "secret_resolver".to_owned(),
            component(
                "secret_resolver",
                "components/secret-resolver.tar",
                &resolver_sha,
                12,
            ),
        );

        ReleaseSetV3Dto {
            schema_version: RELEASE_SET_V3_SCHEMA_VERSION,
            source: ReleaseSetSourceDto {
                repository: core::EXPECTED_REPOSITORY.to_owned(),
                commit_sha: "1111111111111111111111111111111111111111".to_owned(),
                accepted_main: true,
                accepted_main_evidence_sha256: digest('a'),
            },
            components,
            contracts: ContractsIdentityDto {
                files: vec![ProvenanceFileIdentityDto {
                    path: "contracts/public-api.json".to_owned(),
                    sha256: digest('b'),
                    size_bytes: 7,
                }],
                sha256: digest('c'),
            },
            protocols: ProtocolIdentityDto {
                public_api_contract_sha256: digest('d'),
                camouhost_ipc_version: 1,
                profile_bridge_protocol_version: 1,
                resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
            },
            schemas: SchemaIdentityDto {
                d1_repository_identity_sha256: digest('e'),
                catalog: schema_window("catalog"),
                resolver: schema_window("resolver"),
            },
            runtime_compatibility: RuntimeCompatibilityIdentityDto {
                runtime_lock_sha256: digest('f'),
                runtime_role: "camouhost".to_owned(),
                profile_format: "profile-v1".to_owned(),
                browser_identity_policy: "browser-identity-v1".to_owned(),
            },
            capability_profile_compatibility: vec!["production-core-v1".to_owned()],
            build_provenance: BuildProvenanceIdentityDto {
                cargo_lock_sha256: digest('1'),
                rust_toolchain_sha256: digest('2'),
                frontend_lock_sha256: digest('3'),
                release_architecture_sha256: digest('4'),
            },
            artifact_inventory: vec![
                ArtifactIdentityDto {
                    path: "components/control-plane.tar".to_owned(),
                    sha256: control_sha,
                    size_bytes: 11,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/runtime-bundle.tar".to_owned(),
                    sha256: runtime_sha,
                    size_bytes: 13,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/secret-resolver.tar".to_owned(),
                    sha256: resolver_sha,
                    size_bytes: 12,
                    kind: "component".to_owned(),
                },
            ],
        }
        .into_core()
        .map_err(|error| error.to_string())
    }

    #[test]
    fn canonical_identity_is_the_only_content_address_scope() -> Result<(), String> {
        let rendered = render_release_set_v3(&valid_model()?).map_err(|error| error.to_string())?;
        let expected_id = format!(
            "{RELEASE_SET_V3_ID_PREFIX}{}",
            sha256_hex(&rendered.canonical_identity_bytes)
        );
        assert_eq!(rendered.release_set_id, expected_id);
        assert_eq!(
            rendered.release_set_id.len(),
            RELEASE_SET_V3_ID_PREFIX.len() + 64
        );

        let identity_text = std::str::from_utf8(&rendered.canonical_identity_bytes)
            .map_err(|error| error.to_string())?;
        assert!(!identity_text.contains("\"release_set_id\""));

        let document_text = std::str::from_utf8(&rendered.canonical_document_bytes)
            .map_err(|error| error.to_string())?;
        let document = parse_strict_json(document_text)?;
        let object = document
            .as_object()
            .ok_or_else(|| "rendered Release Set v3 document is not an object".to_owned())?;
        assert_eq!(
            object.get("release_set_id").and_then(Value::as_str),
            Some(rendered.release_set_id.as_str())
        );
        assert!(object.get("identity").is_none());

        let mut reconstructed_identity = object.clone();
        let removed = reconstructed_identity
            .remove("release_set_id")
            .ok_or_else(|| "rendered Release Set v3 document has no release_set_id".to_owned())?;
        assert_eq!(removed.as_str(), Some(rendered.release_set_id.as_str()));
        let reconstructed = canonical_json(&Value::Object(reconstructed_identity))?;
        assert_eq!(
            reconstructed.as_bytes(),
            rendered.canonical_identity_bytes.as_slice()
        );
        Ok(())
    }

    #[test]
    fn rendering_is_deterministic_and_keeps_v3_at_the_root() -> Result<(), String> {
        let model = valid_model()?;
        let first = render_release_set_v3(&model).map_err(|error| error.to_string())?;
        let second = render_release_set_v3(&model).map_err(|error| error.to_string())?;
        assert_eq!(first, second);

        let document_text = std::str::from_utf8(&first.canonical_document_bytes)
            .map_err(|error| error.to_string())?;
        let document = parse_strict_json(document_text)?;
        assert_eq!(
            document.get("schema_version").and_then(Value::as_u64),
            Some(RELEASE_SET_V3_SCHEMA_VERSION)
        );
        assert!(document.get("display_version").is_none());
        Ok(())
    }

    #[test]
    fn canonical_output_enforces_the_release_set_byte_budget() -> Result<(), String> {
        let oversized = "x".repeat(MAX_RELEASE_SET_V3_BYTES + 1);
        let error = canonical_bytes(&oversized, "oversized probe")
            .err()
            .ok_or_else(|| "oversized canonical output was accepted".to_owned())?;
        assert_eq!(error.kind(), ReleaseSetV3OutputErrorKind::ByteBudget);
        Ok(())
    }
}
