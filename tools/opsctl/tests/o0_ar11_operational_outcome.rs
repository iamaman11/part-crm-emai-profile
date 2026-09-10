use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ar11_operational_outcome_fixture_matrix_is_credential_free_and_zero_effect() {
    let root = repository_root();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = Command::new(python)
        .arg(root.join("scripts/promotion-operational-outcome-ar11.py"))
        .arg("--self-test")
        .output()
        .expect("python runtime must be available for repository fixture proof");
    assert!(
        output.status.success(),
        "OperationalOutcome fixture matrix failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("AR11 promotion OperationalOutcome fixture matrix passed.")
    );
}

#[test]
fn ar11_workflow_terminalizes_after_owner_capture_and_before_final_assertion() {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release-set-promotion.yml"))
        .expect("release-set-promotion workflow must exist");

    assert!(workflow.contains("promotion-operational-outcome-ar11.py"));
    assert!(workflow.contains("promotion-operational-outcome.json"));
    assert!(workflow.contains("PROMOTION_OPERATOR_OUTCOME_V1"));
    assert!(workflow.contains("if: always()"));
    assert!(workflow.contains("read_only_ready"));
    assert!(workflow.contains("READ_ONLY_READY"));
    assert!(workflow.contains("Enforce terminal AR11 disposition after evidence publication"));

    let terminalize = workflow
        .find("Terminalize one lossless AR11 OperationalOutcome")
        .expect("terminal outcome step must exist");
    let enforce = workflow
        .find("Enforce terminal AR11 disposition after evidence publication")
        .expect("final enforcement step must exist");
    assert!(terminalize < enforce, "outcome must survive before final assertion");
}

#[test]
fn repository_contract_states_permanent_lossless_owner_projection_rule() {
    let root = repository_root();
    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md must exist");
    assert!(agents.contains("one natural-owner verdict -> one lossless terminal OperationalOutcome"));
    assert!(agents.contains("Capture before assert"));
}
