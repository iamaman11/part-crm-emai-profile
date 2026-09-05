#![forbid(unsafe_code)]

use opsctl::canonical::{canonical_json, parse_strict_json, sha256_hex};
use opsctl::d1::repository_projection;
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, Box<dyn Error>> {
    let positions = args
        .iter()
        .enumerate()
        .filter(|(_, value)| value.as_str() == flag)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if positions.len() != 1 {
        return Err(format!("{flag} must be supplied exactly once").into());
    }
    let value = args
        .get(positions[0] + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.starts_with("--") {
        return Err(format!("{flag} requires a path value").into());
    }
    Ok(PathBuf::from(value))
}

fn strict_value(path: &Path, label: &str) -> Result<Value, Box<dyn Error>> {
    let text = fs::read_to_string(path)?;
    parse_strict_json(&text)
        .map_err(|error| format!("{label} strict JSON admission failed: {error}").into())
}

fn digest(value: &Value) -> Result<String, Box<dyn Error>> {
    let canonical = canonical_json(value)?;
    Ok(sha256_hex(canonical.as_bytes()))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let root = required_path(&args, "--root")?;
    let ledger = strict_value(&required_path(&args, "--ledger-json")?, "ledger")?;
    let release = strict_value(
        &required_path(&args, "--release-manifest")?,
        "release manifest",
    )?;
    let projection_text = repository_projection(&root)?;
    let projection = parse_strict_json(&projection_text)?;
    let repository_identity_sha256 = projection
        .get("repository_identity_sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64)
        .ok_or("typed D1 repository projection has no repository_identity_sha256")?;
    let output = json!({
        "schema_version": 1,
        "kind": "D1_CONTRACT_EVIDENCE_BINDINGS",
        "ledger_sha256": digest(&ledger)?,
        "release_manifest_sha256": digest(&release)?,
        "repository_identity_sha256": repository_identity_sha256,
        "mutation_executed": false
    });
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
