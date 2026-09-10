use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TREE_SHA: &str = "dddddddddddddddddddddddddddddddddddddddd";
const RELEASE_SET_ID: &str =
    "release-set-v3-sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const PROMOTION_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PROFILE_ID: &str = "rehearsal-core-v2";
const SECRET_SENTINEL: &str = "O0_E2_SECRET_SENTINEL_MUST_NOT_BE_SERIALIZED";

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unique_temp_dir(label: &str) -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock must be after UNIX epoch: {error}"))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "opsctl-o0-e2-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("create O0 E2 fixture directory {}: {error}", path.display()))?;
    Ok(path)
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("serialize fixture {}: {error}", path.display()))?;
    fs::write(path, bytes).map_err(|error| format!("write fixture {}: {error}", path.display()))
}

fn manual_phase_state(
    phase: &str,
    authorization_bound: bool,
    provider_mutation_started: bool,
    provider_mutation_executed: bool,
    failed_boundary: Option<&str>,
) -> Value {
    json!({
        "schema_version": 1,
        "kind": "AR11_MANUAL_PHASE_STATE",
        "phase": phase,
        "source_sha": SOURCE_SHA,
        "release_set_id": RELEASE_SET_ID,
        "promotion_id": PROMOTION_ID,
        "authorization_bound": authorization_bound,
        "provider_mutation_started": provider_mutation_started,
        "provider_mutation_executed": provider_mutation_executed,
        "production_mutation_executed": false,
        "failed_boundary": failed_boundary,
        "step_outcomes": {
            "fixture": if failed_boundary.is_some() { "failure" } else { "success" }
        }
    })
}

fn promotion_verify(decision: &str, blocker: Option<&str>) -> Value {
    let blockers = blocker.map_or_else(Vec::<String>::new, |value| vec![value.to_string()]);
    json!({
        "schema_version": 1,
        "command": "promotion.verify",
        "decision": decision,
        "verified": decision == "VERIFIED",
        "environment": "staging",
        "target_release_set_id": RELEASE_SET_ID,
        "target_capability_profile_id": PROFILE_ID,
        "blockers": blockers,
        "mutation_executed": false
    })
}

fn run_manual_cli_case(
    label: &str,
    resolve: &Value,
    mutation: &Value,
    post: &Value,
    verify: Option<&Value>,
) -> Result<Value, String> {
    let root = repository_root();
    let fixture_dir = unique_temp_dir(label)?;
    let resolve_path = fixture_dir.join("manual-resolve-state.json");
    let mutation_path = fixture_dir.join("manual-mutation-state.json");
    let post_path = fixture_dir.join("manual-post-verify-state.json");
    let verify_path = fixture_dir.join("promotion-verify.json");
    let outcome_path = fixture_dir.join("promotion-operational-outcome.json");

    write_json(&resolve_path, resolve)?;
    write_json(&mutation_path, mutation)?;
    write_json(&post_path, post)?;
    if let Some(value) = verify {
        write_json(&verify_path, value)?;
    }

    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = Command::new(python)
        .arg(root.join("scripts/promotion-operational-outcome-ar11.py"))
        .arg("--mode")
        .arg("manual")
        .arg("--source-sha")
        .arg(SOURCE_SHA)
        .arg("--tree-sha")
        .arg(TREE_SHA)
        .arg("--environment")
        .arg("staging")
        .arg("--profile-id")
        .arg(PROFILE_ID)
        .arg("--resolve-state-json")
        .arg(&resolve_path)
        .arg("--mutation-state-json")
        .arg(&mutation_path)
        .arg("--post-verify-state-json")
        .arg(&post_path)
        .arg("--promotion-verify-json")
        .arg(&verify_path)
        .arg("--evidence-artifact")
        .arg("o0-e2-credential-free-fixture")
        .arg("--output")
        .arg(&outcome_path)
        .env("CLOUDFLARE_API_TOKEN", SECRET_SENTINEL)
        .env("CLOUDFLARE_ACCOUNT_ID", SECRET_SENTINEL)
        .output()
        .map_err(|error| format!("execute manual OperationalOutcome CLI for {label}: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "manual OperationalOutcome CLI failed for {label}:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    if !outcome_path.is_file() {
        return Err(format!(
            "terminal outcome must be published before any disposition enforcement for {label}"
        ));
    }
    let raw = fs::read_to_string(&outcome_path)
        .map_err(|error| format!("read terminal outcome for {label}: {error}"))?;
    if raw.contains(SECRET_SENTINEL) {
        return Err(format!(
            "credential environment material leaked into terminal outcome for {label}"
        ));
    }
    let outcome: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("parse terminal outcome for {label}: {error}"))?;

    if outcome["contract"] != "PROMOTION_OPERATOR_OUTCOME_V1"
        || outcome["procedure"] != "AR11_RELEASE_SET_PROMOTION"
        || outcome["source_sha"] != SOURCE_SHA
        || outcome["tree_sha"] != TREE_SHA
        || outcome["production_mutation_executed"] != false
    {
        return Err(format!(
            "manual terminal outcome identity/effect contract drifted for {label}: {outcome}"
        ));
    }

    fs::remove_dir_all(&fixture_dir).map_err(|error| {
        format!(
            "remove O0 E2 fixture directory {}: {error}",
            fixture_dir.display()
        )
    })?;
    Ok(outcome)
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
fn ar11_manual_operational_outcome_cli_proves_composed_effect_boundaries_without_credentials()
-> Result<(), String> {
    let resolve_ready = manual_phase_state("RESOLVE_VERIFY", true, false, false, None);
    let mutation_not_started =
        manual_phase_state("MUTATE", true, false, false, Some("EXACT_CURRENT_FENCE"));
    let post_skipped = manual_phase_state(
        "POST_VERIFY",
        true,
        false,
        false,
        Some("POST_VERIFY_SKIPPED"),
    );

    let resolve_auth_failed = manual_phase_state(
        "RESOLVE_VERIFY",
        false,
        false,
        false,
        Some("AUTHORIZATION_BINDING"),
    );
    let outcome = run_manual_cli_case(
        "authorization-failure",
        &resolve_auth_failed,
        &mutation_not_started,
        &post_skipped,
        None,
    )?;
    assert_eq!(outcome["status"], "FAILED_NO_EFFECT");
    assert_eq!(outcome["phase"], "AUTHORIZATION_BINDING");
    assert_eq!(outcome["effect_state"], "EXACT_NO_EFFECT");
    assert_eq!(outcome["provider_mutation_started"], false);
    assert_eq!(outcome["provider_mutation_executed"], false);

    let outcome = run_manual_cli_case(
        "exact-current-fence-failure",
        &resolve_ready,
        &mutation_not_started,
        &post_skipped,
        None,
    )?;
    assert_eq!(outcome["status"], "FAILED_NO_EFFECT");
    assert_eq!(outcome["phase"], "EXACT_CURRENT_FENCE");
    assert_eq!(outcome["effect_state"], "EXACT_NO_EFFECT");
    assert_eq!(outcome["provider_mutation_started"], false);

    let mutation_deploy_failed =
        manual_phase_state("MUTATE", true, true, false, Some("PROVIDER_DEPLOY"));
    let post_after_started = manual_phase_state(
        "POST_VERIFY",
        true,
        true,
        false,
        Some("POST_VERIFY_SKIPPED"),
    );
    let outcome = run_manual_cli_case(
        "deploy-invocation-failure",
        &resolve_ready,
        &mutation_deploy_failed,
        &post_after_started,
        None,
    )?;
    assert_eq!(outcome["status"], "RECOVERY_REQUIRED");
    assert_eq!(outcome["phase"], "PROVIDER_DEPLOY");
    assert_eq!(outcome["effect_state"], "UNKNOWN");
    assert_eq!(outcome["provider_mutation_started"], true);
    assert_eq!(outcome["provider_mutation_executed"], false);
    assert_eq!(outcome["recovery_owner"], "AR-14");

    let mutation_succeeded = manual_phase_state("MUTATE", true, true, true, None);
    let post_succeeded = manual_phase_state("POST_VERIFY", true, true, true, None);
    for (decision, blocker) in [
        ("DRIFTED", "FIXTURE_DEPLOYED_RELEASE_SET_MISMATCH"),
        ("INCOMPLETE", "FIXTURE_REQUIRED_BINDING_MISSING"),
        ("UNKNOWN", "FIXTURE_PROVIDER_STATE_UNKNOWN"),
    ] {
        let verify = promotion_verify(decision, Some(blocker));
        let outcome = run_manual_cli_case(
            &format!("post-verify-{}", decision.to_ascii_lowercase()),
            &resolve_ready,
            &mutation_succeeded,
            &post_succeeded,
            Some(&verify),
        )?;
        assert_eq!(outcome["status"], "RECOVERY_REQUIRED");
        assert_eq!(outcome["phase"], "PROMOTION_VERIFY");
        assert_eq!(outcome["effect_state"], "RECOVERY_REQUIRED");
        assert_eq!(outcome["owner"], "opsctl.promotion.verify");
        assert_eq!(outcome["owner_contract"], "promotion.verify/v1");
        assert_eq!(outcome["owner_reason_code"], blocker);
        assert_eq!(outcome["owner_diagnostic"]["blockers"][0], blocker);
        assert_eq!(outcome["recovery_owner"], "AR-14");
    }

    let verified = promotion_verify("VERIFIED", None);
    let outcome = run_manual_cli_case(
        "verified-success",
        &resolve_ready,
        &mutation_succeeded,
        &post_succeeded,
        Some(&verified),
    )?;
    assert_eq!(outcome["status"], "COMPLETED");
    assert_eq!(outcome["phase"], "POST_VERIFY");
    assert_eq!(outcome["effect_state"], "EFFECT_VERIFIED");
    assert_eq!(outcome["owner"], "opsctl.promotion.verify");
    assert_eq!(outcome["owner_reason_code"], "VERIFIED");
    assert_eq!(outcome["provider_mutation_started"], true);
    assert_eq!(outcome["provider_mutation_executed"], true);
    assert!(outcome.get("recovery_owner").is_none());

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
