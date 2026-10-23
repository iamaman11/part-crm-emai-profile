//! Decode an original published runtime history without substituting CURRENT future SQL.
use super::model::{D1Error, ReleaseSchemaContract};
use super::status::revision_index;
use crate::canonical::{canonical_json, sha256_hex};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedMigration {
    path: String,
    sha256: String,
    size: u64,
}

pub(super) fn original_runtime_history(
    root: &Path,
    manifest: &Value,
    schema: &ReleaseSchemaContract,
    names: &[String],
    source_roots: &[String],
) -> Result<String, D1Error> {
    let files: Vec<PublishedMigration> = serde_json::from_value(
        manifest
            .get("migrations")
            .and_then(|m| m.get("files"))
            .cloned()
            .ok_or_else(|| D1Error::new("original runtime migration file evidence is missing"))?,
    )
    .map_err(|e| {
        D1Error::new(format!(
            "invalid original runtime migration file evidence: {e}"
        ))
    })?;
    if files.len() != names.len() || names.len() != source_roots.len() {
        return Err(D1Error::new(
            "original runtime migration inventory differs from its window",
        ));
    }
    let compiled_target = revision_index(names, &schema.target_schema_revision)?;
    let mut identity = Vec::with_capacity(files.len());
    for (index, ((file, name), source_root)) in
        files.iter().zip(names).zip(source_roots).enumerate()
    {
        if file.path != format!("migrations/d1/{name}")
            || file.size == 0
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(D1Error::new(
                "original runtime migration file identity is invalid",
            ));
        }
        // The compiled schema prefix is immutable. Future files advertised by that
        // publication remain original evidence, never replacement SQL or a target.
        if index <= compiled_target {
            let bytes = fs::read(root.join(source_root).join(name))
                .map_err(|e| D1Error::new(format!("cannot read compiled runtime prefix: {e}")))?;
            if file.sha256 != sha256_hex(&bytes) || file.size != bytes.len() as u64 {
                return Err(D1Error::new(
                    "original runtime compiled migration prefix has changed",
                ));
            }
        }
        identity.push(json!({"name": name, "sha256": file.sha256}));
    }
    let digest = sha256_hex(
        canonical_json(&Value::Array(identity))
            .map_err(D1Error::new)?
            .as_bytes(),
    );
    if digest != schema.migration_history_digest {
        return Err(D1Error::new(
            "original runtime migration history digest does not match its files",
        ));
    }
    Ok(digest)
}
