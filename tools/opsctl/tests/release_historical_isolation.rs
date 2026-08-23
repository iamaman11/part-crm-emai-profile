use opsctl::release::ReleaseAction;
use opsctl::release::commands::{self, ReleaseRunRequest};
use opsctl::release::digest::{canonical_json, sha256_hex};
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const APPROVED_HISTORICAL_BOUNDARY: [&str; 2] = ["document.rs", "document/historical_v2.rs"];

const HISTORICAL_WIRE_MARKERS: [&str; 4] = [
    "d1_evolution_authority_sha256",
    "release-set-v2-sha256-",
    "ReleaseSetManifest",
    "RELEASE_SET_SCHEMA_VERSION: u64 = 2",
];

fn production_source(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn historical_wire_violation(relative: &str, source: &str) -> Option<&'static str> {
    if APPROVED_HISTORICAL_BOUNDARY.contains(&relative) {
        return None;
    }
    let production = production_source(source);
    HISTORICAL_WIRE_MARKERS
        .into_iter()
        .find(|marker| production.contains(marker))
}

fn collect_rust_sources(
    root: &Path,
    current: &Path,
    output: &mut Vec<(String, String)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_rust_sources(root, &path, output)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        output.push((relative, fs::read_to_string(path)?));
    }
    Ok(())
}

fn release_source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/release")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temporary_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "opsctl-historical-v2-{}-{nonce}",
        std::process::id()
    )))
}

fn historical_fixture() -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/release-set-v2-historical.json"
    ))?)
}

fn resign_historical_release(value: &mut Value) -> Result<String, Box<dyn std::error::Error>> {
    let mut identity = value.clone();
    let object = identity
        .as_object_mut()
        .ok_or_else(|| io::Error::other("historical fixture must be an object"))?;
    object.remove("release_set_id");
    object.remove("display_version");
    let release_set_id = format!(
        "release-set-v2-sha256-{}",
        sha256_hex(canonical_json(&identity)?.as_bytes())
    );
    value["release_set_id"] = Value::String(release_set_id.clone());
    Ok(release_set_id)
}

fn accepted_source_evidence(release_set_id: &str) -> Result<Value, Box<dyn std::error::Error>> {
    const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut evidence = json!({
        "schema_version": 1,
        "kind": "AR11_ACCEPTED_SOURCE_EVIDENCE",
        "repository": "iamaman11/part-crm-emai-profile",
        "release_set_id": release_set_id,
        "source_commit_sha": SOURCE,
        "protected_ref": "refs/heads/main",
        "protected_ref_verified": true,
        "observed_protected_main_sha": SOURCE,
        "collection_authority": "github-actions/github-api",
        "proof": {
            "method": "GITHUB_COMPARE_API",
            "base_sha": SOURCE,
            "head_sha": SOURCE,
            "merge_base_sha": SOURCE,
            "status": "identical",
            "ahead_by": 0,
            "behind_by": 0
        }
    });
    let digest = sha256_hex(canonical_json(&evidence)?.as_bytes());
    evidence["evidence_sha256"] = Value::String(digest);
    Ok(evidence)
}

#[test]
fn historical_v2_wire_semantics_are_isolated_from_current_release_code()
-> Result<(), Box<dyn std::error::Error>> {
    let root = release_source_root();
    let mut sources = Vec::new();
    collect_rust_sources(&root, &root, &mut sources)?;

    assert!(
        root.join("document/historical_v2.rs").is_file(),
        "historical v2 decoder must remain a private child of the version-aware document boundary"
    );
    assert!(
        !root.join("historical_v2.rs").exists(),
        "historical v2 decoder must not be exposed as a release-level sibling module"
    );

    for (relative, source) in sources {
        if let Some(marker) = historical_wire_violation(&relative, &source) {
            return Err(format!(
                "historical Release Set v2 wire semantic marker {marker:?} leaked into current release source {relative}"
            )
            .into());
        }
    }
    Ok(())
}

#[test]
fn historical_v2_has_no_tracked_current_contract_authority() {
    assert!(
        !repository_root()
            .join("architecture/release-set-v2.json")
            .exists(),
        "historical v2 wire semantics must live only in the isolated decoder, not a mutable current architecture contract"
    );
}

#[test]
fn historical_v2_verify_is_minimum_integrity_only_and_target_use_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = temporary_root()?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let artifact_root = temp.join("artifacts");
        let components = artifact_root.join("components");
        fs::create_dir_all(&components)?;

        let artifact_bytes = b"x";
        let artifact_digest = sha256_hex(artifact_bytes);
        for path in [
            "control-plane.tar",
            "runtime-bundle.tar",
            "secret-resolver.tar",
        ] {
            fs::write(components.join(path), artifact_bytes)?;
        }

        let mut release = historical_fixture()?;
        for component in ["control_plane", "runtime_bundle", "secret_resolver"] {
            release["components"][component]["artifact_sha256"] =
                Value::String(artifact_digest.clone());
        }
        for artifact in release["artifact_inventory"]
            .as_array_mut()
            .ok_or_else(|| io::Error::other("historical artifact inventory must be an array"))?
        {
            artifact["sha256"] = Value::String(artifact_digest.clone());
        }
        let release_set_id = resign_historical_release(&mut release)?;

        fs::create_dir_all(&temp)?;
        let release_path = temp.join("release-set.json");
        fs::write(&release_path, serde_json::to_vec_pretty(&release)?)?;
        fs::write(
            temp.join("accepted-source-evidence.json"),
            serde_json::to_vec_pretty(&accepted_source_evidence(&release_set_id)?)?,
        )?;

        let root = repository_root();
        let output = commands::run(ReleaseRunRequest {
            root: &root,
            source_root: &root,
            action: ReleaseAction::Verify,
            release_set: &release_path,
            artifact_root: Some(&artifact_root),
            profile_id: None,
            environment: None,
            evidence_json: None,
            current_release_set: None,
        })?;
        let verified: Value = serde_json::from_str(&output)?;
        assert_eq!(
            verified["verification_scope"],
            "HISTORICAL_V2_SOURCE_AND_ARTIFACT_INTEGRITY"
        );
        assert_eq!(verified["historical_compatibility_only"], true);
        assert_eq!(verified["verified_files"], 3);
        assert_eq!(verified["verified_components"], json!([]));
        assert_eq!(verified["verified_provenance_dimensions"], json!([]));

        let target_error = commands::run(ReleaseRunRequest {
            root: &root,
            source_root: &root,
            action: ReleaseAction::Compatibility,
            release_set: &release_path,
            artifact_root: None,
            profile_id: None,
            environment: None,
            evidence_json: None,
            current_release_set: None,
        })
        .expect_err("historical v2 must never be accepted as current compatibility target");
        assert!(
            target_error
                .to_string()
                .contains("CURRENT_RELEASE_SET_V3_REQUIRED")
        );
        Ok(())
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

#[test]
fn historical_v2_isolation_guard_rejects_current_writer_leak() {
    let synthetic_current_writer = r#"
        const PREFIX: &str = "release-set-v2-sha256-";
        fn write_current() {}
    "#;
    assert_eq!(
        historical_wire_violation("v3_output.rs", synthetic_current_writer),
        Some("release-set-v2-sha256-")
    );
}
