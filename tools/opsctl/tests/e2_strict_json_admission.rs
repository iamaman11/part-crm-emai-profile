use opsctl::promotion::snapshot::DeploymentSnapshot;
use opsctl::release::compatibility::CompatibilityEvidence;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn deployment_snapshot_load_rejects_duplicate_member_before_typed_decode() {
    let error = DeploymentSnapshot::load(&fixture("e2-deployment-snapshot-duplicate-member.json"))
        .expect_err("duplicate DeploymentSnapshot members must fail closed");
    let message = error.to_string();
    assert!(
        message.contains("DeploymentSnapshot strict JSON admission failed"),
        "unexpected error boundary: {message}"
    );
    assert!(
        message.contains("duplicate JSON object member: environment"),
        "duplicate-member proof missing: {message}"
    );
}

#[test]
fn release_compatibility_load_rejects_duplicate_member_before_typed_decode() {
    let error = CompatibilityEvidence::load(&fixture(
        "e2-release-compatibility-evidence-duplicate-member.json",
    ))
    .expect_err("duplicate release compatibility members must fail closed");
    let message = error.to_string();
    assert!(
        message.contains("release compatibility evidence strict JSON admission failed"),
        "unexpected error boundary: {message}"
    );
    assert!(
        message.contains("duplicate JSON object member: decision"),
        "duplicate-member proof missing: {message}"
    );
}
