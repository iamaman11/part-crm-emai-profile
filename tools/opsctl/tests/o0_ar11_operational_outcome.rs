use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ar11_operational_outcome_fixture_matrix_is_credential_free_and_zero_effect() -> Result<(), String>
{
    let root = repository_root();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = Command::new(python)
        .arg(root.join("scripts/promotion-operational-outcome-ar11.py"))
        .arg("--self-test")
        .output()
        .map_err(|error| {
            format!("python runtime must be available for repository fixture proof: {error}")
        })?;
    if !output.status.success() {
        return Err(format!(
            "OperationalOutcome fixture matrix failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("AR11 promotion OperationalOutcome fixture matrix passed.")
    );
    Ok(())
}

#[test]
fn ar11_workflow_terminalizes_after_owner_capture_and_before_final_assertion() -> Result<(), String>
{
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release-set-promotion.yml"))
        .map_err(|error| format!("release-set-promotion workflow must exist: {error}"))?;

    assert!(workflow.contains("promotion-operational-outcome-ar11.py"));
    assert!(workflow.contains("promotion-operational-outcome.json"));
    assert!(workflow.contains("PROMOTION_OPERATOR_OUTCOME_V1"));
    assert!(workflow.contains("if: always()"));
    assert!(workflow.contains("read_only_ready"));
    assert!(workflow.contains("READ_ONLY_READY"));
    assert!(workflow.contains("Enforce terminal AR11 disposition after evidence publication"));
    assert!(workflow.contains("id: node_runtime"));
    assert!(workflow.contains("NODE_RUNTIME_SETUP"));
    assert!(workflow.contains("manual-outcome:"));
    assert!(workflow.contains("AR11_RELEASE_SET_PROMOTION"));
    assert!(workflow.contains("Mark provider mutation invocation boundary"));
    assert!(!workflow.contains("promotion-verify.json\" | jq -e '.verified == true"));

    let terminalize = workflow
        .find("Terminalize one lossless AR11 OperationalOutcome")
        .ok_or_else(|| "terminal outcome step must exist".to_string())?;
    let enforce = workflow
        .find("Enforce terminal AR11 disposition after evidence publication")
        .ok_or_else(|| "final enforcement step must exist".to_string())?;
    assert!(
        terminalize < enforce,
        "outcome must survive before final assertion"
    );
    let mutation_start = workflow
        .find("Mark provider mutation invocation boundary")
        .ok_or_else(|| "provider mutation invocation marker must exist".to_string())?;
    let deploy = workflow
        .find("Deploy exact Release Set v3 bits after all fences")
        .ok_or_else(|| "provider deploy step must exist".to_string())?;
    let manual_terminalize = workflow
        .find("Terminalize one lossless manual AR11 OperationalOutcome")
        .ok_or_else(|| "manual terminal outcome step must exist".to_string())?;
    let manual_enforce = workflow
        .find("Enforce terminal manual AR11 disposition after evidence publication")
        .ok_or_else(|| "manual final enforcement step must exist".to_string())?;
    assert!(
        mutation_start < deploy,
        "effect-start marker must precede deploy invocation"
    );
    assert!(
        deploy < manual_terminalize,
        "manual terminal outcome must observe deploy result"
    );
    assert!(
        manual_terminalize < manual_enforce,
        "manual outcome must be published before final assertion"
    );
    Ok(())
}

#[test]
fn repository_contract_states_permanent_lossless_owner_projection_rule() -> Result<(), String> {
    let root = repository_root();
    let agents = fs::read_to_string(root.join("AGENTS.md"))
        .map_err(|error| format!("AGENTS.md must exist: {error}"))?;
    assert!(
        agents.contains("one natural-owner verdict -> one lossless terminal OperationalOutcome")
    );
    assert!(agents.contains("Capture before assert"));
    Ok(())
}
