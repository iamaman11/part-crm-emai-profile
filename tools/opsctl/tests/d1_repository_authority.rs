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
        if !path.is_file() {
            return Err(
                format!("non-file entry in migration directory: {}", path.display()).into(),
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
        if !entry.file_type()?.is_file() {
            return Err(format!(
                "unexpected non-file migration entry: {}",
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

    for (
        id,
        migration_root,
        historical_count,
        historical_revision,
        current_count,
        current_revision,
        post_epoch_count,
        accepted_digest,
    ) in [
        (
            "catalog",
            "migrations/d1",
            26_usize,
            "0026_outbound_mail_intents.sql",
            29_u64,
            "0029_profile_launch_authority.sql",
            3_u64,
            CATALOG_EPOCH_DIGEST,
        ),
        (
            "resolver",
            "migrations/resolver-d1",
            4_usize,
            "0004_refresh_owner_hmac_version.sql",
            4_u64,
            "0004_refresh_owner_hmac_version.sql",
            0_u64,
            RESOLVER_EPOCH_DIGEST,
        ),
    ] {
        let (files, computed_current_digest) = migration_identity(&root, migration_root)?;
        let current = component(&projection, id)?;
        let historical = &files[..historical_count];
        let computed_historical_digest = identity_digest(historical)?;

        assert_eq!(files.len() as u64, current_count);
        assert_eq!(
            historical.last().and_then(|value| value["name"].as_str()),
            Some(historical_revision)
        );
        assert_eq!(computed_historical_digest, accepted_digest);
        assert_eq!(current["migration_count"], current_count);
        assert_eq!(current["current_repository_revision"], current_revision);
        assert_eq!(current["history_digest"], computed_current_digest);
        assert_eq!(current["post_epoch_migration_count"], post_epoch_count);
        assert_eq!(
            current["historical_epoch"]["migration_count"],
            historical_count as u64
        );
        assert_eq!(
            current["historical_epoch"]["final_revision"],
            historical_revision
        );
        assert_eq!(
            current["historical_epoch"]["accepted_history_digest"],
            accepted_digest
        );
    }
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
        json!(["migrations/d1", "migrations/resolver-d1"])
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
            error
                .to_string()
                .contains("historical epoch digest mismatch")
        );
    }
    Ok(())
}

#[test]
fn unowned_post_epoch_sql_fails_closed_for_each_component() -> Result<(), Box<dyn Error>> {
    let source = repo_root();
    for (label, relative) in [
        ("catalog-post-epoch", "migrations/d1/0030_unowned.sql"),
        (
            "resolver-post-epoch",
            "migrations/resolver-d1/0005_unowned.sql",
        ),
    ] {
        let repository = TempRepository::copy_from(&source, label)?;
        fs::write(repository.root().join(relative), b"SELECT 1;\n")?;
        let error = match repository_projection(repository.root()) {
            Ok(_) => return Err(format!("unowned {label} migration unexpectedly passed").into()),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("post-epoch migration/spec count mismatch"),
            "unexpected fail-closed reason for {label}: {error}"
        );
    }
    Ok(())
}

#[test]
fn catalog_0027_is_the_typed_first_post_epoch_revision() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let projection: Value = serde_json::from_str(&repository_projection(&root)?)?;
    let catalog = component(&projection, "catalog")?;
    let resolver = component(&projection, "resolver")?;
    let (catalog_files, _) = migration_identity(&root, "migrations/d1")?;

    assert_eq!(catalog["historical_epoch"]["migration_count"], 26);
    assert_eq!(
        catalog["historical_epoch"]["final_revision"],
        "0026_outbound_mail_intents.sql"
    );
    assert_eq!(
        catalog_files[26]["name"],
        "0027_pas2_payload_fingerprint.sql"
    );
    assert_eq!(
        catalog_files[27]["name"],
        "0028_profile_assignment_detach.sql"
    );
    assert_eq!(
        catalog_files[28]["name"],
        "0029_profile_launch_authority.sql"
    );
    assert_eq!(catalog["migration_count"], 29);
    assert_eq!(catalog["post_epoch_migration_count"], 3);
    assert_eq!(
        catalog["current_repository_revision"],
        "0029_profile_launch_authority.sql"
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
