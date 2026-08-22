use crate::canonical::{DEFAULT_MAX_JSON_DEPTH, parse_strict_json_with_limits};
use crate::release::model::{
    RELEASE_SET_ID_PREFIX as HISTORICAL_V2_ID_PREFIX, ReleaseModelError, ReleaseSetManifest,
};
use crate::release::v3_dto::{MAX_RELEASE_SET_V3_BYTES, decode_release_set_v3};
use crate::release::v3_output::{RELEASE_SET_V3_ID_PREFIX, render_release_set_v3};
use opsctl_core::release as core;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// One version-aware outer reader boundary for durable Release Set documents.
///
/// Current v3 documents are admitted through the strict v3 DTO and the pure semantic core.
/// Historical v2 is decoded only for compatibility/rollback continuity, then projected into the
/// same pure core. The historical decoder never authors or renders a current Release Set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedReleaseSet {
    external_schema_version: u64,
    release_set_id: String,
    display_version: Option<String>,
    semantic: core::ReleaseSetV3,
}

impl LoadedReleaseSet {
    pub fn load(path: &Path) -> Result<Self, ReleaseModelError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_SET_UNAVAILABLE: {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_SET_UNAVAILABLE: {} must be a regular file",
                path.display()
            )));
        }
        let bytes = fs::read(path).map_err(|error| {
            ReleaseModelError::new(format!(
                "RELEASE_SET_UNAVAILABLE: {}: {error}",
                path.display()
            ))
        })?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ReleaseModelError> {
        if bytes.len() > MAX_RELEASE_SET_V3_BYTES {
            return Err(ReleaseModelError::new(format!(
                "Release Set document exceeds byte budget: observed={} max={MAX_RELEASE_SET_V3_BYTES}",
                bytes.len()
            )));
        }
        let input = std::str::from_utf8(bytes).map_err(|error| {
            ReleaseModelError::new(format!("Release Set document is not UTF-8: {error}"))
        })?;
        let value = parse_strict_json_with_limits(
            input,
            MAX_RELEASE_SET_V3_BYTES,
            DEFAULT_MAX_JSON_DEPTH,
        )
        .map_err(|error| {
            ReleaseModelError::new(format!("Release Set strict JSON admission failed: {error}"))
        })?;
        let root = value
            .as_object()
            .ok_or_else(|| ReleaseModelError::new("Release Set document root must be an object"))?;
        let schema_version = root
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ReleaseModelError::new("Release Set schema_version must be an unsigned integer")
            })?;

        match schema_version {
            3 => Self::parse_v3(value),
            2 => Self::parse_historical_v2(input),
            other => Err(ReleaseModelError::new(format!(
                "unsupported Release Set schema_version: {other}; current=v3 historical=v2"
            ))),
        }
    }

    fn parse_v3(mut value: Value) -> Result<Self, ReleaseModelError> {
        let root = value
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("Release Set v3 document root disappeared"))?;
        let release_set_id = root
            .remove("release_set_id")
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .ok_or_else(|| {
                ReleaseModelError::new("Release Set v3 release_set_id must be a string")
            })?;
        if !valid_content_address(&release_set_id, RELEASE_SET_V3_ID_PREFIX) {
            return Err(ReleaseModelError::new(
                "Release Set v3 release_set_id has invalid content-address shape",
            ));
        }

        // Strict admission already rejected duplicate members. Re-encoding the representation-only
        // identity object lets the existing v3 DTO remain the single field/shape adapter; no typed
        // semantic model is serialized back into a reader and no v3 field table is duplicated here.
        let identity_bytes = serde_json::to_vec(&value).map_err(|error| {
            ReleaseModelError::new(format!(
                "cannot serialize admitted Release Set v3 identity representation: {error}"
            ))
        })?;
        let semantic = decode_release_set_v3(&identity_bytes).map_err(|error| {
            ReleaseModelError::new(format!("Release Set v3 admission failed: {error}"))
        })?;
        let rendered = render_release_set_v3(&semantic).map_err(|error| {
            ReleaseModelError::new(format!("Release Set v3 identity render failed: {error}"))
        })?;
        if rendered.release_set_id != release_set_id {
            return Err(ReleaseModelError::new(format!(
                "RELEASE_IDENTITY_MISMATCH: expected {} observed {release_set_id}",
                rendered.release_set_id
            )));
        }

        Ok(Self {
            external_schema_version: 3,
            release_set_id,
            display_version: None,
            semantic,
        })
    }

    fn parse_historical_v2(input: &str) -> Result<Self, ReleaseModelError> {
        let historical = ReleaseSetManifest::parse_json(input).map_err(|error| {
            ReleaseModelError::new(format!(
                "historical Release Set v2 decoder rejected document: {error}"
            ))
        })?;
        if !valid_content_address(&historical.release_set_id, HISTORICAL_V2_ID_PREFIX) {
            return Err(ReleaseModelError::new(
                "historical Release Set v2 release_set_id has invalid content-address shape",
            ));
        }
        let semantic = historical_v2_to_core(&historical)?;
        Ok(Self {
            external_schema_version: 2,
            release_set_id: historical.release_set_id,
            display_version: historical.display_version,
            semantic,
        })
    }

    #[must_use]
    pub const fn external_schema_version(&self) -> u64 {
        self.external_schema_version
    }

    #[must_use]
    pub fn release_set_id(&self) -> &str {
        &self.release_set_id
    }

    #[must_use]
    pub fn display_version(&self) -> Option<&str> {
        self.display_version.as_deref()
    }

    #[must_use]
    pub const fn semantic(&self) -> &core::ReleaseSetV3 {
        &self.semantic
    }

    #[must_use]
    pub const fn is_historical_v2(&self) -> bool {
        self.external_schema_version == 2
    }
}

#[must_use]
pub fn supported_release_set_id(value: &str) -> bool {
    valid_content_address(value, RELEASE_SET_V3_ID_PREFIX)
        || valid_content_address(value, HISTORICAL_V2_ID_PREFIX)
}

fn valid_content_address(value: &str, prefix: &str) -> bool {
    let Some(digest) = value.strip_prefix(prefix) else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn historical_v2_to_core(
    historical: &ReleaseSetManifest,
) -> Result<core::ReleaseSetV3, ReleaseModelError> {
    let components = historical
        .components
        .iter()
        .map(|(key, component)| {
            (
                key.clone(),
                core::ReleaseComponentIdentity {
                    component_id: component.component_id.clone(),
                    release_id: component.release_id.clone(),
                    source_commit_sha: component.source_commit_sha.clone(),
                    artifact_path: component.artifact_path.clone(),
                    artifact_sha256: component.artifact_sha256.clone(),
                    artifact_size_bytes: component.artifact_size_bytes,
                    component_manifest_sha256: component.component_manifest_sha256.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let contracts = core::ContractsIdentity {
        files: historical
            .contracts
            .files
            .iter()
            .map(|file| core::ProvenanceFileIdentity {
                path: file.path.clone(),
                sha256: file.sha256.clone(),
                size_bytes: file.size_bytes,
            })
            .collect(),
        sha256: historical.contracts.sha256.clone(),
    };
    let schema_window = |window: &crate::release::model::SchemaCompatibilityWindow| {
        core::SchemaCompatibilityWindow {
            database_component: window.database_component.clone(),
            target_schema_revision: window.target_schema_revision.clone(),
            supported_schema_min: window.supported_schema_min.clone(),
            supported_schema_max: window.supported_schema_max.clone(),
            migration_history_digest: window.migration_history_digest.clone(),
            compatibility_policy_digest: window.compatibility_policy_digest.clone(),
        }
    };

    core::ReleaseSetV3::new(core::ReleaseSetV3Parts {
        source: core::ReleaseSetSource {
            repository: historical.source.repository.clone(),
            commit_sha: historical.source.commit_sha.clone(),
            accepted_main: historical.source.accepted_main,
            accepted_main_evidence_sha256: historical.source.accepted_main_evidence_sha256.clone(),
        },
        components,
        contracts,
        protocols: core::ProtocolIdentity {
            public_api_contract_sha256: historical.protocols.public_api_contract_sha256.clone(),
            camouhost_ipc_version: historical.protocols.camouhost_ipc_version,
            profile_bridge_protocol_version: historical.protocols.profile_bridge_protocol_version,
            resolver_protocol: historical.protocols.resolver_protocol.clone(),
        },
        schemas: core::SchemaIdentity {
            d1_repository_identity_sha256: historical.schemas.d1_repository_identity_sha256.clone(),
            catalog: schema_window(&historical.schemas.catalog),
            resolver: schema_window(&historical.schemas.resolver),
        },
        runtime_compatibility: core::RuntimeCompatibilityIdentity {
            runtime_lock_sha256: historical.runtime_compatibility.runtime_lock_sha256.clone(),
            runtime_role: historical.runtime_compatibility.runtime_role.clone(),
            profile_format: historical.runtime_compatibility.profile_format.clone(),
            browser_identity_policy: historical
                .runtime_compatibility
                .browser_identity_policy
                .clone(),
        },
        capability_profile_compatibility: historical.capability_profile_compatibility.clone(),
        build_provenance: core::BuildProvenanceIdentity {
            cargo_lock_sha256: historical.build_provenance.cargo_lock_sha256.clone(),
            rust_toolchain_sha256: historical.build_provenance.rust_toolchain_sha256.clone(),
            frontend_lock_sha256: historical.build_provenance.frontend_lock_sha256.clone(),
            release_architecture_sha256: historical
                .build_provenance
                .release_architecture_sha256
                .clone(),
        },
        artifact_inventory: historical
            .artifact_inventory
            .iter()
            .map(|artifact| core::ArtifactIdentity {
                path: artifact.path.clone(),
                sha256: artifact.sha256.clone(),
                size_bytes: artifact.size_bytes,
                kind: artifact.kind.clone(),
            })
            .collect(),
    })
    .map_err(|error| {
        ReleaseModelError::new(format!(
            "historical Release Set v2 pure-core compatibility projection failed: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::LoadedReleaseSet;
    use crate::release::digest::{canonical_json, sha256_hex};
    use crate::release::model::{RELEASE_SET_ID_PREFIX, ReleaseModelError};
    use crate::release::v3_dto::{
        ArtifactIdentityDto, BuildProvenanceIdentityDto, ContractsIdentityDto, ProtocolIdentityDto,
        ProvenanceFileIdentityDto, RELEASE_SET_V3_SCHEMA_VERSION, ReleaseComponentIdentityDto,
        ReleaseSetSourceDto, ReleaseSetV3Dto, RuntimeCompatibilityIdentityDto,
        SchemaCompatibilityWindowDto, SchemaIdentityDto,
    };
    use crate::release::v3_output::render_release_set_v3;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const GIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REPO: &str = "iamaman11/part-crm-emai-profile";

    fn v3_model() -> Result<opsctl_core::release::ReleaseSetV3, Box<dyn std::error::Error>> {
        let component = |id: &str, path: &str, digest: &str| ReleaseComponentIdentityDto {
            component_id: id.to_owned(),
            release_id: format!("{id}-release-v1"),
            source_commit_sha: GIT.to_owned(),
            artifact_path: path.to_owned(),
            artifact_sha256: digest.to_owned(),
            artifact_size_bytes: 1,
            component_manifest_sha256: SHA.to_owned(),
        };
        let mut components = BTreeMap::new();
        components.insert(
            "control_plane".to_owned(),
            component("control_plane", "components/control-plane.tar", &"1".repeat(64)),
        );
        components.insert(
            "secret_resolver".to_owned(),
            component("secret_resolver", "components/secret-resolver.tar", &"2".repeat(64)),
        );
        components.insert(
            "runtime_bundle".to_owned(),
            component("runtime_bundle", "components/runtime-bundle.tar", &"3".repeat(64)),
        );
        let schema = |component: &str| SchemaCompatibilityWindowDto {
            database_component: component.to_owned(),
            target_schema_revision: "0001_initial.sql".to_owned(),
            supported_schema_min: "0001_initial.sql".to_owned(),
            supported_schema_max: "0001_initial.sql".to_owned(),
            migration_history_digest: SHA.to_owned(),
            compatibility_policy_digest: SHA.to_owned(),
        };
        Ok(ReleaseSetV3Dto {
            schema_version: RELEASE_SET_V3_SCHEMA_VERSION,
            source: ReleaseSetSourceDto {
                repository: REPO.to_owned(),
                commit_sha: GIT.to_owned(),
                accepted_main: true,
                accepted_main_evidence_sha256: SHA.to_owned(),
            },
            components,
            contracts: ContractsIdentityDto {
                files: vec![ProvenanceFileIdentityDto {
                    path: "openapi/v1/openapi.json".to_owned(),
                    sha256: SHA.to_owned(),
                    size_bytes: 1,
                }],
                sha256: SHA.to_owned(),
            },
            protocols: ProtocolIdentityDto {
                public_api_contract_sha256: SHA.to_owned(),
                camouhost_ipc_version: 1,
                profile_bridge_protocol_version: 1,
                resolver_protocol: "mailbox-secret-resolver-v1".to_owned(),
            },
            schemas: SchemaIdentityDto {
                d1_repository_identity_sha256: SHA.to_owned(),
                catalog: schema("catalog"),
                resolver: schema("resolver"),
            },
            runtime_compatibility: RuntimeCompatibilityIdentityDto {
                runtime_lock_sha256: SHA.to_owned(),
                runtime_role: "real_camoufox".to_owned(),
                profile_format: "profile-v1".to_owned(),
                browser_identity_policy: "browser-identity-v1".to_owned(),
            },
            capability_profile_compatibility: vec!["rehearsal-core-v1".to_owned()],
            build_provenance: BuildProvenanceIdentityDto {
                cargo_lock_sha256: SHA.to_owned(),
                rust_toolchain_sha256: SHA.to_owned(),
                frontend_lock_sha256: SHA.to_owned(),
                release_architecture_sha256: SHA.to_owned(),
            },
            artifact_inventory: vec![
                ArtifactIdentityDto {
                    path: "components/control-plane.tar".to_owned(),
                    sha256: "1".repeat(64),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/secret-resolver.tar".to_owned(),
                    sha256: "2".repeat(64),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
                ArtifactIdentityDto {
                    path: "components/runtime-bundle.tar".to_owned(),
                    sha256: "3".repeat(64),
                    size_bytes: 1,
                    kind: "component".to_owned(),
                },
            ],
        }
        .into_core()?)
    }

    fn historical_v2() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let accepted = sha256_hex(
            canonical_json(&json!({
                "authority":"accepted-main",
                "commit_sha":GIT,
                "repository":REPO
            }))?
            .as_bytes(),
        );
        let schema = |component: &str| json!({
            "database_component":component,
            "target_schema_revision":"0001_initial.sql",
            "supported_schema_min":"0001_initial.sql",
            "supported_schema_max":"0001_initial.sql",
            "migration_history_digest":SHA,
            "compatibility_policy_digest":SHA
        });
        let component = |id: &str, path: &str| json!({
            "release_id":id,
            "source_commit_sha":GIT,
            "artifact_path":path,
            "artifact_sha256":SHA,
            "artifact_size_bytes":1,
            "component_manifest_sha256":SHA
        });
        let mut value = json!({
            "schema_version":2,
            "release_set_id":format!("{RELEASE_SET_ID_PREFIX}{SHA}"),
            "source":{"repository":REPO,"commit_sha":GIT,"accepted_main":true,"accepted_main_evidence_sha256":accepted},
            "components":{
                "control_plane":component("cp","components/control-plane.tar"),
                "secret_resolver":component("rs","components/secret-resolver.tar"),
                "runtime_bundle":component("rt","components/runtime-bundle.tar")
            },
            "contracts":{"files":[{"path":"openapi/v1/openapi.json","sha256":SHA,"size_bytes":1}],"sha256":SHA},
            "protocols":{"public_api_contract_sha256":SHA,"camouhost_ipc_version":1,"profile_bridge_protocol_version":1,"resolver_protocol":"mailbox-secret-resolver-v1"},
            "schemas":{"d1_repository_identity_sha256":SHA,"catalog":schema("catalog"),"resolver":schema("resolver")},
            "runtime_compatibility":{"runtime_lock_sha256":SHA,"runtime_role":"real_camoufox","profile_format":"profile-v1","browser_identity_policy":"browser-identity-v1"},
            "capability_profile_compatibility":["rehearsal-core-v1"],
            "build_provenance":{"cargo_lock_sha256":SHA,"rust_toolchain_sha256":SHA,"frontend_lock_sha256":SHA,"release_architecture_sha256":SHA},
            "artifact_inventory":[
                {"path":"components/control-plane.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/secret-resolver.tar","sha256":SHA,"size_bytes":1,"kind":"component"},
                {"path":"components/runtime-bundle.tar","sha256":SHA,"size_bytes":1,"kind":"component"}
            ]
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or_else(|| ReleaseModelError::new("fixture root must be object"))?
            .remove("release_set_id");
        value["release_set_id"] = Value::String(format!(
            "{RELEASE_SET_ID_PREFIX}{}",
            sha256_hex(canonical_json(&identity)?.as_bytes())
        ));
        Ok(serde_json::to_vec(&value)?)
    }

    #[test]
    fn current_v3_round_trips_through_one_reader_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let semantic = v3_model()?;
        let rendered = render_release_set_v3(&semantic)?;
        let loaded = LoadedReleaseSet::parse(&rendered.canonical_document_bytes)?;
        assert_eq!(loaded.external_schema_version(), 3);
        assert!(!loaded.is_historical_v2());
        assert_eq!(loaded.release_set_id(), rendered.release_set_id);
        assert_eq!(loaded.semantic(), &semantic);
        Ok(())
    }

    #[test]
    fn v3_content_address_mismatch_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let rendered = render_release_set_v3(&v3_model()?)?;
        let mut value: Value = serde_json::from_slice(&rendered.canonical_document_bytes)?;
        value["release_set_id"] = Value::String(format!(
            "{}{}",
            crate::release::v3_output::RELEASE_SET_V3_ID_PREFIX,
            "f".repeat(64)
        ));
        let error = LoadedReleaseSet::parse(&serde_json::to_vec(&value)?).unwrap_err();
        assert!(error.to_string().contains("RELEASE_IDENTITY_MISMATCH"));
        Ok(())
    }

    #[test]
    fn historical_v2_is_decoder_only_but_projects_to_pure_core() -> Result<(), Box<dyn std::error::Error>> {
        let loaded = LoadedReleaseSet::parse(&historical_v2()?)?;
        assert_eq!(loaded.external_schema_version(), 2);
        assert!(loaded.is_historical_v2());
        assert_eq!(loaded.semantic().schema_version.number(), 3);
        assert_eq!(loaded.semantic().source.commit_sha, GIT);
        Ok(())
    }

    #[test]
    fn unknown_release_schema_is_rejected() {
        let error = LoadedReleaseSet::parse(br#"{"schema_version":4}"#).unwrap_err();
        assert!(error.to_string().contains("unsupported Release Set schema_version"));
    }
}
