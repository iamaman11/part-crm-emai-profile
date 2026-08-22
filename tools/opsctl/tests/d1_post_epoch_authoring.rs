use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORICAL_REVISION: &str = "0026_outbound_mail_intents.sql";
const FUTURE_REVISION: &str = "0027_post_epoch_probe.sql";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct TempRepository {
    path: PathBuf,
}

impl TempRepository {
    fn from_current() -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!(
            "opsctl-d1-post-epoch-authoring-{}-{nonce}",
            std::process::id()
        ));
        let source = repo_root();

        copy_directory(
            &source.join("tools/opsctl/src"),
            &path.join("tools/opsctl/src"),
        )?;
        fs::create_dir_all(path.join("tools/opsctl"))?;
        fs::copy(
            source.join("tools/opsctl/Cargo.toml"),
            path.join("tools/opsctl/Cargo.toml"),
        )?;
        fs::copy(
            source.join("tools/opsctl/Cargo.lock"),
            path.join("tools/opsctl/Cargo.lock"),
        )?;
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
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(source_path, target_path)?;
        } else {
            return Err(format!("unexpected non-regular fixture entry: {}", source_path.display()).into());
        }
    }
    Ok(())
}

fn patch_future_catalog_spec(root: &Path) -> Result<(), Box<dyn Error>> {
    let path = root.join("tools/opsctl/src/d1/catalog.rs");
    let source = fs::read_to_string(&path)?;
    let current = r#"    const fn future_migrations(self) -> &'static [MigrationSpec] {
        match self {
            Self::Catalog | Self::Resolver => &[],
        }
    }
"#;
    let future = r#"    const fn future_migrations(self) -> &'static [MigrationSpec] {
        match self {
            Self::Catalog => &[MigrationSpec {
                revision: "0027_post_epoch_probe.sql",
                migration_class: MigrationClass::Expand,
                rollout_order: RolloutOrder::MigrateBeforeCode,
                fail_forward_required: false,
                destructive: false,
                code_rollback_allowed: true,
                contract_preconditions: &[],
            }],
            Self::Resolver => &[],
        }
    }
"#;
    if source.matches(current).count() != 1 {
        return Err("canonical empty future-migration sentinel changed; update the post-epoch proof".into());
    }
    fs::write(path, source.replacen(current, future, 1))?;
    Ok(())
}

fn write_future_sql(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::write(
        root.join("migrations/d1").join(FUTURE_REVISION),
        b"CREATE TABLE ar9_post_epoch_probe (\n    id TEXT PRIMARY KEY NOT NULL,\n    created_at_ms INTEGER NOT NULL\n);\n",
    )?;
    Ok(())
}

fn historical_catalog_names(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(root.join("migrations/d1"))?
        .map(|entry| {
            let entry = entry?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| std::io::Error::other("migration filename must be UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.retain(|name| name != FUTURE_REVISION);
    names.sort();
    if names.last().map(String::as_str) != Some(HISTORICAL_REVISION) {
        return Err("historical Catalog revision changed; update the post-epoch proof".into());
    }
    Ok(names)
}

fn write_ledger(path: &Path, names: &[String]) -> Result<(), Box<dyn Error>> {
    let rows = names
        .iter()
        .enumerate()
        .map(|(index, name)| json!({"id": index + 1, "name": name}))
        .collect::<Vec<_>>();
    fs::write(path, serde_json::to_vec_pretty(&json!({"rows": rows}))?)?;
    Ok(())
}

fn write_manifest(path: &Path, contract: Value) -> Result<(), Box<dyn Error>> {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({"schema_contract": contract}))?,
    )?;
    Ok(())
}

fn run_opsctl(root: &Path, args: &[&str]) -> Result<Value, Box<dyn Error>> {
    let cargo: OsString = env::var_os("CARGO").ok_or("CARGO executable is unavailable")?;
    let output = Command::new(cargo)
        .arg("run")
        .arg("--offline")
        .arg("--locked")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join("tools/opsctl/Cargo.toml"))
        .arg("--")
        .arg("--root")
        .arg(root)
        .args(args)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "future-shaped opsctl command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn component<'a>(projection: &'a Value, id: &str) -> Result<&'a Value, Box<dyn Error>> {
    projection["components"]
        .as_array()
        .and_then(|components| {
            components
                .iter()
                .find(|component| component["component_id"] == id)
        })
        .ok_or_else(|| format!("repository projection is missing component {id}").into())
}

#[test]
fn first_post_epoch_catalog_migration_runs_through_real_authoring_path() -> Result<(), Box<dyn Error>> {
    let repository = TempRepository::from_current()?;
    let root = repository.root();
    let historical = historical_catalog_names(root)?;

    patch_future_catalog_spec(root)?;
    write_future_sql(root)?;

    let first_projection = run_opsctl(root, &["d1", "repository"])?;
    let second_projection = run_opsctl(root, &["d1", "repository"])?;
    assert_eq!(first_projection, second_projection);

    let catalog = component(&first_projection, "catalog")?;
    assert_eq!(catalog["migration_count"], 27);
    assert_eq!(catalog["post_epoch_migration_count"], 1);
    assert_eq!(catalog["current_repository_revision"], FUTURE_REVISION);

    let target_contract = catalog["release_schema_contract"].clone();
    assert_eq!(target_contract["target_schema_revision"], FUTURE_REVISION);
    assert_eq!(target_contract["supported_schema_min"], FUTURE_REVISION);
    assert_eq!(target_contract["supported_schema_max"], FUTURE_REVISION);

    let mut current_contract = target_contract.clone();
    current_contract["target_schema_revision"] = json!(HISTORICAL_REVISION);
    current_contract["supported_schema_min"] = json!(HISTORICAL_REVISION);
    current_contract["supported_schema_max"] = json!(HISTORICAL_REVISION);

    let mut known_good_contract = current_contract.clone();
    known_good_contract["supported_schema_max"] = json!(FUTURE_REVISION);

    write_manifest(&root.join("target-release.json"), target_contract)?;
    write_manifest(&root.join("current-release.json"), current_contract)?;
    write_manifest(&root.join("known-good-release.json"), known_good_contract)?;
    write_ledger(&root.join("before-ledger.json"), &historical)?;

    let mut after = historical.clone();
    after.push(FUTURE_REVISION.to_owned());
    write_ledger(&root.join("after-ledger.json"), &after)?;

    let compatibility = run_opsctl(
        root,
        &[
            "d1",
            "compatibility",
            "--component",
            "catalog",
            "--ledger-json",
            "before-ledger.json",
            "--release-manifest",
            "target-release.json",
        ],
    )?;
    assert_eq!(compatibility["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(compatibility["decision"], "MIGRATION_REQUIRED");
    assert_eq!(compatibility["allowed"], true);
    assert_eq!(compatibility["planned_migrations"], json!([FUTURE_REVISION]));
    assert_eq!(
        compatibility["planned_migration_contracts"][0]["migration_class"],
        "EXPAND"
    );
    assert_eq!(
        compatibility["planned_migration_contracts"][0]["rollout_order"],
        "MIGRATE_BEFORE_CODE"
    );

    let plan = run_opsctl(
        root,
        &[
            "d1",
            "plan",
            "--component",
            "catalog",
            "--ledger-json",
            "before-ledger.json",
            "--release-manifest",
            "target-release.json",
            "--current-manifest",
            "current-release.json",
            "--known-good-manifest",
            "known-good-release.json",
        ],
    )?;
    assert_eq!(plan["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(plan["decision"], "MIGRATE_FIRST");
    assert_eq!(plan["allowed"], true);
    assert_eq!(plan["planned_migrations"], json!([FUTURE_REVISION]));
    assert_eq!(plan["rollback_context_complete"], true);

    let verify = run_opsctl(
        root,
        &[
            "d1",
            "verify",
            "--component",
            "catalog",
            "--ledger-json",
            "after-ledger.json",
            "--release-manifest",
            "target-release.json",
        ],
    )?;
    assert_eq!(verify["ledger_state"], "EXACT");
    assert_eq!(verify["decision"], "SAFE");
    assert_eq!(verify["allowed"], true);
    assert_eq!(verify["target_revision"], FUTURE_REVISION);

    Ok(())
}
