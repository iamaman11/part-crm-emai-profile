use opsctl::{
    canonical::{canonical_json, sha256_hex},
    d1,
};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;

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

#[test]
fn current_fresh_zero_construction_is_owned_by_production_repository_projection()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let first: Value = serde_json::from_str(&d1::repository_projection(&root)?)?;
    let second: Value = serde_json::from_str(&d1::repository_projection(&root)?)?;
    assert_eq!(
        first, second,
        "same typed D1 authority must produce one deterministic repository projection"
    );

    let envelope = first.get("fresh_zero_construction").ok_or_else(|| {
        io::Error::other("production D1 projection is missing fresh_zero_construction")
    })?;
    let construction = envelope
        .get("construction")
        .ok_or_else(|| io::Error::other("fresh-zero construction identity is missing"))?;
    let catalog = catalog_projection(&first)?;
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

    assert_eq!(construction["schema_version"], 1);
    assert_eq!(construction["kind"], "D1_CURRENT_FRESH_ZERO_CONSTRUCTION");
    assert_eq!(construction["component_id"], "catalog");
    assert_eq!(construction["target_schema_revision"], target);
    assert_eq!(
        construction["repository_identity_sha256"],
        first["repository_identity_sha256"]
    );
    assert_eq!(construction["provider_mutation_authorized"], false);
    assert_eq!(construction["production_mutation_authorized"], false);

    let governed_roots = first["executable_schema_authority"]
        .as_array()
        .ok_or_else(|| io::Error::other("executable_schema_authority is missing"))?;
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

    for entry in migration_sources {
        let migration_file = required_string(entry, "migration_file", "construction source")?;
        let source_root = required_string(entry, "source_root", "construction source")?;
        let projected_sha = required_string(entry, "sha256", "construction source")?;
        assert!(
            governed_roots
                .iter()
                .any(|root| root.as_str() == Some(source_root)),
            "construction source root must be governed by executable_schema_authority"
        );
        let actual_sha = sha256_hex(&fs::read(root.join(source_root).join(migration_file))?);
        assert_eq!(projected_sha, actual_sha);
    }

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

    let canonical = canonical_json(construction).map_err(io::Error::other)?;
    let expected_digest = sha256_hex(canonical.as_bytes());
    assert_eq!(
        required_string(envelope, "construction_sha256", "fresh-zero construction")?,
        expected_digest
    );

    let mut weakened = construction.clone();
    weakened["provider_mutation_authorized"] = Value::Bool(true);
    let weakened_digest = sha256_hex(
        canonical_json(&weakened)
            .map_err(io::Error::other)?
            .as_bytes(),
    );
    assert_ne!(
        expected_digest, weakened_digest,
        "authorization weakening must invalidate the deterministic construction identity"
    );
    Ok(())
}
