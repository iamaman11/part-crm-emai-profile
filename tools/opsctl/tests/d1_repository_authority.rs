use opsctl::canonical::{canonical_json, sha256_hex};
use opsctl::d1::repository_projection;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CATALOG_EPOCH_DIGEST: &str =
    "4d1d8b8d3bba5d0903385d05fc18e0036628ff1123e0e26e9a080a340f7b5e2e";
const RESOLVER_EPOCH_DIGEST: &str =
    "98fd6f91a839223b06c441df4901dbd4fda8e69f2f90606f00e43faad91877ec";
const LEGACY_PAS2_REVISION: &str = "0027_pas2_payload_fingerprint.sql";
const SUCCESSOR_EXPAND_REVISION: &str = "0027_pas2_payload_fingerprint_expand.sql";
const SUCCESSOR_CONTRACT_REVISION: &str = "0032_pas2_payload_fingerprint_contract.sql";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn component<'a>(projection: &'a Value, id: &str) -> Result<&'a Value, Box<dyn Error>> {
    projection["components"]
        .as_array()
        .and_then(|components| {
            components
                .iter()
                .find(|component| component["component_id"] == id)
        })
        .ok_or_else(|| format!("D1 repository projection is missing component {id}").into())
}

fn identity_digest(identity: &[Value]) -> Result<String, Box<dyn Error>> {
    let canonical = canonical_json(&Value::Array(identity.to_vec()))?;
    Ok(sha256_hex(canonical.as_bytes()))
}

fn migration_identity(root: &Path, relative: &str) -> Result<(Vec<Value>, String), Box<dyn Error>> {
    let mut entries = fs::read_dir(root.join(relative))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    let mut identity = Vec::with_capacity(entries.len());
    for path in entries {
        if !path.is_file() || path.is_symlink() {
            return Err(
                format!("non-regular entry in migration directory: {}", path.display()).into(),
            );
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("migration filename is not UTF-8: {}", path.display()))?;
        let bytes = fs::read(&path)?;
        identity.push(json!({
            "name": name,
            "sha256": sha256_hex(&bytes),
        }));
    }

    let digest = identity_digest(&identity)?;
    Ok((identity, digest))
}

fn executable_identity_from_projection(
    root: &Path,
    component: &Value,
) -> Result<(Vec<Value>, String), Box<dyn Error>> {
    let sources = component["executable_migration_sources"]
        .as_array()
        .ok_or("Catalog executable_migration_sources projection is missing")?;
    let mut identity = Vec::with_capacity(sources.len());
    for source in sources {
        let name = source["migration_file"]
            .as_str()
            .ok_or("Catalog executable migration filename is missing")?;
        let source_root = source["source_root"]
            .as_str()
            .ok_or("Catalog executable migration source root is missing")?;
        if !matches!(source_root, "migrations/d1" | "migrations/d1-successor") {
            return Err(format!("unexpected Catalog source root: {source_root}").into());
        }
        let path = root.join(source_root).join(name);
        if !path.is_file() || path.is_symlink() {
            return Err(format!("Catalog executable source is not a regular file: {}", path.display()).into());
        }
        identity.push(json!({
            "name": name,
            "sha256": sha256_hex(&fs::read(path)?),
        }));
    }
    let digest = identity_digest(&identity)?;
    Ok((identity, digest))
}

struct TempRepository {
    path: PathBuf,
}

impl TempRepository {
    fn copy_from(source: &Path, label: &str) -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opsctl-d1-authority-{label}-{}-{nonce}",
            std::process::id()
        ));
        copy_directory(&source.join("migrations/d1"), &path.join("migrations/d1"))?;
        copy_directory(
            &source.join("migrations/d1-successor"),
            &path.join("migrations/d1-successor"),
        )?;
        copy_directory(
            &source.join("migrations/resolver-d1"),
            &path.join("migrations/resolver-d1"),
        )?;
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

fn copy_directory(source: &Path, target: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || entry.path().is_symlink() {
            return Err(format!(
                "unexpected non-regular migration entry: {}",
                entry.path().display()
            )
            .into());
        }
        fs::copy(entry.path(), target.join(entry.file_name()))?;
    }
    Ok(())
}

fn cargo_manifests(root: &Path, relative: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut manifests = Vec::new();
    let base = root.join(relative);
    if !base.exists() {
        return Ok(manifests);
    }
    let mut pending = vec![base];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else if entry.file_name() == "Cargo.toml" {
                manifests.push(path);
            }
        }
    }
    manifests.sort();
    Ok(manifests)
}

#[test]
fn frozen_epoch_and_current_projection_are_derived_from_real_sql_bytes()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let projection: Value = serde_json::from_str(&repository_projection(&root)?)?;

    let catalog = component(&projection, "catalog")?;
    let (catalog_legacy, _) = migration_identity(&root, "migrations/d1")?;
    let (catalog_current, catalog_current_digest) =
        executable_identity_from_projection(&root, catalog)?;
    assert_eq!(catalog_legacy.len(), 31);
    assert_eq!(catalog_legacy[25]["name"], "0026_outbound_mail_intents.sql");
    assert_eq!(catalog_legacy[26]["name"], LEGACY_PAS2_REVISION);
    assert_eq!(identity_digest(&catalog_legacy[..26])?, CATALOG_EPOCH_DIGEST);
    assert_eq!(catalog_current.len(), 32);
    assert_eq!(catalog_current[26]["name"], SUCCESSOR_EXPAND_REVISION);
    assert_eq!(catalog_current[31]["name"], SUCCESSOR_CONTRACT_REVISION);
    assert!(catalog_current.iter().all(|entry| entry["name"] != LEGACY_PAS2_REVISION));
    assert_eq!(catalog["migration_count"], 32);
    assert_eq!(catalog["current_repository_revision"], SUCCESSOR_CONTRACT_REVISION);
    assert_eq!(catalog["history_digest"], catalog_current_digest);
    assert_eq!(catalog["post_epoch_migration_count"], 6);
    assert_eq!(catalog["historical_epoch"]["migration_count"], 26);
    assert_eq!(
        catalog["historical_epoch"]["final_revision"],
        "0026_outbound_mail_intents.sql"
    );
    assert_eq!(
        catalog["historical_epoch"]["accepted_history_digest"],
        CATALOG_EPOCH_DIGEST
    );
    assert_eq!(catalog["legacy_history"]["immutable"], true);
    assert_eq!(catalog["legacy_history"]["executable_by_successor_lineage"], false);

    let resolver = component(&projection, "resolver")?;
    let (resolver_files, resolver_current_digest) = migration_identity(&root, "migrations/resolver-d1")?;
    assert_eq!(resolver_files.len(), 4);
    assert_eq!(resolver_files[3]["name"], "0004_refresh_owner_hmac_version.sql");
    assert_eq!(identity_digest(&resolver_files[..4])?, RESOLVER_EPOCH_DIGEST);
    assert_eq!(resolver["migration_count"], 4);
    assert_eq!(resolver["current_repository_revision"], "0004_refresh_owner_hmac_version.sql");
    assert_eq!(resolver["history_digest"], resolver_current_digest);
    assert_eq!(resolver["post_epoch_migration_count"], 0);
    assert_eq!(resolver["historical_epoch"]["accepted_history_digest"], RESOLVER_EPOCH_DIGEST);
    Ok(())
}

#[test]
fn repository_projection_is_deterministic_and_effect_free() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let first = repository_projection(&root)?;
    let second = repository_projection(&root)?;
    assert_eq!(first, second);

    let value: Value = serde_json::from_str(&first)?;
    assert_eq!(value["kind"], "D1_REPOSITORY_PROJECTION");
    assert_eq!(value["semantic_authority"], "tools/opsctl/src/d1");
    assert_eq!(
        value["executable_schema_authority"],
        json!([
            "migrations/d1",
            "migrations/d1-successor",
            "migrations/resolver-d1"
        ])
    );
    assert_eq!(value["effect_boundary"]["network"], false);
    assert_eq!(value["effect_boundary"]["provider_credentials"], false);
    assert_eq!(value["effect_boundary"]["provider_mutation"], false);
    assert_eq!(value["effect_boundary"]["database_mutation"], false);
    assert_eq!(value["effect_boundary"]["production_mutation"], false);
    Ok(())
}

#[test]
fn historical_sql_tampering_fails_closed_for_each_component() -> Result<(), Box<dyn Error>> {
    let source = repo_root();
    for (label, relative) in [
        ("catalog-tamper", "migrations/d1/0001_catalog.sql"),
        (
            "resolver-tamper",
            "migrations/resolver-d1/0001_resolver_security_boundary.sql",
        ),
    ] {
        let repository = TempRepository::copy_from(&source, label)?;
        fs::write(repository.root().join(relative), b"SELECT 1;\n")?;
        let error = match repository_projection(repository.root()) {
            Ok(_) => return Err(format!("tampered {label} SQL unexpectedly passed").into()),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("historical epoch digest mismatch"),
            "unexpected fail-closed reason for {label}: {error}"
        );
    }
    Ok(())
}

#[test]
fn unowned_post_epoch_sql_fails_closed_for_each_component() -> Result<(), Box<dyn Error>> {
    let source = repo_root();
    for (label, relative, expected_reason) in [
        (
            "catalog-successor-post-epoch",
            "migrations/d1-successor/0033_unowned.sql",
            "Catalog successor migration inventory mismatch",
        ),
        (
            "resolver-post-epoch",
            "migrations/resolver-d1/0005_unowned.sql",
            "post-epoch migration/spec count mismatch",
        ),
    ] {
        let repository = TempRepository::copy_from(&source, label)?;
        fs::write(repository.root().join(relative), b"SELECT 1;\n")?;
        let error = match repository_projection(repository.root()) {
            Ok(_) => return Err(format!("unowned {label} migration unexpectedly passed").into()),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains(expected_reason),
            "unexpected fail-closed reason for {label}: {error}"
        );
    }
    Ok(())
}

#[test]
fn legacy_and_successor_pas2_revisions_have_distinct_governed_roles()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let projection: Value = serde_json::from_str(&repository_projection(&root)?)?;
    let catalog = component(&projection, "catalog")?;
    let resolver = component(&projection, "resolver")?;
    let (catalog_legacy, _) = migration_identity(&root, "migrations/d1")?;
    let successor_files = migration_identity(&root, "migrations/d1-successor")?.0;
    let sources = catalog["executable_migration_sources"]
        .as_array()
        .ok_or("Catalog executable migration source projection missing")?;

    assert_eq!(catalog["historical_epoch"]["migration_count"], 26);
    assert_eq!(
        catalog["historical_epoch"]["final_revision"],
        "0026_outbound_mail_intents.sql"
    );
    assert_eq!(catalog_legacy[26]["name"], LEGACY_PAS2_REVISION);
    assert_eq!(catalog_legacy[30]["name"], "0031_device_binding_governance.sql");
    assert_eq!(successor_files.len(), 2);
    assert_eq!(successor_files[0]["name"], SUCCESSOR_EXPAND_REVISION);
    assert_eq!(successor_files[1]["name"], SUCCESSOR_CONTRACT_REVISION);

    assert_eq!(catalog["migration_count"], 32);
    assert_eq!(catalog["post_epoch_migration_count"], 6);
    assert_eq!(catalog["current_repository_revision"], SUCCESSOR_CONTRACT_REVISION);
    assert_eq!(catalog["migration_lineage"], "catalog-successor-v1");
    assert_eq!(sources.len(), 32);
    assert_eq!(sources[26]["migration_file"], SUCCESSOR_EXPAND_REVISION);
    assert_eq!(sources[26]["source_root"], "migrations/d1-successor");
    assert_eq!(sources[27]["migration_file"], "0028_profile_assignment_detach.sql");
    assert_eq!(sources[27]["source_root"], "migrations/d1");
    assert_eq!(sources[31]["migration_file"], SUCCESSOR_CONTRACT_REVISION);
    assert_eq!(sources[31]["source_root"], "migrations/d1-successor");
    assert!(sources.iter().all(|source| source["migration_file"] != LEGACY_PAS2_REVISION));
    assert_eq!(
        catalog["release_schema_contract"]["target_schema_revision"],
        "0031_device_binding_governance.sql"
    );
    assert_eq!(
        catalog["release_schema_contract"]["supported_schema_min"],
        "0031_device_binding_governance.sql"
    );
    assert_eq!(
        catalog["release_schema_contract"]["supported_schema_max"],
        SUCCESSOR_CONTRACT_REVISION
    );

    assert_eq!(resolver["historical_epoch"]["migration_count"], 4);
    assert_eq!(resolver["post_epoch_migration_count"], 0);
    assert_eq!(
        resolver["current_repository_revision"],
        "0004_refresh_owner_hmac_version.sql"
    );
    Ok(())
}

#[test]
fn product_runtime_cannot_depend_on_opsctl() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let mut manifests = cargo_manifests(&root, "apps")?;
    manifests.extend(cargo_manifests(&root, "crates")?);
    assert!(!manifests.is_empty());

    for manifest in manifests {
        let text = fs::read_to_string(&manifest)?;
        assert!(
            !text.contains("opsctl"),
            "product/runtime manifest must not depend on opsctl: {}",
            manifest.display()
        );
    }
    Ok(())
}
