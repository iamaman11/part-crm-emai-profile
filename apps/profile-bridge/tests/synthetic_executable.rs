use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

fn temp_root(label: &str) -> std::path::PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "profile-bridge-synthetic-{label}-{}-{counter}",
        std::process::id()
    ))
}

#[test]
fn synthetic_binary_runs_the_composed_operator_path() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("success");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_profile-bridge-synthetic"))
        .arg("profilebridge://claim/claim_01JOPERATOR000000000000000000000001")
        .arg(&root)
        .output()?;

    assert!(output.status.success(), "stderr={}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        "synthetic-operator-complete state=DIRTY_LOCAL"
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn synthetic_binary_rejects_non_absolute_materialization_root()
-> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_profile-bridge-synthetic"))
        .arg("profilebridge://claim/claim_01JOPERATOR000000000000000000000002")
        .arg("relative-root")
        .output()?;

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)?.contains("materialization root must be absolute")
    );
    Ok(())
}
