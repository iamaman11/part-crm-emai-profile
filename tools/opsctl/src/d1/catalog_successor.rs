use super::catalog_legacy;
use super::model::{ComponentAuthority, D1Error, MigrationClass, MigrationContract, RolloutOrder};
use crate::canonical::{canonical_json, canonical_pretty_json, sha256_hex};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

// Keep the accepted v1 successor implementation byte-for-byte as predecessor provenance. The current
// module is the sole live Catalog owner and composes one bounded successor generation on top of it.
#[path = "catalog_successor_v1.rs"]
mod predecessor;

const HISTORY_DIGEST_ALGORITHM: &str = "sha256(canonical-json(name+sha256))";
const LEGACY_ROOT: &str = "migrations/d1";
const PREDECESSOR_SUCCESSOR_ROOT: &str = "migrations/d1-successor";
const CURRENT_SUCCESSOR_ROOT: &str = "migrations/d1-successor-v2";
const SUCCESSOR_LINEAGE_ID: &str = "catalog-successor-v2";
const PREDECESSOR_CONTRACT_REVISION: &str = "0032_pas2_payload_fingerprint_contract.sql";
const BRIDGE_ENROLLMENT_REVISION: &str = "0032_bridge_device_enrollment_authority.sql";
const SUCCESSOR_CONTRACT_REVISION: &str = "0033_pas2_payload_fingerprint_contract.sql";
const CURRENT_SUCCESSOR_FILES: [&str; 2] =
    [BRIDGE_ENROLLMENT_REVISION, SUCCESSOR_CONTRACT_REVISION];

#[derive(Debug, Clone)]
struct CatalogSuccessor {
    authority: ComponentAuthority,
    migration_source_roots: Vec<String>,
    historical_epoch: Value,
    legacy_history: Value,
}

impl CatalogSuccessor {
    fn load(root: &Path) -> Result<Self, D1Error> {
        // Loading the predecessor also mechanically validates the immutable legacy 0001..0031
        // boundary and the accepted v1 successor directory before v2 is composed.
        let predecessor_authority = predecessor::component_authority(root, "catalog")?;
        validate_predecessor_authority(&predecessor_authority)?;
        validate_predecessor_repository_identity(root)?;
        validate_current_successor_directory(root)?;
        validate_deferred_contract_copy(root)?;

        let predecessor_projection = predecessor_catalog_projection(root)?;
        let historical_epoch = predecessor_projection
            .get("historical_epoch")
            .cloned()
            .ok_or_else(|| {
                D1Error::new("Catalog predecessor projection is missing historical_epoch")
            })?;
        let legacy_history = predecessor_projection
            .get("legacy_history")
            .cloned()
            .ok_or_else(|| {
                D1Error::new("Catalog predecessor projection is missing legacy_history")
            })?;

        let mut ordered_history = predecessor_authority.ordered_history.clone();
        if ordered_history.pop().as_deref() != Some(PREDECESSOR_CONTRACT_REVISION) {
            return Err(D1Error::new(
                "Catalog predecessor successor does not end at the accepted deferred PAS-2 CONTRACT",
            ));
        }

        let mut migration_source_roots =
            predecessor_source_roots(&predecessor_projection, &predecessor_authority)?;
        if migration_source_roots.pop().as_deref() != Some(PREDECESSOR_SUCCESSOR_ROOT) {
            return Err(D1Error::new(
                "Catalog predecessor PAS-2 CONTRACT source root drifted",
            ));
        }

        let mut post_epoch = predecessor_authority.post_epoch.clone();
        let mut deferred_contract = post_epoch.pop().ok_or_else(|| {
            D1Error::new("Catalog predecessor deferred CONTRACT metadata is missing")
        })?;
        validate_predecessor_contract(&deferred_contract)?;

        ordered_history.push(BRIDGE_ENROLLMENT_REVISION.to_owned());
        ordered_history.push(SUCCESSOR_CONTRACT_REVISION.to_owned());
        migration_source_roots.push(CURRENT_SUCCESSOR_ROOT.to_owned());
        migration_source_roots.push(CURRENT_SUCCESSOR_ROOT.to_owned());

        post_epoch.push(MigrationContract {
            migration_file: BRIDGE_ENROLLMENT_REVISION.to_owned(),
            migration_class: MigrationClass::Expand,
            rollout_order: RolloutOrder::MigrateBeforeCode,
            fail_forward_required: false,
            destructive: false,
            code_rollback_allowed: true,
            contract_preconditions: Vec::new(),
        });
        deferred_contract.migration_file = SUCCESSOR_CONTRACT_REVISION.to_owned();
        post_epoch.push(deferred_contract);

        validate_contiguous_history(&ordered_history, predecessor_authority.historical_len)?;
        let history_digest =
            successor_history_digest(root, &ordered_history, &migration_source_roots)?;
        let current_repository_revision = ordered_history
            .last()
            .cloned()
            .ok_or_else(|| D1Error::new("Catalog successor-v2 migration lineage is empty"))?;

        Ok(Self {
            authority: ComponentAuthority {
                component_id: "catalog".to_owned(),
                historical_len: predecessor_authority.historical_len,
                ordered_history,
                post_epoch,
                current_repository_revision,
                history_digest,
                policy_digest: predecessor_authority.policy_digest,
            },
            migration_source_roots,
            historical_epoch,
            legacy_history,
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
            "successor_migration_root": CURRENT_SUCCESSOR_ROOT,
            "migration_lineage": SUCCESSOR_LINEAGE_ID,
            "current_repository_revision": self.authority.current_repository_revision,
            "migration_count": self.authority.ordered_history.len(),
            "history_digest_algorithm": HISTORY_DIGEST_ALGORITHM,
            "history_digest": self.authority.history_digest,
            "compatibility_policy_digest": self.authority.policy_digest,
            "executable_migration_sources": executable_migration_sources,
            "historical_epoch": self.historical_epoch,
            "legacy_history": self.legacy_history,
            "predecessor_successor_history": {
                "migration_root": PREDECESSOR_SUCCESSOR_ROOT,
                "accepted_contract_revision": PREDECESSOR_CONTRACT_REVISION,
                "immutable": true,
                "reused_executable_revision": "0027_pas2_payload_fingerprint_expand.sql",
                "superseded_contract_revision": PREDECESSOR_CONTRACT_REVISION,
            },
            "post_epoch_migration_count": self.authority.ordered_history.len() - self.authority.historical_len,
        })
    }

    fn release_contract_projection(&self) -> Result<Value, D1Error> {
        let latest = self
            .authority
            .ordered_history
            .last()
            .ok_or_else(|| D1Error::new("Catalog successor-v2 migration lineage is empty"))?;
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
        if trailing_contract_count != 1 {
            return Err(D1Error::new(
                "Catalog successor-v2 must contain exactly one trailing SEPARATE_CONTRACT_RELEASE",
            ));
        }
        let last_contract = self
            .authority
            .post_epoch
            .last()
            .ok_or_else(|| D1Error::new("trailing contract policy is missing"))?;
        if last_contract.migration_file != *latest
            || last_contract.migration_file != SUCCESSOR_CONTRACT_REVISION
        {
            return Err(D1Error::new(
                "Catalog successor-v2 trailing contract policy does not match its latest revision",
            ));
        }
        let target = self
            .authority
            .ordered_history
            .get(self.authority.ordered_history.len().saturating_sub(2))
            .ok_or_else(|| {
                D1Error::new("trailing contract release requires an immediate predecessor revision")
            })?;
        if target != BRIDGE_ENROLLMENT_REVISION {
            return Err(D1Error::new(
                "Catalog successor-v2 deferred PAS-2 CONTRACT must immediately follow Bridge enrollment EXPAND",
            ));
        }
        Ok(json!({
            "database_component": "catalog",
            "target_schema_revision": target,
            "supported_schema_min": target,
            "supported_schema_max": latest,
            "migration_history_digest": self.authority.history_digest,
            "compatibility_policy_digest": self.authority.policy_digest,
        }))
    }

    fn pre_migration_runtime_contract_projection(&self) -> Result<Value, D1Error> {
        let historical = self
            .authority
            .ordered_history
            .get(self.authority.historical_len.saturating_sub(1))
            .ok_or_else(|| D1Error::new("Catalog historical runtime boundary is missing"))?;

        let mut supported_max = historical.as_str();
        for contract in &self.authority.post_epoch {
            let migration_before_code_compatible = matches!(
                contract.rollout_order,
                RolloutOrder::MigrateBeforeCode | RolloutOrder::Either
            ) && contract.code_rollback_allowed
                && !contract.destructive
                && !contract.fail_forward_required;
            if !migration_before_code_compatible {
                break;
            }
            supported_max = contract.migration_file.as_str();
        }

        Ok(json!({
            "database_component": "catalog",
            "target_schema_revision": historical,
            "supported_schema_min": historical,
            "supported_schema_max": supported_max,
            "migration_history_digest": self.authority.history_digest,
            "compatibility_policy_digest": self.authority.policy_digest,
        }))
    }

    fn inventory_projection(&self) -> Result<Value, D1Error> {
        let mut value = self.identity_projection();
        value["release_schema_contract"] = self.release_contract_projection()?;
        value["pre_migration_runtime_schema_contract"] =
            self.pre_migration_runtime_contract_projection()?;
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
    predecessor::component_authority(root, component)
}

pub(crate) fn release_contract(root: &Path, component: &str) -> Result<Value, D1Error> {
    if component == "catalog" {
        return CatalogSuccessor::load(root)?.release_contract_projection();
    }
    predecessor::release_contract(root, component)
}

pub(crate) fn repository_projection(root: &Path) -> Result<String, D1Error> {
    let catalog = CatalogSuccessor::load(root)?;
    let mut projection: Value = serde_json::from_str(&predecessor::repository_projection(root)?)
        .map_err(|error| {
            D1Error::new(format!("cannot parse predecessor D1 projection: {error}"))
        })?;
    let repository_identity = {
        let components = projection
            .get_mut("components")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| D1Error::new("predecessor D1 projection is missing components"))?;
        let catalog_slot = components
            .iter_mut()
            .find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
            .ok_or_else(|| {
                D1Error::new("predecessor D1 projection is missing Catalog component")
            })?;
        *catalog_slot = catalog.inventory_projection()?;
        repository_identity_from_components(components)?
    };

    projection["executable_schema_authority"] = json!([
        LEGACY_ROOT,
        PREDECESSOR_SUCCESSOR_ROOT,
        CURRENT_SUCCESSOR_ROOT,
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

fn validate_predecessor_authority(authority: &ComponentAuthority) -> Result<(), D1Error> {
    if authority.component_id != "catalog"
        || authority.current_repository_revision != PREDECESSOR_CONTRACT_REVISION
        || authority.ordered_history.last().map(String::as_str)
            != Some(PREDECESSOR_CONTRACT_REVISION)
    {
        return Err(D1Error::new(
            "accepted Catalog predecessor successor boundary drifted before successor-v2 composition",
        ));
    }
    let contract = authority
        .post_epoch
        .last()
        .ok_or_else(|| D1Error::new("Catalog predecessor contract metadata is missing"))?;
    validate_predecessor_contract(contract)
}

fn validate_predecessor_contract(contract: &MigrationContract) -> Result<(), D1Error> {
    if contract.migration_file != PREDECESSOR_CONTRACT_REVISION
        || contract.migration_class != MigrationClass::Contract
        || contract.rollout_order != RolloutOrder::SeparateContractRelease
        || !contract.fail_forward_required
        || !contract.destructive
        || contract.code_rollback_allowed
        || contract.contract_preconditions.is_empty()
    {
        return Err(D1Error::new(
            "accepted predecessor PAS-2 CONTRACT metadata drifted from its fail-forward separate-release boundary",
        ));
    }
    Ok(())
}

fn validate_predecessor_repository_identity(root: &Path) -> Result<(), D1Error> {
    let projection: Value = serde_json::from_str(&predecessor::repository_projection(root)?)
        .map_err(|error| {
            D1Error::new(format!("cannot parse predecessor D1 projection: {error}"))
        })?;
    let projected = projection
        .get("repository_identity_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("predecessor D1 projection is missing repository identity"))?;
    let computed = predecessor::repository_identity_sha256(root)?;
    if projected != computed {
        return Err(D1Error::new(
            "accepted predecessor repository identity drifted before successor-v2 composition",
        ));
    }
    Ok(())
}

fn predecessor_catalog_projection(root: &Path) -> Result<Value, D1Error> {
    let projection: Value = serde_json::from_str(&predecessor::repository_projection(root)?)
        .map_err(|error| {
            D1Error::new(format!("cannot parse predecessor D1 projection: {error}"))
        })?;
    projection
        .get("components")
        .and_then(Value::as_array)
        .and_then(|components| {
            components.iter().find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
        })
        .cloned()
        .ok_or_else(|| D1Error::new("predecessor D1 projection is missing Catalog component"))
}

fn predecessor_source_roots(
    projection: &Value,
    authority: &ComponentAuthority,
) -> Result<Vec<String>, D1Error> {
    let sources = projection
        .get("executable_migration_sources")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            D1Error::new("Catalog predecessor projection is missing executable migration sources")
        })?;
    if sources.len() != authority.ordered_history.len() {
        return Err(D1Error::new(
            "Catalog predecessor source projection cardinality drifted",
        ));
    }
    sources
        .iter()
        .zip(authority.ordered_history.iter())
        .map(|(entry, expected_name)| {
            let name = entry
                .get("migration_file")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    D1Error::new("Catalog predecessor source entry is missing migration_file")
                })?;
            let source_root = entry
                .get("source_root")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    D1Error::new("Catalog predecessor source entry is missing source_root")
                })?;
            if name != expected_name {
                return Err(D1Error::new(
                    "Catalog predecessor executable migration source order drifted",
                ));
            }
            Ok(source_root.to_owned())
        })
        .collect()
}

fn validate_contiguous_history(
    ordered_history: &[String],
    historical_len: usize,
) -> Result<(), D1Error> {
    for (index, name) in ordered_history.iter().enumerate().skip(historical_len) {
        let expected_revision = index + 1;
        let actual_revision = revision_number(name)?;
        if actual_revision != expected_revision {
            return Err(D1Error::new(format!(
                "Catalog successor-v2 typed migration order is not contiguous: expected={expected_revision:04}, actual={actual_revision:04}, file={name}"
            )));
        }
    }
    Ok(())
}

fn validate_current_successor_directory(root: &Path) -> Result<(), D1Error> {
    let directory = root.join(CURRENT_SUCCESSOR_ROOT);
    let metadata = fs::symlink_metadata(&directory).map_err(|error| {
        D1Error::new(format!(
            "cannot inspect Catalog successor-v2 migration directory {}: {error}",
            directory.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(D1Error::new(
            "Catalog successor-v2 migration root must be a real directory",
        ));
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| {
        D1Error::new(format!(
            "cannot enumerate Catalog successor-v2 migration directory: {error}"
        ))
    })? {
        let entry = entry.map_err(|error| {
            D1Error::new(format!(
                "cannot inspect Catalog successor-v2 entry: {error}"
            ))
        })?;
        let entry_metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            D1Error::new(format!(
                "cannot inspect Catalog successor-v2 migration {}: {error}",
                entry.path().display()
            ))
        })?;
        if entry_metadata.file_type().is_symlink() || !entry_metadata.is_file() {
            return Err(D1Error::new(format!(
                "Catalog successor-v2 migration root contains a non-regular file: {}",
                entry.path().display()
            )));
        }
        names.push(
            entry.file_name().into_string().map_err(|_| {
                D1Error::new("Catalog successor-v2 migration filename must be UTF-8")
            })?,
        );
    }
    names.sort();
    let expected = CURRENT_SUCCESSOR_FILES.map(str::to_owned).to_vec();
    if names != expected {
        return Err(D1Error::new(format!(
            "Catalog successor-v2 migration inventory mismatch: expected={expected:?}, actual={names:?}"
        )));
    }
    Ok(())
}

fn validate_deferred_contract_copy(root: &Path) -> Result<(), D1Error> {
    let predecessor = read_regular_repository_file(
        root,
        &format!("{PREDECESSOR_SUCCESSOR_ROOT}/{PREDECESSOR_CONTRACT_REVISION}"),
    )?;
    let current = read_regular_repository_file(
        root,
        &format!("{CURRENT_SUCCESSOR_ROOT}/{SUCCESSOR_CONTRACT_REVISION}"),
    )?;
    if predecessor != current {
        return Err(D1Error::new(
            "Catalog successor-v2 must preserve the accepted PAS-2 CONTRACT bytes while deferring its revision",
        ));
    }
    Ok(())
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
    migration_source_roots: &[String],
) -> Result<String, D1Error> {
    if ordered_history.len() != migration_source_roots.len() {
        return Err(D1Error::new(
            "Catalog successor-v2 migration source map cardinality mismatch",
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
        D1Error::new(format!(
            "cannot inspect repository root {}: {error}",
            root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(D1Error::new(
            "repository root must be a real directory, not a symlink",
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        D1Error::new(format!(
            "cannot canonicalize repository root {}: {error}",
            root.display()
        ))
    })?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        D1Error::new(format!(
            "cannot inspect migration source {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(D1Error::new(format!(
            "migration source must be a regular file: {relative}"
        )));
    }
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        D1Error::new(format!(
            "cannot canonicalize migration source {}: {error}",
            path.display()
        ))
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
        BRIDGE_ENROLLMENT_REVISION, CURRENT_SUCCESSOR_ROOT, PREDECESSOR_SUCCESSOR_ROOT,
        SUCCESSOR_CONTRACT_REVISION, component_authority, release_contract, repository_projection,
    };
    use serde_json::Value;
    use std::error::Error;
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn current_catalog_lineage_inserts_enrollment_before_deferred_contract()
    -> Result<(), Box<dyn Error>> {
        let authority = component_authority(&repository_root(), "catalog")?;
        assert_eq!(authority.ordered_history.len(), 33);
        assert_eq!(authority.ordered_history[31], BRIDGE_ENROLLMENT_REVISION);
        assert_eq!(authority.ordered_history[32], SUCCESSOR_CONTRACT_REVISION);
        assert_eq!(
            authority.current_repository_revision,
            SUCCESSOR_CONTRACT_REVISION
        );
        assert_eq!(authority.post_epoch.len(), 7);
        Ok(())
    }

    #[test]
    fn release_window_targets_enrollment_and_defers_pas2_contract() -> Result<(), Box<dyn Error>> {
        let contract = release_contract(&repository_root(), "catalog")?;
        assert_eq!(
            contract["target_schema_revision"],
            BRIDGE_ENROLLMENT_REVISION
        );
        assert_eq!(contract["supported_schema_min"], BRIDGE_ENROLLMENT_REVISION);
        assert_eq!(
            contract["supported_schema_max"],
            SUCCESSOR_CONTRACT_REVISION
        );
        Ok(())
    }

    #[test]
    fn projection_preserves_predecessor_successor_and_exposes_one_live_v2_lineage()
    -> Result<(), Box<dyn Error>> {
        let projection: Value = serde_json::from_str(&repository_projection(&repository_root())?)?;
        let catalog = projection["components"]
            .as_array()
            .and_then(|components| {
                components
                    .iter()
                    .find(|component| component["component_id"] == "catalog")
            })
            .ok_or("catalog projection is missing")?;
        assert_eq!(catalog["migration_lineage"], "catalog-successor-v2");
        assert_eq!(catalog["legacy_history"]["immutable"], true);
        assert_eq!(catalog["predecessor_successor_history"]["immutable"], true);
        let runtime = &catalog["pre_migration_runtime_schema_contract"];
        assert_eq!(
            runtime["target_schema_revision"],
            catalog["historical_epoch"]["final_revision"]
        );
        assert_eq!(runtime["supported_schema_max"], BRIDGE_ENROLLMENT_REVISION);
        let sources = catalog["executable_migration_sources"]
            .as_array()
            .ok_or("executable migration source projection is missing")?;
        assert_eq!(sources.len(), 33);
        assert_eq!(sources[26]["source_root"], PREDECESSOR_SUCCESSOR_ROOT);
        assert_eq!(sources[27]["source_root"], "migrations/d1");
        assert_eq!(sources[31]["migration_file"], BRIDGE_ENROLLMENT_REVISION);
        assert_eq!(sources[31]["source_root"], CURRENT_SUCCESSOR_ROOT);
        assert_eq!(sources[32]["migration_file"], SUCCESSOR_CONTRACT_REVISION);
        assert_eq!(sources[32]["source_root"], CURRENT_SUCCESSOR_ROOT);
        assert_eq!(
            projection["executable_schema_authority"],
            serde_json::json!([
                "migrations/d1",
                "migrations/d1-successor",
                "migrations/d1-successor-v2",
                "migrations/resolver-d1"
            ])
        );
        Ok(())
    }
}
