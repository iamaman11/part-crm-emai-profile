use std::fs;
use std::path::{Path, PathBuf};

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
