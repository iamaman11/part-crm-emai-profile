use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_JSON_BYTES: usize = 1024 * 1024;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

type TestResult = Result<(), Box<dyn Error>>;

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> Result<Self, Box<dyn Error>> {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "opsctl-hosted-evidence-{}-{label}-{id}",
            std::process::id()
        ));
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Box::new(error)),
        }
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn write_json(&self, name: &str, value: &Value) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.path.join(name);
        fs::write(&path, serde_json::to_vec(value)?)?;
        Ok(path)
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.path.join(name);
        fs::write(&path, bytes)?;
        Ok(path)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.path));
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_evidence(action: &str, inputs: &[(&str, &Path)]) -> Result<Output, Box<dyn Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_opsctl"));
    command
        .arg("--root")
        .arg(repo_root())
        .arg("evidence")
        .arg(action);
    for (flag, path) in inputs {
        command.arg(flag).arg(path);
    }
    Ok(command.output()?)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected fail-closed command; stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !output.stderr.is_empty(),
        "failure must emit a structured error"
    );
}

fn context() -> Value {
    json!({
        "schema_version": 1,
        "evidence_kind": "credential_readiness",
        "payload_version": 1,
        "repository": "iamaman11/part-crm-emai-profile",
        "source_sha": "0123456789abcdef0123456789abcdef01234567",
        "source_ref": "refs/heads/main",
        "workflow": {
            "name": "Hosted probe",
            "workflow_ref": "iamaman11/part-crm-emai-profile/.github/workflows/probe.yml@refs/heads/main",
            "run_id": 42,
            "run_attempt": 1,
            "observation_job": "observe"
        },
        "environment": "staging",
        "observed_at": "2026-08-21T18:00:00Z",
        "provider_mutation": false,
        "production_mutation": false
    })
}

fn credential_payload() -> Value {
    json!({
        "provider": "cloudflare",
        "credential_identity": "staging-observe",
        "provider_metadata_identifier": "token-metadata-id",
        "status": "READY",
        "capabilities": ["workers.read", "d1.read"]
    })
}

fn resource_payload() -> Value {
    json!({
        "provider": "github",
        "resource_type": "workflow",
        "resource_id": ".github/workflows/hosted-evidence-publish.yml",
        "state": "ACTIVE",
        "revision": "v1",
        "enabled": true
    })
}

fn release_context(mutates: bool) -> Value {
    let mut value = context();
    value["evidence_kind"] = json!("release_set_transition");
    value["provider_mutation"] = json!(mutates);
    value
}

fn release_payload(decision: &str, compatibility: &str) -> Value {
    json!({
        "provider": "cloudflare",
        "profile_id": "rehearsal-core-v1",
        "previous_release_set_id": "release-a",
        "target_release_set_id": "release-b",
        "decision": decision,
        "compatibility": compatibility
    })
}

fn build_to_file(
    workspace: &TempWorkspace,
    context_value: &Value,
    payload_value: &Value,
    stem: &str,
) -> Result<(PathBuf, Vec<u8>), Box<dyn Error>> {
    let context_path = workspace.write_json(&format!("{stem}-context.json"), context_value)?;
    let raw_path = workspace.write_json(&format!("{stem}-raw.json"), payload_value)?;
    let output = run_evidence(
        "build",
        &[
            ("--raw-observation", &raw_path),
            ("--context-json", &context_path),
        ],
    )?;
    assert_success(&output);
    let evidence_path = workspace.write_bytes(&format!("{stem}-evidence.json"), &output.stdout)?;
    Ok((evidence_path, output.stdout))
}

fn assert_build_fails(
    workspace: &TempWorkspace,
    context_value: &Value,
    payload_value: &Value,
    stem: &str,
) -> TestResult {
    let context_path = workspace.write_json(&format!("{stem}-context.json"), context_value)?;
    let raw_path = workspace.write_json(&format!("{stem}-raw.json"), payload_value)?;
    let output = run_evidence(
        "build",
        &[
            ("--raw-observation", &raw_path),
            ("--context-json", &context_path),
        ],
    )?;
    assert_failure(&output);
    Ok(())
}

fn verify_with_context(
    workspace: &TempWorkspace,
    evidence_path: &Path,
    context_value: &Value,
    stem: &str,
) -> Result<Output, Box<dyn Error>> {
    let context_path = workspace.write_json(&format!("{stem}-expected.json"), context_value)?;
    run_evidence(
        "verify",
        &[
            ("--evidence-json", evidence_path),
            ("--context-json", &context_path),
        ],
    )
}

fn assert_verify_fails(
    workspace: &TempWorkspace,
    evidence_path: &Path,
    context_value: &Value,
    stem: &str,
) -> TestResult {
    let output = verify_with_context(workspace, evidence_path, context_value, stem)?;
    assert_failure(&output);
    Ok(())
}

#[test]
fn deterministic_build_round_trip_validate_inspect_and_verify() -> TestResult {
    let workspace = TempWorkspace::new("round-trip")?;
    let context_path = workspace.write_json("context.json", &context())?;
    let raw_path = workspace.write_json("raw.json", &credential_payload())?;
    let inputs = [
        ("--raw-observation", raw_path.as_path()),
        ("--context-json", context_path.as_path()),
    ];
    let first = run_evidence("build", &inputs)?;
    let second = run_evidence("build", &inputs)?;
    assert_success(&first);
    assert_success(&second);
    assert_eq!(
        first.stdout, second.stdout,
        "build must be byte deterministic"
    );

    let evidence_path = workspace.write_bytes("hosted-evidence.json", &first.stdout)?;
    let validate = run_evidence("validate", &[("--evidence-json", &evidence_path)])?;
    assert_success(&validate);
    let validate_json: Value = serde_json::from_slice(&validate.stdout)?;
    assert_eq!(validate_json["decision"], "VALID");

    let inspect = run_evidence("inspect", &[("--evidence-json", &evidence_path)])?;
    assert_success(&inspect);
    let inspect_json: Value = serde_json::from_slice(&inspect.stdout)?;
    assert_eq!(inspect_json["decision"], "INSPECTED");
    assert_eq!(inspect_json["canonical"], true);
    assert_eq!(
        inspect_json["envelope"]["payload"]["capabilities"],
        json!(["d1.read", "workers.read"])
    );

    let verify = verify_with_context(&workspace, &evidence_path, &context(), "exact")?;
    assert_success(&verify);
    let verify_json: Value = serde_json::from_slice(&verify.stdout)?;
    assert_eq!(verify_json["decision"], "VERIFIED");
    assert_eq!(verify_json["workflow_run_id"], 42);
    assert_eq!(verify_json["workflow_run_attempt"], 1);
    let digest = verify_json["sha256"]
        .as_str()
        .ok_or("missing verify digest")?;
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    Ok(())
}

#[test]
fn verify_rejects_every_independent_binding_mismatch() -> TestResult {
    let workspace = TempWorkspace::new("binding-mismatch")?;
    let (evidence_path, _) =
        build_to_file(&workspace, &context(), &credential_payload(), "binding")?;

    let mut value = context();
    value["repository"] = json!("example/other");
    assert_verify_fails(&workspace, &evidence_path, &value, "repository")?;

    let mut value = context();
    value["source_sha"] = json!("abcdef0123456789abcdef0123456789abcdef01");
    assert_verify_fails(&workspace, &evidence_path, &value, "source-sha")?;

    let mut value = context();
    value["source_ref"] = json!("refs/heads/other");
    assert_verify_fails(&workspace, &evidence_path, &value, "source-ref")?;

    let mut value = context();
    value["workflow"]["name"] = json!("Other workflow");
    assert_verify_fails(&workspace, &evidence_path, &value, "workflow-name")?;

    let mut value = context();
    value["workflow"]["workflow_ref"] =
        json!("iamaman11/part-crm-emai-profile/.github/workflows/other.yml@refs/heads/main");
    assert_verify_fails(&workspace, &evidence_path, &value, "workflow-ref")?;

    let mut value = context();
    value["workflow"]["run_id"] = json!(43);
    assert_verify_fails(&workspace, &evidence_path, &value, "run-id")?;

    let mut value = context();
    value["workflow"]["run_attempt"] = json!(2);
    assert_verify_fails(&workspace, &evidence_path, &value, "run-attempt")?;

    let mut value = context();
    value["workflow"]["observation_job"] = json!("other-job");
    assert_verify_fails(&workspace, &evidence_path, &value, "observation-job")?;

    let mut value = context();
    value["environment"] = json!("rehearsal");
    assert_verify_fails(&workspace, &evidence_path, &value, "environment")?;
    Ok(())
}

#[test]
fn noncanonical_serialized_bytes_fail_verification() -> TestResult {
    let workspace = TempWorkspace::new("noncanonical")?;
    let (_, canonical) = build_to_file(&workspace, &context(), &credential_payload(), "canonical")?;
    let parsed: Value = serde_json::from_slice(&canonical)?;
    let pretty = serde_json::to_string_pretty(&parsed)?;
    let evidence_path = workspace.write_bytes("pretty-evidence.json", pretty.as_bytes())?;
    assert_verify_fails(&workspace, &evidence_path, &context(), "pretty")?;
    Ok(())
}

#[test]
fn schema_kind_version_unknown_fields_and_duplicate_sets_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("schema")?;

    let mut value = context();
    value["schema_version"] = json!(2);
    assert_build_fails(&workspace, &value, &credential_payload(), "schema")?;

    let mut value = context();
    value["evidence_kind"] = json!("future_kind");
    assert_build_fails(&workspace, &value, &credential_payload(), "kind")?;

    let mut value = context();
    value["payload_version"] = json!(2);
    assert_build_fails(&workspace, &value, &credential_payload(), "version")?;

    let mut value = context();
    value["future"] = json!(true);
    assert_build_fails(&workspace, &value, &credential_payload(), "context-field")?;

    let mut payload = credential_payload();
    payload["future"] = json!(true);
    assert_build_fails(&workspace, &context(), &payload, "payload-field")?;

    let mut payload = credential_payload();
    payload["capabilities"] = json!(["workers.read", "workers.read"]);
    assert_build_fails(&workspace, &context(), &payload, "duplicate-capability")?;
    Ok(())
}

#[test]
fn secret_field_bearer_and_private_key_material_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("secrets")?;

    let mut payload = credential_payload();
    payload["api_token"] = json!("redacted-for-test");
    assert_build_fails(&workspace, &context(), &payload, "secret-field")?;

    let mut payload = credential_payload();
    payload["provider_metadata_identifier"] = json!("Bearer redacted-for-test");
    assert_build_fails(&workspace, &context(), &payload, "bearer")?;

    let mut payload = credential_payload();
    payload["provider_metadata_identifier"] =
        json!("-----BEGIN PRIVATE KEY-----redacted-----END PRIVATE KEY-----");
    assert_build_fails(&workspace, &context(), &payload, "private-key")?;
    Ok(())
}

#[test]
fn malformed_sha_ref_timestamp_run_identity_and_effects_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("invalid-context")?;

    let mut value = context();
    value["source_sha"] = json!("ABCDEF");
    assert_build_fails(&workspace, &value, &credential_payload(), "sha")?;

    let mut value = context();
    value["source_ref"] = json!("main");
    assert_build_fails(&workspace, &value, &credential_payload(), "ref")?;

    let mut value = context();
    value["observed_at"] = json!("2026-08-21 18:00:00Z");
    assert_build_fails(&workspace, &value, &credential_payload(), "timestamp")?;

    let mut value = context();
    value["workflow"]["run_id"] = json!(0);
    assert_build_fails(&workspace, &value, &credential_payload(), "run-id")?;

    let mut value = context();
    value["workflow"]["run_attempt"] = json!(0);
    assert_build_fails(&workspace, &value, &credential_payload(), "run-attempt")?;

    let mut value = context();
    value["environment"] = json!("production");
    value["production_mutation"] = json!(true);
    assert_build_fails(&workspace, &value, &credential_payload(), "production-only")?;

    let mut value = context();
    value["environment"] = json!("production");
    value["provider_mutation"] = json!(true);
    assert_build_fails(&workspace, &value, &credential_payload(), "provider-only")?;
    Ok(())
}

#[test]
fn oversized_input_and_observational_mutation_claims_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("bounded")?;
    let oversized = vec![b' '; MAX_JSON_BYTES + 1];
    let evidence_path = workspace.write_bytes("oversized.json", &oversized)?;
    assert_failure(&run_evidence(
        "validate",
        &[("--evidence-json", &evidence_path)],
    )?);

    let mut credential_context = context();
    credential_context["provider_mutation"] = json!(true);
    assert_build_fails(
        &workspace,
        &credential_context,
        &credential_payload(),
        "credential-mutation",
    )?;

    let mut resource_context = context();
    resource_context["evidence_kind"] = json!("hosted_resource_state");
    resource_context["provider_mutation"] = json!(true);
    assert_build_fails(
        &workspace,
        &resource_context,
        &resource_payload(),
        "resource-mutation",
    )?;
    Ok(())
}

#[test]
fn hosted_resource_state_variant_builds_and_verifies() -> TestResult {
    let workspace = TempWorkspace::new("resource")?;
    let mut context_value = context();
    context_value["evidence_kind"] = json!("hosted_resource_state");
    let (evidence_path, _) =
        build_to_file(&workspace, &context_value, &resource_payload(), "resource")?;
    let output = verify_with_context(&workspace, &evidence_path, &context_value, "resource")?;
    assert_success(&output);
    Ok(())
}

#[test]
fn release_transition_no_change_blocked_and_mutation_semantics_are_typed() -> TestResult {
    let workspace = TempWorkspace::new("release")?;

    for (index, (decision, compatibility)) in
        [("NO_CHANGE", "INCOMPATIBLE"), ("BLOCKED", "UNKNOWN")]
            .into_iter()
            .enumerate()
    {
        let context_value = release_context(false);
        let payload = release_payload(decision, compatibility);
        let stem = format!("nonmutating-{index}");
        let (evidence_path, _) = build_to_file(&workspace, &context_value, &payload, &stem)?;
        let output = verify_with_context(&workspace, &evidence_path, &context_value, &stem)?;
        assert_success(&output);
    }

    for (index, decision) in ["APPLIED", "ROLLED_BACK"].into_iter().enumerate() {
        let context_value = release_context(true);
        let stem = format!("mutation-{index}");
        assert_build_fails(
            &workspace,
            &context_value,
            &release_payload(decision, "INCOMPATIBLE"),
            &format!("{stem}-blocked"),
        )?;
        let (_, bytes) = build_to_file(
            &workspace,
            &context_value,
            &release_payload(decision, "COMPATIBLE"),
            &format!("{stem}-compatible"),
        )?;
        assert!(!bytes.is_empty());
    }

    let mut production = release_context(true);
    production["environment"] = json!("production");
    production["production_mutation"] = json!(true);
    let (_, bytes) = build_to_file(
        &workspace,
        &production,
        &release_payload("APPLIED", "COMPATIBLE"),
        "production-compatible",
    )?;
    assert!(!bytes.is_empty());
    Ok(())
}

#[test]
fn cli_rejects_forbidden_argument_combinations_and_unknown_actions() -> TestResult {
    let workspace = TempWorkspace::new("cli")?;
    let context_path = workspace.write_json("context.json", &context())?;
    let raw_path = workspace.write_json("raw.json", &credential_payload())?;
    let (evidence_path, _) = build_to_file(&workspace, &context(), &credential_payload(), "cli")?;

    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw_path),
            ("--context-json", &context_path),
            ("--evidence-json", &evidence_path),
        ],
    )?);
    assert_failure(&run_evidence(
        "validate",
        &[
            ("--evidence-json", &evidence_path),
            ("--context-json", &context_path),
        ],
    )?);
    assert_failure(&run_evidence(
        "inspect",
        &[
            ("--evidence-json", &evidence_path),
            ("--raw-observation", &raw_path),
        ],
    )?);
    assert_failure(&run_evidence(
        "verify",
        &[
            ("--evidence-json", &evidence_path),
            ("--context-json", &context_path),
            ("--raw-observation", &raw_path),
        ],
    )?);
    assert_failure(&run_evidence("build", &[("--raw-observation", &raw_path)])?);
    assert_failure(&run_evidence("validate", &[])?);
    assert_failure(&run_evidence("publish", &[])?);
    Ok(())
}

#[test]
fn evidence_policy_module_has_no_provider_network_or_process_authority() {
    let source = include_str!("../src/evidence.rs");
    for forbidden in [
        "std::process",
        "Command::new",
        "reqwest",
        "ureq",
        "std::net",
        "TcpStream",
        "curl ",
        "wrangler",
        "cloudflare.com",
        "api.github.com",
    ] {
        assert!(
            !source.contains(forbidden),
            "offline evidence policy unexpectedly contains forbidden authority marker: {forbidden}"
        );
    }
}
