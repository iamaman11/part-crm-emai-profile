use std::path::PathBuf;
use std::process::Command;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn current_reconstruction_materializer_is_credential_free_and_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = Command::new(python)
        .current_dir(&root)
        .arg("scripts/d1-current-reconstruction-materialize.py")
        .arg("self-test")
        .env_remove("CLOUDFLARE_API_TOKEN")
        .env_remove("CLOUDFLARE_OBSERVE_API_TOKEN")
        .env_remove("CLOUDFLARE_ACCOUNT_ID")
        .output()?;
    assert!(
        output.status.success(),
        "CURRENT reconstruction materializer self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("CURRENT reconstruction materializer adapter passed"));
    assert!(stdout.contains("provider_mutation=NO"));
    assert!(stdout.contains("production_mutation=NO"));
    Ok(())
}
