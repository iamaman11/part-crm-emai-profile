use opsctl::{
    canonical::{canonical_json, sha256_hex},
    d1,
};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn required_string<'a>(
    value: &'a Value,
    field: &str,
    label: &str,
) -> Result<&'a str, Box<dyn Error>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::other(format!("{label} is missing {field}")).into())
}

fn catalog_projection(projection: &Value) -> Result<&Value, Box<dyn Error>> {
    let components = projection
        .get("components")
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::other("typed D1 projection is missing components"))?;
    let matches = components
        .iter()
        .filter(|component| {
            component.get("component_id").and_then(Value::as_str) == Some("catalog")
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(io::Error::other(
            "typed D1 projection must contain exactly one Catalog component",
        )
        .into());
    }
    Ok(matches[0])
}

fn read_governed_source(
    root: &Path,
    source_root: &str,
    migration_file: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if Path::new(migration_file).components().count() != 1 || !migration_file.ends_with(".sql") {
        return Err(io::Error::other(
            "fresh-zero migration filename is not one repository-local SQL file",
        )
        .into());
    }
    let canonical_root = fs::canonicalize(root)?;
    let migration_root = root.join(source_root);
    let migration_root_metadata = fs::symlink_metadata(&migration_root)?;
    if migration_root_metadata.file_type().is_symlink() || !migration_root_metadata.is_dir() {
        return Err(io::Error::other("fresh-zero migration root must be a real directory").into());
    }
    let source = migration_root.join(migration_file);
    let metadata = fs::symlink_metadata(&source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::other("fresh-zero migration source must be a regular file").into());
    }
    let canonical_source = fs::canonicalize(&source)?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(io::Error::other("fresh-zero migration source escaped repository root").into());
    }
    Ok(fs::read(canonical_source)?)
}

fn derive_current_construction(root: &Path, projection: &Value) -> Result<Value, Box<dyn Error>> {
    let catalog = catalog_projection(projection)?;
    let contract = catalog.get("release_schema_contract").ok_or_else(|| {
        io::Error::other("typed Catalog projection is missing release_schema_contract")
    })?;
    let target = required_string(
        contract,
        "target_schema_revision",
        "Catalog release contract",
    )?;
    let supported_max =
        required_string(contract, "supported_schema_max", "Catalog release contract")?;
    let repository_identity = required_string(
        projection,
        "repository_identity_sha256",
        "typed D1 repository projection",
    )?;

    let governed_roots = projection
        .get("executable_schema_authority")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::other("typed D1 projection is missing executable_schema_authority")
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::other("executable schema root must be a string"))
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if governed_roots.is_empty() {
        return Err(io::Error::other("executable schema authority must not be empty").into());
    }

    let sources = catalog
        .get("executable_migration_sources")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::other("typed Catalog projection is missing executable_migration_sources")
        })?;
    let target_positions = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            (source.get("migration_file").and_then(Value::as_str) == Some(target)).then_some(index)
        })
        .collect::<Vec<_>>();
    if target_positions.len() != 1 {
        return Err(io::Error::other(
            "CURRENT Catalog target is missing or ambiguous in executable lineage",
        )
        .into());
    }
    let target_index = target_positions[0];

    let mut construction_sources = Vec::with_capacity(target_index + 1);
    for source in &sources[..=target_index] {
        let migration_file = required_string(source, "migration_file", "Catalog migration source")?;
        let source_root = required_string(source, "source_root", "Catalog migration source")?;
        if !governed_roots.contains(source_root) {
            return Err(io::Error::other(
                "fresh-zero migration source escaped executable schema authority",
            )
            .into());
        }
        let bytes = read_governed_source(root, source_root, migration_file)?;
        construction_sources.push(json!({
            "migration_file": migration_file,
            "source_root": source_root,
            "sha256": sha256_hex(&bytes),
        }));
    }

    let deferred_revisions = sources[target_index + 1..]
        .iter()
        .map(|source| {
            required_string(source, "migration_file", "deferred Catalog migration")
                .map(str::to_owned)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if target == supported_max {
        if !deferred_revisions.is_empty() {
            return Err(io::Error::other(
                "Catalog lineage contains deferred revisions outside an exact release target",
            )
            .into());
        }
    } else if deferred_revisions.last().map(String::as_str) != Some(supported_max) {
        return Err(io::Error::other(
            "Catalog deferred lineage does not terminate at supported_schema_max",
        )
        .into());
    }

    let identity = json!({
        "schema_version": 1,
        "kind": "D1_CURRENT_FRESH_ZERO_CONSTRUCTION",
        "component_id": "catalog",
        "repository_identity_sha256": repository_identity,
        "target_schema_revision": target,
        "migration_sources": construction_sources,
        "deferred_revisions": deferred_revisions,
        "provider_mutation_authorized": false,
        "production_mutation_authorized": false,
    });
    let canonical = canonical_json(&identity).map_err(io::Error::other)?;
    Ok(json!({
        "construction": identity,
        "construction_sha256": sha256_hex(canonical.as_bytes()),
    }))
}

#[test]
fn current_fresh_zero_construction_is_projection_derived_digest_bound_and_contract_safe()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let projection: Value = serde_json::from_str(&d1::repository_projection(&root)?)?;

    let first = derive_current_construction(&root, &projection)?;
    let second = derive_current_construction(&root, &projection)?;
    assert_eq!(
        first, second,
        "same typed D1 authority must produce one deterministic construction identity"
    );

    let construction = first
        .get("construction")
        .ok_or_else(|| io::Error::other("construction identity is missing"))?;
    let catalog = catalog_projection(&projection)?;
    let contract = catalog
        .get("release_schema_contract")
        .ok_or_else(|| io::Error::other("Catalog release contract is missing"))?;
    let target = required_string(
        contract,
        "target_schema_revision",
        "Catalog release contract",
    )?;
    let supported_max =
        required_string(contract, "supported_schema_max", "Catalog release contract")?;

    assert_eq!(construction["target_schema_revision"], target);
    assert_eq!(
        construction["repository_identity_sha256"],
        projection["repository_identity_sha256"]
    );
    assert_eq!(construction["provider_mutation_authorized"], false);
    assert_eq!(construction["production_mutation_authorized"], false);

    let migration_sources = construction["migration_sources"]
        .as_array()
        .ok_or_else(|| io::Error::other("construction migration_sources are missing"))?;
    assert!(!migration_sources.is_empty());
    assert_eq!(
        migration_sources
            .last()
            .and_then(|entry| entry["migration_file"].as_str()),
        Some(target)
    );
    assert!(migration_sources.iter().all(|entry| {
        entry["sha256"].as_str().is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
        })
    }));

    let deferred = construction["deferred_revisions"]
        .as_array()
        .ok_or_else(|| io::Error::other("construction deferred_revisions are missing"))?;
    assert!(
        deferred
            .iter()
            .all(|revision| revision.as_str() != Some(target))
    );
    if target != supported_max {
        assert_eq!(deferred.last().and_then(Value::as_str), Some(supported_max));
        assert!(
            migration_sources
                .iter()
                .all(|entry| entry["migration_file"].as_str() != Some(supported_max))
        );
    }

    let construction_sha256 =
        required_string(&first, "construction_sha256", "fresh-zero construction")?;
    assert_eq!(construction_sha256.len(), 64);
    assert!(
        construction_sha256
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    );
    Ok(())
}
