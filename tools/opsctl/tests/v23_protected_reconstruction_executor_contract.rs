use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn protected_d1_executor_owns_exact_current_reconstruction_effect() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/d1-migration-executor.yml"))?;

    assert!(workflow.contains("name: Protected D1 Migration Executor"));
    assert!(workflow.contains("- reconstruction"));
    assert!(workflow.contains("inputs.operation_mode == 'reconstruction'"));
    assert!(workflow.contains("D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"));
    assert!(workflow.contains("RECONSTRUCTION_EXECUTOR_ADMISSION_VERIFIED"));
    assert!(workflow.contains("scripts/d1-current-reconstruction-materialize.py materialize"));
    assert!(workflow.contains("scripts/cloudflare-d1-bootstrap.py validate-empty"));
    assert!(workflow.contains("{kind:\"RECONSTRUCTION_APPLIED\""));
    assert!(workflow.contains("--reconstruction artifacts/d1-reconstruction/reconstruction.json"));
    assert!(workflow.contains("RECONSTRUCTION_POST_STATE_VERIFIED"));
    assert!(workflow.contains("COMPLETED_VERIFIED"));

    let reconstruction = workflow
        .split("  authorize_reconstruction:")
        .nth(1)
        .ok_or("missing reconstruction jobs")?;
    assert!(reconstruction.contains("  reconstruct:"));
    assert!(reconstruction.contains("environment: staging"));
    assert!(reconstruction.contains("CLOUDFLARE_OBSERVE_API_TOKEN"));
    assert!(reconstruction.contains("CLOUDFLARE_API_TOKEN"));
    assert!(reconstruction.contains("wrangler@4.94.0 d1 execute"));
    assert!(reconstruction.contains("--file artifacts/d1-reconstruction/bootstrap.sql"));
    assert!(!reconstruction.contains("d1 migrations apply"));
    assert!(!reconstruction.contains("d1 create"));
    assert!(!reconstruction.contains("d1 delete"));
    assert!(!reconstruction.contains("time-travel restore"));
    assert!(!reconstruction.contains("production"));

    let workflow_dir = root.join(".github/workflows");
    let duplicate = fs::read_dir(workflow_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("yml"))
        .filter(|entry| entry.file_name().to_string_lossy().contains("reconstruction"))
        .count();
    assert_eq!(duplicate, 0, "reconstruction must not acquire a second workflow owner");
    Ok(())
}
