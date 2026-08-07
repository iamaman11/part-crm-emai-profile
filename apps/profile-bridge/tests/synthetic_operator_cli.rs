use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn temp_root(label: &str) -> std::path::PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "profile-bridge-synthetic-{label}-{}-{counter}",
        std::process::id()
    ))
}

#[test]
fn synthetic_operator_cli_completes_dirty_local() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("success");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_profile-bridge-synthetic"))
        .arg("profilebridge://claim/claim_0123456789abcdef0123456789abcdef")
        .arg(&root)
        .output()?;

    let _ = fs::remove_dir_all(&root);
    assert!(output.status.success(), "stderr={}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        "synthetic-operator-complete state=DIRTY_LOCAL\n"
    );
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn synthetic_operator_cli_rejects_invalid_claim_without_echoing_it()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("invalid");
    let secret_like_claim = "profilebridge://claim/not-valid/secret-claim";
    let output = Command::new(env!("CARGO_BIN_EXE_profile-bridge-synthetic"))
        .arg(secret_like_claim)
        .arg(&root)
        .output()?;

    let _ = fs::remove_dir_all(&root);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("invalid claim URI"));
    assert!(!stderr.contains(secret_like_claim));
    Ok(())
}
