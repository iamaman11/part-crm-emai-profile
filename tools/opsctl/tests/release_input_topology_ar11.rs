use opsctl::release::input_topology::ReleaseInputTopology;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn temp_root(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "opsctl-ar11-release-input-{label}-{}-{nonce}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn canonical_input(input_id: &str, path: &str) -> String {
    format!(
        r#"{{
          "input_id":"{input_id}",
          "kind":"FIXTURE",
          "semantic_owner":"fixture",
          "canonical_source":"{path}",
          "release_identity_source":"{path}",
          "compatibility_dimension":"fixture",
          "required_for_release_set":true,
          "verification":["fixture"],
          "consumers":["fixture"]
        }}"#
    )
}

fn authority(rows: &[String]) -> String {
    format!(
        r#"{{
          "schema_version":1,
          "kind":"AR11_RELEASE_ARCHITECTURE_SOURCE",
          "release_inputs":[{}]
        }}"#,
        rows.join(",")
    )
}

#[test]
fn missing_release_identity_source_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("missing")?;
    let topology =
        ReleaseInputTopology::parse_json(&authority(&[canonical_input("missing", "missing.txt")]))?;
    let error = match topology.resolve(&root) {
        Err(error) => error,
        Ok(_) => return Err("missing release identity source unexpectedly resolved".into()),
    };
    assert!(error.to_string().contains("RELEASE_INPUT_MISSING"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn duplicate_release_input_id_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let source = authority(&[
        canonical_input("duplicate", "first.txt"),
        canonical_input("duplicate", "second.txt"),
    ]);
    let error = match ReleaseInputTopology::parse_json(&source) {
        Err(error) => error,
        Ok(_) => return Err("duplicate input_id unexpectedly parsed".into()),
    };
    assert!(error.to_string().contains("duplicate release input id"));
    Ok(())
}

#[test]
fn content_drift_changes_resolved_release_identity() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("digest-drift")?;
    let path = root.join("input.txt");
    fs::write(&path, b"first")?;
    let topology =
        ReleaseInputTopology::parse_json(&authority(&[canonical_input("digest", "input.txt")]))?;
    let first = topology.resolve(&root)?;
    fs::write(&path, b"other")?;
    let second = topology.resolve(&root)?;
    assert_ne!(first[0].sha256, second[0].sha256);
    assert_eq!(first[0].size_bytes, second[0].size_bytes);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_release_identity_source_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let root = temp_root("symlink")?;
    fs::write(root.join("target.txt"), b"target")?;
    symlink("target.txt", root.join("input.txt"))?;
    let topology =
        ReleaseInputTopology::parse_json(&authority(&[canonical_input("symlink", "input.txt")]))?;
    let error = match topology.resolve(&root) {
        Err(error) => error,
        Ok(_) => return Err("symlink release identity source unexpectedly resolved".into()),
    };
    assert!(
        error
            .to_string()
            .contains("RELEASE_INPUT_SYMLINK_FORBIDDEN")
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn canonical_repository_topology_still_resolves() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let topology = ReleaseInputTopology::load(&root)?;
    let resolved = topology.resolve(&root)?;
    assert_eq!(resolved.len(), 19);
    Ok(())
}
