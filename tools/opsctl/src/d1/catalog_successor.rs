use super::catalog_legacy;
use super::model::{
    ComponentAuthority, D1Error, MigrationClass, MigrationContract, RolloutOrder,
};
use crate::canonical::{canonical_json, canonical_pretty_json, sha256_hex};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

const HISTORY_DIGEST_ALGORITHM: &str = "sha256(canonical-json(name+sha256))";
const LEGACY_ROOT: &str = "migrations/d1";
const SUCCESSOR_ROOT: &str = "migrations/d1-successor";
const SUCCESSOR_LINEAGE_ID: &str = "catalog-successor-v1";
const HISTORICAL_FINAL_REVISION: &str = "0026_outbound_mail_intents.sql";
const HISTORICAL_MIGRATION_COUNT: usize = 26;
const HISTORICAL_ACCEPTED_DIGEST: &str =
    "4d1d8b8d3bba5d0903385d05fc18e0036628ff1123e0e26e9a080a340f7b5e2e";
const LEGACY_CURRENT_REVISION: &str = "0031_device_binding_governance.sql";
const SUCCESSOR_EXPAND_REVISION: &str = "0027_pas2_payload_fingerprint_expand.sql";
const SUCCESSOR_CONTRACT_REVISION: &str = "0032_pas2_payload_fingerprint_contract.sql";
const LEGACY_POST_EPOCH: [&str; 5] = [
    "0027_pas2_payload_fingerprint.sql",
    "0028_profile_assignment_detach.sql",
    "0029_profile_launch_authority.sql",
    "0030_profile_generation_successor_commit.sql",
    "0031_device_binding_governance.sql",
];

#[derive(Debug, Clone, Copy)]
struct MigrationSpec {
    revision: &'static str,
    migration_class: MigrationClass,
    rollout_order: RolloutOrder,
    fail_forward_required: bool,
    destructive: bool,
    code_rollback_allowed: bool,
    contract_preconditions: &'static [&'static str],
}

const CATALOG_POST_EPOCH_MIGRATIONS: &[MigrationSpec] = &[
    MigrationSpec {
        revision: SUCCESSOR_EXPAND_REVISION,
        migration_class: MigrationClass::Expand,
        rollout_order: RolloutOrder::MigrateBeforeCode,
        fail_forward_required: false,
        destructive: false,
        code_rollback_allowed: true,
        contract_preconditions: &[],
    },
    MigrationSpec {
        revision: "0028_profile_assignment_detach.sql",
        migration_class: MigrationClass::Expand,
        rollout_order: RolloutOrder::MigrateBeforeCode,
        fail_forward_required: false,
        destructive: false,
        code_rollback_allowed: true,
        contract_preconditions: &[],
    },
    MigrationSpec {
        revision: "0029_profile_launch_authority.sql",
        migration_class: MigrationClass::Expand,
        rollout_order: RolloutOrder::MigrateBeforeCode,
        fail_forward_required: false,
        destructive: false,
        code_rollback_allowed: true,
        contract_preconditions: &[],
    },
    MigrationSpec {
        revision: "0030_profile_generation_successor_commit.sql",
        migration_class: MigrationClass::Expand,
        rollout_order: RolloutOrder::MigrateBeforeCode,
        fail_forward_required: false,
        destructive: false,
        code_rollback_allowed: true,
        contract_preconditions: &[],
    },
    MigrationSpec {
        revision: LEGACY_CURRENT_REVISION,
        migration_class: MigrationClass::Expand,
        rollout_order: RolloutOrder::MigrateBeforeCode,
        fail_forward_required: false,
        destructive: false,
        code_rollback_allowed: true,
        contract_preconditions: &[],
    },
    MigrationSpec {
        revision: SUCCESSOR_CONTRACT_REVISION,
        migration_class: MigrationClass::Contract,
        rollout_order: RolloutOrder::SeparateContractRelease,
        fail_forward_required: true,
        destructive: true,
        code_rollback_allowed: false,
        contract_preconditions: &[
            "server_owned_payload_fingerprint_active",
            "request_digest_readers_writers_retired",
        ],
    },
];

impl MigrationSpec {
    fn to_contract(self) -> Result<MigrationContract, D1Error> {
        if self.destructive && self.code_rollback_allowed {
            return Err(D1Error::new(
                "destructive migration cannot claim code rollback safety",
            ));
        }
        if self.migration_class == MigrationClass::Contract
            && self.rollout_order != RolloutOrder::SeparateContractRelease
        {
            return Err(D1Error::new(
                "CONTRACT migration must use SEPARATE_CONTRACT_RELEASE",
            ));
        }
        Ok(MigrationContract {
            migration_file: self.revision.to_owned(),
            migration_class: self.migration_class,
            rollout_order: self.rollout_order,
            fail_forward_required: self.fail_forward_required,
            destructive: self.destructive,
            code_rollback_allowed: self.code_rollback_allowed,
            contract_preconditions: self
                .contract_preconditions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
struct CatalogSuccessor {
    authority: ComponentAuthority,
    migration_source_roots: Vec<&'static str>,
}

impl CatalogSuccessor {
    fn load(root: &Path) -> Result<Self, D1Error> {
        let legacy = catalog_legacy::component_authority(root, "catalog")?;
        validate_legacy_authority(&legacy)?;
        let successor_files = expected_successor_files(&legacy)?;
        validate_successor_directory(root, &successor_files)?;

        let mut ordered_history = legacy.ordered_history[..HISTORICAL_MIGRATION_COUNT].to_vec();
        ordered_history.extend(
            CATALOG_POST_EPOCH_MIGRATIONS
                .iter()
                .map(|spec| spec.revision.to_owned()),
        );
        let migration_source_roots = ordered_history
            .iter()
            .map(|name| migration_source_root(&legacy, &successor_files, name))
            .collect::<Result<Vec<_>, _>>()?;

        let history_digest = successor_history_digest(root, &ordered_history, &migration_source_roots)?;
        let policy_digest = legacy.policy_digest.clone();
        verify_policy_digest(&policy_digest)?;
        let post_epoch = CATALOG_POST_EPOCH_MIGRATIONS
            .iter()
            .copied()
            .map(MigrationSpec::to_contract)
            .collect::<Result<Vec<_>, _>>()?;
        let current_repository_revision = ordered_history
            .last()
            .cloned()
            .ok_or_else(|| D1Error::new("Catalog successor migration lineage is empty"))?;

        Ok(Self {
            authority: ComponentAuthority {
                component_id: "catalog".to_owned(),
                historical_len: HISTORICAL_MIGRATION_COUNT,
                ordered_history,
                post_epoch,
                current_repository_revision,
                history_digest,
                policy_digest,
            },
            migration_source_roots,
        })
    }

    fn identity_projection(&self) -> Value {
        let executable_migration_sources = self
            .authority
            .ordered_history
            .iter()
            .zip(self.migration_source_roots.iter())
            .map(|(migration_file, source_root)| {
                json!({
                    "migration_file": migration_file,
                    "source_root": source_root,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "component_id": "catalog",
            "migration_root": LEGACY_ROOT,
            "successor_migration_root": SUCCESSOR_ROOT,
            "migration_lineage": SUCCESSOR_LINEAGE_ID,
            "current_repository_revision": self.authority.current_repository_revision,
            "migration_count": self.authority.ordered_history.len(),
            "history_digest_algorithm": HISTORY_DIGEST_ALGORITHM,
            "history_digest": self.authority.history_digest,
            "compatibility_policy_digest": self.authority.policy_digest,
            "executable_migration_sources": executable_migration_sources,
            "historical_epoch": {
                "final_revision": HISTORICAL_FINAL_REVISION,
                "migration_count": HISTORICAL_MIGRATION_COUNT,
                "accepted_history_digest": HISTORICAL_ACCEPTED_DIGEST,
                "retroactive_runtime_compatibility_claims": false,
            },
            "legacy_history": {
                "migration_root": LEGACY_ROOT,
                "current_repository_revision": LEGACY_CURRENT_REVISION,
                "immutable": true,
                "executable_by_successor_lineage": false,
            },
            "post_epoch_migration_count": self.authority.ordered_history.len() - HISTORICAL_MIGRATION_COUNT,
        })
    }

    fn release_contract_projection(&self) -> Result<Value, D1Error> {
        let latest = self
            .authority
            .ordered_history
            .last()
            .ok_or_else(|| D1Error::new("Catalog successor migration lineage is empty"))?;
        let trailing_contract_count = self
            .authority
            .post_epoch
            .iter()
            .rev()
            .take_while(|contract| {
                contract.migration_class == MigrationClass::Contract
                    && contract.rollout_order == RolloutOrder::SeparateContractRelease
            })
            .count();
        if trailing_contract_count > 1 {
            return Err(D1Error::new(
                "Catalog release schema projection supports at most one trailing SEPARATE_CONTRACT_RELEASE",
            ));
        }
        let target = if trailing_contract_count == 1 {
            let last_contract = self
                .authority
                .post_epoch
                .last()
                .ok_or_else(|| D1Error::new("trailing contract policy is missing"))?;
            if last_contract.migration_file != *latest {
                return Err(D1Error::new(
                    "trailing contract policy does not match Catalog latest revision",
                ));
            }
            self.authority
                .ordered_history
                .get(self.authority.ordered_history.len().saturating_sub(2))
                .ok_or_else(|| {
                    D1Error::new(
                        "trailing contract release requires an immediate predecessor revision",
                    )
                })?
        } else {
            latest
        };
        Ok(json!({
            "database_component": "catalog",
            "target_schema_revision": target,
            "supported_schema_min": target,
            "supported_schema_max": latest,
            "migration_history_digest": self.authority.history_digest,
            "compatibility_policy_digest": self.authority.policy_digest,
        }))
    }

    fn inventory_projection(&self) -> Result<Value, D1Error> {
        let mut value = self.identity_projection();
        value["release_schema_contract"] = self.release_contract_projection()?;
        Ok(value)
    }
}

pub(crate) fn component_authority(
    root: &Path,
    component: &str,
) -> Result<ComponentAuthority, D1Error> {
    if component == "catalog" {
        return Ok(CatalogSuccessor::load(root)?.authority);
    }
    catalog_legacy::component_authority(root, component)
}

pub(crate) fn release_contract(root: &Path, component: &str) -> Result<Value, D1Error> {
    if component == "catalog" {
        return CatalogSuccessor::load(root)?.release_contract_projection();
    }
    catalog_legacy::release_contract(root, component)
}

pub(crate) fn repository_projection(root: &Path) -> Result<String, D1Error> {
    let catalog = CatalogSuccessor::load(root)?;
    let mut projection: Value = serde_json::from_str(&catalog_legacy::repository_projection(root)?)
        .map_err(|error| D1Error::new(format!("cannot parse legacy D1 projection: {error}")))?;
    let repository_identity = {
        let components = projection
            .get_mut("components")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| D1Error::new("legacy D1 projection is missing components"))?;
        let catalog_slot = components
            .iter_mut()
            .find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
            .ok_or_else(|| D1Error::new("legacy D1 projection is missing Catalog component"))?;
        *catalog_slot = catalog.inventory_projection()?;
        repository_identity_from_components(components)?
    };

    projection["executable_schema_authority"] = json!([
        LEGACY_ROOT,
        SUCCESSOR_ROOT,
        "migrations/resolver-d1"
    ]);
    projection["repository_identity_sha256"] = json!(repository_identity);
    canonical_pretty_json(&projection).map_err(D1Error::new)
}

pub(crate) fn repository_identity_sha256(root: &Path) -> Result<String, D1Error> {
    let projection: Value = serde_json::from_str(&repository_projection(root)?)
        .map_err(|error| D1Error::new(format!("cannot parse D1 repository projection: {error}")))?;
    projection
        .get("repository_identity_sha256")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new("D1 repository projection is missing repository identity"))
}

fn validate_legacy_authority(authority: &ComponentAuthority) -> Result<(), D1Error> {
    if authority.component_id != "catalog"
        || authority.historical_len != HISTORICAL_MIGRATION_COUNT
        || authority.ordered_history.len() != 31
        || authority.current_repository_revision != LEGACY_CURRENT_REVISION
    {
        return Err(D1Error::new(
            "accepted Catalog legacy migration lineage no longer matches the frozen 0001..0031 boundary",
        ));
    }
    if authority.ordered_history[25] != HISTORICAL_FINAL_REVISION
        || authority.ordered_history[26..31] != LEGACY_POST_EPOCH
    {
        return Err(D1Error::new(
            "accepted Catalog legacy post-epoch migration identity changed",
        ));
    }
    Ok(())
}

fn expected_successor_files(
    legacy: &ComponentAuthority,
) -> Result<Vec<&'static str>, D1Error> {
    let mut expected = Vec::new();
    for (index, spec) in CATALOG_POST_EPOCH_MIGRATIONS.iter().enumerate() {
        let expected_revision = HISTORICAL_MIGRATION_COUNT + index + 1;
        let actual_revision = revision_number(spec.revision)?;
        if actual_revision != expected_revision {
            return Err(D1Error::new(format!(
                "Catalog successor typed migration order is not contiguous: expected={expected_revision:04}, actual={actual_revision:04}, file={}",
                spec.revision
            )));
        }
        match legacy.ordered_history.get(actual_revision.saturating_sub(1)) {
            Some(legacy_name) if legacy_name == spec.revision => {}
            _ => expected.push(spec.revision),
        }
    }
    Ok(expected)
}

fn validate_successor_directory(
    root: &Path,
    expected_files: &[&str],
) -> Result<(), D1Error> {
    let directory = root.join(SUCCESSOR_ROOT);
    let metadata = fs::symlink_metadata(&directory).map_err(|error| {
        D1Error::new(format!(
            "cannot inspect Catalog successor migration directory {}: {error}",
            directory.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(D1Error::new(
            "Catalog successor migration root must be a real directory",
        ));
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| {
        D1Error::new(format!(
            "cannot enumerate Catalog successor migration directory: {error}"
        ))
    })? {
        let entry = entry.map_err(|error| {
            D1Error::new(format!("cannot inspect Catalog successor migration entry: {error}"))
        })?;
        let entry_metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            D1Error::new(format!(
                "cannot inspect Catalog successor migration {}: {error}",
                entry.path().display()
            ))
        })?;
        if entry_metadata.file_type().is_symlink() || !entry_metadata.is_file() {
            return Err(D1Error::new(format!(
                "Catalog successor migration root contains a non-regular file: {}",
                entry.path().display()
            )));
        }
        names.push(
            entry
                .file_name()
                .into_string()
                .map_err(|_| D1Error::new("Catalog successor migration filename must be UTF-8"))?,
        );
    }
    names.sort();
    let expected = expected_files
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    if names != expected {
        return Err(D1Error::new(format!(
            "Catalog successor migration inventory mismatch: expected={expected:?}, actual={names:?}"
        )));
    }
    Ok(())
}

fn migration_source_root(
    legacy: &ComponentAuthority,
    successor_files: &[&str],
    name: &str,
) -> Result<&'static str, D1Error> {
    if successor_files.iter().any(|candidate| *candidate == name) {
        return Ok(SUCCESSOR_ROOT);
    }
    let revision = revision_number(name)?;
    if legacy
        .ordered_history
        .get(revision.saturating_sub(1))
        .map(String::as_str)
        == Some(name)
    {
        return Ok(LEGACY_ROOT);
    }
    Err(D1Error::new(format!(
        "Catalog executable migration lacks a canonical source: {name}"
    )))
}

fn revision_number(name: &str) -> Result<usize, D1Error> {
    let prefix = name
        .get(..4)
        .ok_or_else(|| D1Error::new(format!("invalid Catalog migration revision: {name}")))?;
    if !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(D1Error::new(format!(
            "invalid Catalog migration revision: {name}"
        )));
    }
    prefix.parse::<usize>().map_err(|error| {
        D1Error::new(format!(
            "invalid Catalog migration revision {name}: {error}"
        ))
    })
}

fn successor_history_digest(
    root: &Path,
    ordered_history: &[String],
    migration_source_roots: &[&str],
) -> Result<String, D1Error> {
    if ordered_history.len() != migration_source_roots.len() {
        return Err(D1Error::new(
            "Catalog successor migration source map cardinality mismatch",
        ));
    }
    let identity = Value::Array(
        ordered_history
            .iter()
            .zip(migration_source_roots.iter())
            .map(|(name, source_root)| migration_identity(root, source_root, name))
            .collect::<Result<Vec<_>, _>>()?,
    );
    canonical_json(&identity)
        .map(|encoded| sha256_hex(encoded.as_bytes()))
        .map_err(D1Error::new)
}

fn migration_identity(root: &Path, migration_root: &str, name: &str) -> Result<Value, D1Error> {
    let bytes = read_regular_repository_file(root, &format!("{migration_root}/{name}"))?;
    Ok(json!({"name": name, "sha256": sha256_hex(&bytes)}))
}

fn read_regular_repository_file(root: &Path, relative: &str) -> Result<Vec<u8>, D1Error> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        D1Error::new(format!("cannot inspect repository root {}: {error}", root.display()))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(D1Error::new(
            "repository root must be a real directory, not a symlink",
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        D1Error::new(format!("cannot canonicalize repository root {}: {error}", root.display()))
    })?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        D1Error::new(format!("cannot inspect migration source {}: {error}", path.display()))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(D1Error::new(format!(
            "migration source must be a regular file: {relative}"
        )));
    }
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        D1Error::new(format!("cannot canonicalize migration source {}: {error}", path.display()))
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(D1Error::new(format!(
            "migration source escapes repository root: {relative}"
        )));
    }
    fs::read(canonical_path)
        .map_err(|error| D1Error::new(format!("cannot read migration source {relative}: {error}")))
}

fn compatibility_policy_projection() -> Value {
    json!({
        "historical_epoch_runtime_compatibility": "UNKNOWN_FAIL_CLOSED",
        "new_migrations_require_full_contract": true,
        "remote_ledger_must_be_known_canonical_order": true,
        "known_prefix_is_recoverable": true,
        "unknown_or_diverged_is_fail_closed": true,
    })
}

fn verify_policy_digest(expected: &str) -> Result<(), D1Error> {
    let actual = canonical_json(&compatibility_policy_projection())
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(D1Error::new)?;
    if actual != expected {
        return Err(D1Error::new(format!(
            "Catalog successor compatibility policy drift: legacy={expected}, successor={actual}"
        )));
    }
    Ok(())
}

fn repository_identity_from_components(components: &[Value]) -> Result<String, D1Error> {
    let mut identities = Vec::with_capacity(components.len());
    for component in components {
        let mut identity = component.clone();
        identity
            .as_object_mut()
            .ok_or_else(|| D1Error::new("D1 component projection must be an object"))?
            .remove("release_schema_contract");
        identities.push(identity);
    }
    let value = json!({
        "schema_version": 1,
        "kind": "D1_REPOSITORY_IDENTITY",
        "components": identities,
        "compatibility_policy": compatibility_policy_projection(),
    });
    canonical_json(&value)
        .map(|encoded| sha256_hex(encoded.as_bytes()))
        .map_err(D1Error::new)
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_CURRENT_REVISION, SUCCESSOR_CONTRACT_REVISION, SUCCESSOR_EXPAND_REVISION,
        component_authority, release_contract, repository_projection,
    };
    use serde_json::Value;
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn current_catalog_lineage_composes_immutable_legacy_and_successor_sql() {
        let root = repository_root();
        let authority = component_authority(&root, "catalog").expect("catalog authority");
        assert_eq!(authority.ordered_history.len(), 32);
        assert_eq!(authority.ordered_history[26], SUCCESSOR_EXPAND_REVISION);
        assert_eq!(authority.ordered_history[31], SUCCESSOR_CONTRACT_REVISION);
        assert_eq!(authority.current_repository_revision, SUCCESSOR_CONTRACT_REVISION);
        assert_eq!(authority.post_epoch.len(), 6);
    }

    #[test]
    fn release_window_stops_before_trailing_contract() {
        let root = repository_root();
        let contract = release_contract(&root, "catalog").expect("release contract");
        assert_eq!(contract["target_schema_revision"], LEGACY_CURRENT_REVISION);
        assert_eq!(contract["supported_schema_min"], LEGACY_CURRENT_REVISION);
        assert_eq!(
            contract["supported_schema_max"],
            SUCCESSOR_CONTRACT_REVISION
        );
    }

    #[test]
    fn repository_projection_exposes_one_successor_lineage_without_hiding_legacy_history() {
        let projection: Value = serde_json::from_str(
            &repository_projection(&repository_root()).expect("repository projection"),
        )
        .expect("projection json");
        let catalog = projection["components"]
            .as_array()
            .and_then(|components| {
                components
                    .iter()
                    .find(|component| component["component_id"] == "catalog")
            })
            .expect("catalog projection");
        assert_eq!(catalog["migration_lineage"], "catalog-successor-v1");
        assert_eq!(catalog["legacy_history"]["immutable"], true);
        let sources = catalog["executable_migration_sources"]
            .as_array()
            .expect("executable migration source projection");
        assert_eq!(sources.len(), 32);
        assert_eq!(sources[26]["migration_file"], SUCCESSOR_EXPAND_REVISION);
        assert_eq!(sources[26]["source_root"], "migrations/d1-successor");
        assert_eq!(sources[27]["source_root"], "migrations/d1");
        assert_eq!(sources[31]["migration_file"], SUCCESSOR_CONTRACT_REVISION);
        assert_eq!(sources[31]["source_root"], "migrations/d1-successor");
        assert_eq!(
            projection["executable_schema_authority"],
            serde_json::json!([
                "migrations/d1",
                "migrations/d1-successor",
                "migrations/resolver-d1"
            ])
        );
    }
}
