use super::model::{
    ComponentAuthority, D1Error, Decision, LedgerState, MigrationClass, MigrationContract,
    RolloutOrder,
};
use crate::canonical::{canonical_json, sha256_hex};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const HISTORY_DIGEST_ALGORITHM: &str = "sha256(canonical-json(name+sha256))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum D1Component {
    Catalog,
    Resolver,
}

impl D1Component {
    pub(crate) fn parse(value: &str) -> Result<Self, D1Error> {
        match value {
            "catalog" => Ok(Self::Catalog),
            "resolver" => Ok(Self::Resolver),
            other => Err(D1Error::new(format!("unknown D1 component: {other}"))),
        }
    }

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Resolver => "resolver",
        }
    }

    pub(crate) const fn migration_root(self) -> &'static str {
        match self {
            Self::Catalog => "migrations/d1",
            Self::Resolver => "migrations/resolver-d1",
        }
    }

    const fn historical_epoch(self) -> HistoricalEpoch {
        match self {
            Self::Catalog => HistoricalEpoch {
                final_revision: "0026_outbound_mail_intents.sql",
                migration_count: 26,
                accepted_history_digest: "4d1d8b8d3bba5d0903385d05fc18e0036628ff1123e0e26e9a080a340f7b5e2e",
            },
            Self::Resolver => HistoricalEpoch {
                final_revision: "0004_refresh_owner_hmac_version.sql",
                migration_count: 4,
                accepted_history_digest: "98fd6f91a839223b06c441df4901dbd4fda8e69f2f90606f00e43faad91877ec",
            },
        }
    }

    const fn future_migrations(self) -> &'static [MigrationSpec] {
        match self {
            Self::Catalog | Self::Resolver => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoricalEpoch {
    final_revision: &'static str,
    migration_count: usize,
    accepted_history_digest: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MigrationSpec {
    revision: &'static str,
    migration_class: MigrationClass,
    rollout_order: RolloutOrder,
    fail_forward_required: bool,
    destructive: bool,
    code_rollback_allowed: bool,
    contract_preconditions: &'static [&'static str],
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationFile {
    revision: u32,
    name: String,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryMigrationCatalog {
    component: D1Component,
    migrations: Vec<MigrationFile>,
    history_digest: String,
    policy_digest: String,
}

impl RepositoryMigrationCatalog {
    pub(crate) fn load(root: &Path, component: D1Component) -> Result<Self, D1Error> {
        let migration_directory = checked_migration_directory(root, component)?;
        let mut migrations = Vec::new();
        for entry in fs::read_dir(&migration_directory).map_err(|error| {
            D1Error::new(format!(
                "cannot enumerate canonical D1 migration directory {}: {error}",
                migration_directory.display()
            ))
        })? {
            let entry = entry.map_err(|error| {
                D1Error::new(format!(
                    "cannot inspect canonical D1 migration entry: {error}"
                ))
            })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                D1Error::new(format!(
                    "cannot inspect canonical D1 migration {}: {error}",
                    entry.path().display()
                ))
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(D1Error::new(format!(
                    "canonical D1 migration directory contains a non-regular file: {}",
                    entry.path().display()
                )));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| D1Error::new("canonical D1 migration filename must be valid UTF-8"))?;
            let revision = parse_revision(&name)?;
            let bytes = fs::read(entry.path()).map_err(|error| {
                D1Error::new(format!(
                    "cannot read canonical D1 migration {}: {error}",
                    entry.path().display()
                ))
            })?;
            migrations.push(MigrationFile {
                revision,
                name,
                sha256: sha256_hex(&bytes),
            });
        }
        if migrations.is_empty() {
            return Err(D1Error::new(format!(
                "canonical D1 migration directory is empty: {}",
                component.migration_root()
            )));
        }
        migrations.sort_by_key(|migration| migration.revision);
        validate_revisions(&migrations)?;

        let epoch = component.historical_epoch();
        verify_historical_epoch(&migrations, epoch, component)?;
        verify_future_specs(&migrations, epoch, component.future_migrations(), component)?;

        let history_digest = migration_history_digest(&migrations)?;
        let policy_digest = compatibility_policy_digest()?;
        Ok(Self {
            component,
            migrations,
            history_digest,
            policy_digest,
        })
    }

    pub(crate) fn authority(&self) -> Result<ComponentAuthority, D1Error> {
        let epoch = self.component.historical_epoch();
        let ordered_history = self
            .migrations
            .iter()
            .map(|migration| migration.name.clone())
            .collect::<Vec<_>>();
        let current_repository_revision = ordered_history
            .last()
            .cloned()
            .ok_or_else(|| D1Error::new("canonical D1 migration history is empty"))?;
        let post_epoch = self
            .component
            .future_migrations()
            .iter()
            .copied()
            .map(MigrationSpec::to_contract)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ComponentAuthority {
            component_id: self.component.id().to_owned(),
            historical_len: epoch.migration_count,
            ordered_history,
            post_epoch,
            current_repository_revision,
            history_digest: self.history_digest.clone(),
            policy_digest: self.policy_digest.clone(),
        })
    }

    pub(crate) fn release_contract_projection(&self) -> Result<Value, D1Error> {
        let last = self
            .migrations
            .last()
            .ok_or_else(|| D1Error::new("canonical D1 migration history is empty"))?;
        Ok(json!({
            "database_component": self.component.id(),
            "target_schema_revision": last.name,
            "supported_schema_min": last.name,
            "supported_schema_max": last.name,
            "migration_history_digest": self.history_digest,
            "compatibility_policy_digest": self.policy_digest,
        }))
    }

    fn identity_projection(&self) -> Value {
        let epoch = self.component.historical_epoch();
        let current = self
            .migrations
            .last()
            .map(|migration| migration.name.as_str())
            .unwrap_or_default();
        json!({
            "component_id": self.component.id(),
            "migration_root": self.component.migration_root(),
            "current_repository_revision": current,
            "migration_count": self.migrations.len(),
            "history_digest_algorithm": HISTORY_DIGEST_ALGORITHM,
            "history_digest": self.history_digest,
            "compatibility_policy_digest": self.policy_digest,
            "historical_epoch": {
                "final_revision": epoch.final_revision,
                "migration_count": epoch.migration_count,
                "accepted_history_digest": epoch.accepted_history_digest,
                "retroactive_runtime_compatibility_claims": false,
            },
            "post_epoch_migration_count": self.migrations.len() - epoch.migration_count,
        })
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
    let component = D1Component::parse(component)?;
    RepositoryMigrationCatalog::load(root, component)?.authority()
}

pub(crate) fn repository_projection(root: &Path) -> Result<String, D1Error> {
    let catalog = RepositoryMigrationCatalog::load(root, D1Component::Catalog)?;
    let resolver = RepositoryMigrationCatalog::load(root, D1Component::Resolver)?;
    let components = vec![
        catalog.inventory_projection()?,
        resolver.inventory_projection()?,
    ];
    let identity = json!({
        "schema_version": 1,
        "kind": "D1_REPOSITORY_IDENTITY",
        "components": [catalog.identity_projection(), resolver.identity_projection()],
        "compatibility_policy": compatibility_policy_projection(),
    });
    let canonical_identity = canonical_json(&identity).map_err(D1Error::new)?;
    let migration_classes = MigrationClass::ALL.map(MigrationClass::as_str);
    let ledger_states = LedgerState::ALL.map(LedgerState::as_str);
    let rollout_orders = RolloutOrder::ALL.map(RolloutOrder::as_str);
    let rollout_decisions = Decision::ALL.map(Decision::as_str);
    let output = json!({
        "schema_version": 1,
        "kind": "D1_REPOSITORY_PROJECTION",
        "semantic_authority": "tools/opsctl/src/d1",
        "executable_schema_authority": ["migrations/d1", "migrations/resolver-d1"],
        "historical_provenance": {
            "program_slice": "AR-9",
            "tracking_issue": 366,
            "acceptance_evidence": "docs/evidence/2026-08-19-ar9-final-acceptance.json",
        },
        "migration_classes": migration_classes,
        "ledger_states": ledger_states,
        "rollout_orders": rollout_orders,
        "rollout_decisions": rollout_decisions,
        "repository_identity_sha256": sha256_hex(canonical_identity.as_bytes()),
        "components": components,
        "effect_boundary": {
            "mode": "READ_ONLY_METADATA_ONLY",
            "network": false,
            "provider_credentials": false,
            "provider_mutation": false,
            "database_mutation": false,
            "production_mutation": false,
        },
        "architecture_complete": false,
        "production_core_gate": "BLOCKED",
        "production_ready": false,
        "production_mutation": false,
    });
    crate::canonical::canonical_pretty_json(&output).map_err(D1Error::new)
}

pub(crate) fn repository_identity_sha256(root: &Path) -> Result<String, D1Error> {
    let catalog = RepositoryMigrationCatalog::load(root, D1Component::Catalog)?;
    let resolver = RepositoryMigrationCatalog::load(root, D1Component::Resolver)?;
    let identity = json!({
        "schema_version": 1,
        "kind": "D1_REPOSITORY_IDENTITY",
        "components": [catalog.identity_projection(), resolver.identity_projection()],
        "compatibility_policy": compatibility_policy_projection(),
    });
    canonical_json(&identity)
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(D1Error::new)
}

pub(crate) fn release_contract(root: &Path, component: &str) -> Result<Value, D1Error> {
    RepositoryMigrationCatalog::load(root, D1Component::parse(component)?)?
        .release_contract_projection()
}

fn checked_migration_directory(root: &Path, component: D1Component) -> Result<PathBuf, D1Error> {
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
    let path = root.join(component.migration_root());
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        D1Error::new(format!(
            "cannot inspect canonical D1 migration directory {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(D1Error::new(format!(
            "canonical D1 migration path must be a real directory: {}",
            component.migration_root()
        )));
    }
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        D1Error::new(format!(
            "cannot canonicalize D1 migration directory {}: {error}",
            path.display()
        ))
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(D1Error::new(
            "canonical D1 migration directory escapes repository root",
        ));
    }
    Ok(canonical_path)
}

fn parse_revision(name: &str) -> Result<u32, D1Error> {
    if name.len() < 10 || !name.ends_with(".sql") {
        return Err(D1Error::new(format!(
            "invalid canonical D1 migration filename: {name}"
        )));
    }
    let bytes = name.as_bytes();
    if !bytes[..4].iter().all(|byte| byte.is_ascii_digit()) || bytes[4] != b'_' {
        return Err(D1Error::new(format!(
            "invalid canonical D1 migration revision prefix: {name}"
        )));
    }
    let suffix = &name[5..name.len() - 4];
    if suffix.is_empty()
        || suffix.starts_with('_')
        || suffix.ends_with('_')
        || suffix.contains("__")
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(D1Error::new(format!(
            "invalid canonical D1 migration name: {name}"
        )));
    }
    name[..4].parse::<u32>().map_err(|error| {
        D1Error::new(format!(
            "invalid canonical D1 migration revision {name}: {error}"
        ))
    })
}

fn validate_revisions(migrations: &[MigrationFile]) -> Result<(), D1Error> {
    let mut revisions = BTreeSet::new();
    for (index, migration) in migrations.iter().enumerate() {
        if !revisions.insert(migration.revision) {
            return Err(D1Error::new(format!(
                "duplicate canonical D1 migration revision: {:04}",
                migration.revision
            )));
        }
        let expected = u32::try_from(index + 1)
            .map_err(|_| D1Error::new("canonical D1 migration revision count overflow"))?;
        if migration.revision != expected {
            return Err(D1Error::new(format!(
                "canonical D1 migration revision gap: expected {expected:04}, found {:04}",
                migration.revision
            )));
        }
    }
    Ok(())
}

fn verify_historical_epoch(
    migrations: &[MigrationFile],
    epoch: HistoricalEpoch,
    component: D1Component,
) -> Result<(), D1Error> {
    let prefix = migrations.get(..epoch.migration_count).ok_or_else(|| {
        D1Error::new(format!(
            "{} historical epoch is missing migrations: expected {}",
            component.id(),
            epoch.migration_count
        ))
    })?;
    if prefix.last().map(|migration| migration.name.as_str()) != Some(epoch.final_revision) {
        return Err(D1Error::new(format!(
            "{} historical epoch final revision mismatch",
            component.id()
        )));
    }
    let digest = migration_history_digest(prefix)?;
    if digest != epoch.accepted_history_digest {
        return Err(D1Error::new(format!(
            "{} historical epoch digest mismatch: accepted={}, computed={digest}",
            component.id(),
            epoch.accepted_history_digest
        )));
    }
    Ok(())
}

fn verify_future_specs(
    migrations: &[MigrationFile],
    epoch: HistoricalEpoch,
    specs: &[MigrationSpec],
    component: D1Component,
) -> Result<(), D1Error> {
    let post_epoch = migrations.get(epoch.migration_count..).ok_or_else(|| {
        D1Error::new(format!(
            "{} historical epoch boundary is invalid",
            component.id()
        ))
    })?;
    if post_epoch.len() != specs.len() {
        return Err(D1Error::new(format!(
            "{} post-epoch migration/spec count mismatch: SQL={}, typed_specs={}",
            component.id(),
            post_epoch.len(),
            specs.len()
        )));
    }
    for (migration, spec) in post_epoch.iter().zip(specs) {
        if migration.name != spec.revision {
            return Err(D1Error::new(format!(
                "{} post-epoch migration lacks matching typed policy: {}",
                component.id(),
                migration.name
            )));
        }
        spec.to_contract()?;
    }
    Ok(())
}

fn migration_history_digest(migrations: &[MigrationFile]) -> Result<String, D1Error> {
    let identity = Value::Array(
        migrations
            .iter()
            .map(|migration| json!({"name": migration.name, "sha256": migration.sha256}))
            .collect(),
    );
    canonical_json(&identity)
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(D1Error::new)
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

fn compatibility_policy_digest() -> Result<String, D1Error> {
    canonical_json(&compatibility_policy_projection())
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(D1Error::new)
}

#[cfg(test)]
mod tests {
    use super::{D1Component, RepositoryMigrationCatalog, component_authority};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempRepository {
        path: PathBuf,
    }

    impl TempRepository {
        fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let path = std::env::temp_dir().join(format!(
                "opsctl-d1-catalog-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("migrations/resolver-d1"))?;
            Ok(Self { path })
        }

        fn root(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRepository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn copy_resolver_epoch(target: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let source = repository_root().join("migrations/resolver-d1");
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
        Ok(())
    }

    #[test]
    fn accepted_repository_epochs_match_compiled_anchors() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = repository_root();
        let catalog = component_authority(&root, "catalog")?;
        let resolver = component_authority(&root, "resolver")?;
        assert_eq!(catalog.ordered_history.len(), 26);
        assert_eq!(resolver.ordered_history.len(), 4);
        assert_eq!(
            catalog.current_repository_revision,
            "0026_outbound_mail_intents.sql"
        );
        assert_eq!(
            resolver.current_repository_revision,
            "0004_refresh_owner_hmac_version.sql"
        );
        Ok(())
    }

    #[test]
    fn modified_historical_sql_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let repository = TempRepository::new("tampered")?;
        let directory = repository.root().join("migrations/resolver-d1");
        copy_resolver_epoch(&directory)?;
        fs::write(
            directory.join("0002_oauth_refresh_fencing.sql"),
            b"SELECT 1;\n",
        )?;
        let error = match RepositoryMigrationCatalog::load(
            repository.root(),
            D1Component::Resolver,
        ) {
            Ok(_) => return Err("tampered historical SQL unexpectedly passed".into()),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("historical epoch digest mismatch")
        );
        Ok(())
    }

    #[test]
    fn missing_historical_migration_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let repository = TempRepository::new("missing")?;
        let directory = repository.root().join("migrations/resolver-d1");
        copy_resolver_epoch(&directory)?;
        fs::remove_file(directory.join("0003_lookup_hmac_versions.sql"))?;
        let error = match RepositoryMigrationCatalog::load(
            repository.root(),
            D1Component::Resolver,
        ) {
            Ok(_) => return Err("missing historical SQL unexpectedly passed".into()),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("revision gap")
                || error.to_string().contains("missing migrations")
        );
        Ok(())
    }

    #[test]
    fn unexpected_post_epoch_sql_without_typed_spec_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository = TempRepository::new("post-epoch")?;
        let directory = repository.root().join("migrations/resolver-d1");
        copy_resolver_epoch(&directory)?;
        fs::write(directory.join("0005_unowned.sql"), b"SELECT 1;\n")?;
        let error = match RepositoryMigrationCatalog::load(
            repository.root(),
            D1Component::Resolver,
        ) {
            Ok(_) => return Err("unowned post-epoch SQL unexpectedly passed".into()),
            Err(error) => error,
        };
        assert!(error.to_string().contains("migration/spec count mismatch"));
        Ok(())
    }

    #[test]
    fn invalid_and_duplicate_revisions_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        for (label, filename) in [
            ("invalid", "README.md"),
            ("duplicate", "0004_duplicate.sql"),
        ] {
            let repository = TempRepository::new(label)?;
            let directory = repository.root().join("migrations/resolver-d1");
            copy_resolver_epoch(&directory)?;
            fs::write(directory.join(filename), b"SELECT 1;\n")?;
            assert!(
                RepositoryMigrationCatalog::load(repository.root(), D1Component::Resolver).is_err()
            );
        }
        Ok(())
    }
}
