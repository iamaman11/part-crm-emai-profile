use opsctl::d1::repository_projection;
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CATALOG_HISTORICAL_REVISION: &str = "0026_outbound_mail_intents.sql";
const CATALOG_HISTORICAL_DIGEST: &str =
    "4d1d8b8d3bba5d0903385d05fc18e0036628ff1123e0e26e9a080a340f7b5e2e";
const CATALOG_FUTURE_REVISION: &str = "0027_post_epoch_probe.sql";
const RESOLVER_HISTORICAL_REVISION: &str = "0004_refresh_owner_hmac_version.sql";
const RESOLVER_HISTORICAL_DIGEST: &str =
    "98fd6f91a839223b06c441df4901dbd4fda8e69f2f90606f00e43faad91877ec";
const RESOLVER_FUTURE_REVISION: &str = "0005_post_epoch_probe.sql";
const CANONICAL_ROOT_SENTINELS: &[&str] = &[
    "Cargo.toml",
    "architecture/inventory.json",
    "architecture/python-estate-ar6.json",
    "architecture/credential-authority.json",
    "architecture/credential-lifecycle.json",
    "architecture/profile-security.json",
    "architecture/operator-contract.json",
    "scripts/generate-architecture-inventory.py",
    "scripts/python-estate-ar6.py",
];

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

        for relative in CANONICAL_ROOT_SENTINELS {
            copy_file(&source, &path, relative)?;
        }
        copy_directory(
            &source.join("tools/opsctl/src"),
            &path.join("tools/opsctl/src"),
        )?;
        copy_file(&source, &path, "tools/opsctl/Cargo.toml")?;
        copy_file(&source, &path, "tools/opsctl/Cargo.lock")?;
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

fn copy_file(source_root: &Path, target_root: &Path, relative: &str) -> Result<(), Box<dyn Error>> {
    let source = source_root.join(relative);
    let target = target_root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| format!("fixture path has no parent: {relative}"))?;
    fs::create_dir_all(parent)?;
    fs::copy(source, target)?;
    Ok(())
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
            return Err(format!(
                "unexpected non-regular fixture entry: {}",
                source_path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn patch_future_specs(root: &Path) -> Result<(), Box<dyn Error>> {
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
            Self::Resolver => &[MigrationSpec {
                revision: "0005_post_epoch_probe.sql",
                migration_class: MigrationClass::Expand,
                rollout_order: RolloutOrder::MigrateBeforeCode,
                fail_forward_required: false,
                destructive: false,
                code_rollback_allowed: true,
                contract_preconditions: &[],
            }],
        }
    }
"#;
    if source.matches(current).count() != 1 {
        return Err(
            "canonical empty future-migration sentinel changed; update the post-epoch proof".into(),
        );
    }
    fs::write(path, source.replacen(current, future, 1))?;
    Ok(())
}

fn install_future_sql(root: &Path) -> Result<(), Box<dyn Error>> {
    let source = repo_root();
    copy_file(
        &source,
        root,
        "tests/d1-evolution/post-epoch/catalog/0027_post_epoch_probe.sql",
    )?;
    copy_file(
        &source,
        root,
        "tests/d1-evolution/post-epoch/resolver/0005_post_epoch_probe.sql",
    )?;
    fs::rename(
        root.join("tests/d1-evolution/post-epoch/catalog/0027_post_epoch_probe.sql"),
        root.join("migrations/d1/0027_post_epoch_probe.sql"),
    )?;
    fs::rename(
        root.join("tests/d1-evolution/post-epoch/resolver/0005_post_epoch_probe.sql"),
        root.join("migrations/resolver-d1/0005_post_epoch_probe.sql"),
    )?;
    Ok(())
}

fn historical_names(
    root: &Path,
    migration_root: &str,
    future_revision: &str,
    historical_revision: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(root.join(migration_root))?
        .map(|entry| {
            let entry = entry?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| std::io::Error::other("migration filename must be UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.retain(|name| name != future_revision);
    names.sort();
    if names.last().map(String::as_str) != Some(historical_revision) {
        return Err(format!(
            "historical revision changed for {migration_root}; update the post-epoch proof"
        )
        .into());
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

struct FutureCase {
    component: &'static str,
    migration_root: &'static str,
    historical_revision: &'static str,
    historical_digest: &'static str,
    future_revision: &'static str,
}

fn prove_future_case(
    root: &Path,
    projection: &Value,
    case: &FutureCase,
) -> Result<(), Box<dyn Error>> {
    let historical = historical_names(
        root,
        case.migration_root,
        case.future_revision,
        case.historical_revision,
    )?;
    let projected = component(projection, case.component)?;
    assert_eq!(projected["migration_count"], historical.len() + 1);
    assert_eq!(projected["post_epoch_migration_count"], 1);
    assert_eq!(
        projected["current_repository_revision"],
        case.future_revision
    );
    assert_ne!(projected["history_digest"], case.historical_digest);

    let target_contract = projected["release_schema_contract"].clone();
    assert_eq!(
        target_contract["target_schema_revision"],
        case.future_revision
    );
    assert_eq!(
        target_contract["supported_schema_min"],
        case.future_revision
    );
    assert_eq!(
        target_contract["supported_schema_max"],
        case.future_revision
    );

    let mut current_contract = target_contract.clone();
    current_contract["target_schema_revision"] = json!(case.historical_revision);
    current_contract["supported_schema_min"] = json!(case.historical_revision);
    current_contract["supported_schema_max"] = json!(case.historical_revision);

    let mut known_good_contract = current_contract.clone();
    known_good_contract["supported_schema_max"] = json!(case.future_revision);

    let target_manifest = format!("{}-target-release.json", case.component);
    let current_manifest = format!("{}-current-release.json", case.component);
    let known_good_manifest = format!("{}-known-good-release.json", case.component);
    let before_ledger = format!("{}-before-ledger.json", case.component);
    let after_ledger = format!("{}-after-ledger.json", case.component);

    write_manifest(&root.join(&target_manifest), target_contract)?;
    write_manifest(&root.join(&current_manifest), current_contract)?;
    write_manifest(&root.join(&known_good_manifest), known_good_contract)?;
    write_ledger(&root.join(&before_ledger), &historical)?;

    let mut after = historical.clone();
    after.push(case.future_revision.to_owned());
    write_ledger(&root.join(&after_ledger), &after)?;

    let compatibility = run_opsctl(
        root,
        &[
            "d1",
            "compatibility",
            "--component",
            case.component,
            "--ledger-json",
            &before_ledger,
            "--release-manifest",
            &target_manifest,
        ],
    )?;
    assert_eq!(compatibility["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(compatibility["decision"], "MIGRATION_REQUIRED");
    assert_eq!(compatibility["allowed"], true);
    assert_eq!(
        compatibility["planned_migrations"],
        json!([case.future_revision])
    );
    assert_eq!(
        compatibility["planned_migration_contracts"][0]["migration_class"],
        "EXPAND"
    );
    assert_eq!(
        compatibility["planned_migration_contracts"][0]["rollout_order"],
        "MIGRATE_BEFORE_CODE"
    );
    assert_eq!(
        compatibility["planned_migration_contracts"][0]["code_rollback_allowed"],
        true
    );

    let plan = run_opsctl(
        root,
        &[
            "d1",
            "plan",
            "--component",
            case.component,
            "--ledger-json",
            &before_ledger,
            "--release-manifest",
            &target_manifest,
            "--current-manifest",
            &current_manifest,
            "--known-good-manifest",
            &known_good_manifest,
        ],
    )?;
    assert_eq!(plan["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(plan["decision"], "MIGRATE_FIRST");
    assert_eq!(plan["allowed"], true);
    assert_eq!(plan["planned_migrations"], json!([case.future_revision]));
    assert_eq!(plan["rollback_context_complete"], true);

    let verify = run_opsctl(
        root,
        &[
            "d1",
            "verify",
            "--component",
            case.component,
            "--ledger-json",
            &after_ledger,
            "--release-manifest",
            &target_manifest,
        ],
    )?;
    assert_eq!(verify["ledger_state"], "EXACT");
    assert_eq!(verify["decision"], "SAFE");
    assert_eq!(verify["allowed"], true);
    assert_eq!(verify["target_revision"], case.future_revision);
    Ok(())
}

#[test]
fn first_post_epoch_catalog_and_resolver_migrations_run_through_real_authoring_path()
-> Result<(), Box<dyn Error>> {
    let repository = TempRepository::from_current()?;
    let root = repository.root();

    patch_future_specs(root)?;
    install_future_sql(root)?;

    let first_projection = run_opsctl(root, &["d1", "repository"])?;
    let second_projection = run_opsctl(root, &["d1", "repository"])?;
    assert_eq!(first_projection, second_projection);

    for case in [
        FutureCase {
            component: "catalog",
            migration_root: "migrations/d1",
            historical_revision: CATALOG_HISTORICAL_REVISION,
            historical_digest: CATALOG_HISTORICAL_DIGEST,
            future_revision: CATALOG_FUTURE_REVISION,
        },
        FutureCase {
            component: "resolver",
            migration_root: "migrations/resolver-d1",
            historical_revision: RESOLVER_HISTORICAL_REVISION,
            historical_digest: RESOLVER_HISTORICAL_DIGEST,
            future_revision: RESOLVER_FUTURE_REVISION,
        },
    ] {
        prove_future_case(root, &first_projection, &case)?;
    }

    let canonical_projection: Value = serde_json::from_str(&repository_projection(&repo_root())?)?;
    assert_eq!(
        component(&canonical_projection, "catalog")?["post_epoch_migration_count"],
        0
    );
    assert_eq!(
        component(&canonical_projection, "resolver")?["post_epoch_migration_count"],
        0
    );
    assert!(
        !repo_root()
            .join("migrations/d1/0027_post_epoch_probe.sql")
            .exists()
    );
    assert!(
        !repo_root()
            .join("migrations/resolver-d1/0005_post_epoch_probe.sql")
            .exists()
    );
    Ok(())
}
