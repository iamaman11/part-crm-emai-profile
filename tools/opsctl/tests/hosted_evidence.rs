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
    assert!(!output.stderr.is_empty(), "failure must emit a structured error");
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

fn release_context(decision_mutates: bool) -> Value {
    let mut value = context();
    value["evidence_kind"] = json!("release_set_transition");
    value["provider_mutation"] = json!(decision_mutates);
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
) -> Result<(PathBuf, Vec<u8>), Box<dyn Error>> {
    let context_path = workspace.write_json("context.json", context_value)?;
    let raw_path = workspace.write_json("raw.json", payload_value)?;
    let output = run_evidence(
        "build",
        &[
            ("--raw-observation", &raw_path),
            ("--context-json", &context_path),
        ],
    )?;
    assert_success(&output);
    let evidence_path = workspace.write_bytes("hosted-evidence.json", &output.stdout)?;
    Ok((evidence_path, output.stdout))
}

fn verify_with_context(
    workspace: &TempWorkspace,
    evidence_path: &Path,
    context_value: &Value,
    name: &str,
) -> Result<Output, Box<dyn Error>> {
    let context_path = workspace.write_json(name, context_value)?;
    run_evidence(
        "verify",
        &[
            ("--evidence-json", evidence_path),
            ("--context-json", &context_path),
        ],
    )
}

#[test]
fn deterministic_build_round_trip_validate_inspect_and_verify() -> TestResult {
    let workspace = TempWorkspace::new("round-trip")?;
    let first_context = workspace.write_json("context-a.json", &context())?;
    let raw = workspace.write_json("raw-a.json", &credential_payload())?;
    let first = run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &first_context),
        ],
    )?;
    let second = run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &first_context),
        ],
    )?;
    assert_success(&first);
    assert_success(&second);
    assert_eq!(first.stdout, second.stdout, "build must be byte deterministic");

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

    let verify = verify_with_context(&workspace, &evidence_path, &context(), "expected.json")?;
    assert_success(&verify);
    let verify_json: Value = serde_json::from_slice(&verify.stdout)?;
    assert_eq!(verify_json["decision"], "VERIFIED");
    assert_eq!(verify_json["workflow_run_id"], 42);
    assert_eq!(verify_json["workflow_run_attempt"], 1);
    let digest = verify_json["sha256"].as_str().ok_or("missing verify digest")?;
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    Ok(())
}

#[test]
fn verify_rejects_repository_source_workflow_run_job_and_environment_mismatches() -> TestResult {
    let workspace = TempWorkspace::new("binding-mismatch")?;
    let (evidence_path, _) = build_to_file(&workspace, &context(), &credential_payload())?;

    let mut cases = Vec::new();
    let mut value = context();
    value["repository"] = json!("example/other");
    cases.push(value);
    let mut value = context();
    value["source_sha"] = json!("abcdef0123456789abcdef0123456789abcdef01");
    cases.push(value);
    let mut value = context();
    value["source_ref"] = json!("refs/heads/other");
    cases.push(value);
    let mut value = context();
    value["workflow"]["name"] = json!("Other workflow");
    cases.push(value);
    let mut value = context();
    value["workflow"]["workflow_ref"] = json!(
        "iamaman11/part-crm-emai-profile/.github/workflows/other.yml@refs/heads/main"
    );
    cases.push(value);
    let mut value = context();
    value["workflow"]["run_id"] = json!(43);
    cases.push(value);
    let mut value = context();
    value["workflow"]["run_attempt"] = json!(2);
    cases.push(value);
    let mut value = context();
    value["workflow"]["observation_job"] = json!("other-job");
    cases.push(value);
    let mut value = context();
    value["environment"] = json!("rehearsal");
    cases.push(value);

    for (index, case) in cases.iter().enumerate() {
        let name = format!("mismatch-{index}.json");
        let output = verify_with_context(&workspace, &evidence_path, case, &name)?;
        assert_failure(&output);
    }
    Ok(())
}

#[test]
fn verify_rejects_noncanonical_serialized_bytes() -> TestResult {
    let workspace = TempWorkspace::new("noncanonical")?;
    let (_, canonical) = build_to_file(&workspace, &context(), &credential_payload())?;
    let parsed: Value = serde_json::from_slice(&canonical)?;
    let pretty = serde_json::to_string_pretty(&parsed)?;
    let evidence_path = workspace.write_bytes("pretty-evidence.json", pretty.as_bytes())?;
    let output = verify_with_context(&workspace, &evidence_path, &context(), "expected.json")?;
    assert_failure(&output);
    Ok(())
}

#[test]
fn schema_kind_version_and_unknown_fields_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("schema-dispatch")?;

    let mut unknown_schema = context();
    unknown_schema["schema_version"] = json!(2);
    let context_path = workspace.write_json("schema.json", &unknown_schema)?;
    let raw = workspace.write_json("raw.json", &credential_payload())?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut unknown_kind = context();
    unknown_kind["evidence_kind"] = json!("future_kind");
    let context_path = workspace.write_json("kind.json", &unknown_kind)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut unknown_version = context();
    unknown_version["payload_version"] = json!(2);
    let context_path = workspace.write_json("version.json", &unknown_version)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut context_unknown_field = context();
    context_unknown_field["future"] = json!(true);
    let context_path = workspace.write_json("context-field.json", &context_unknown_field)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut payload_unknown_field = credential_payload();
    payload_unknown_field["future"] = json!(true);
    let context_path = workspace.write_json("context-valid.json", &context())?;
    let raw_unknown = workspace.write_json("raw-unknown.json", &payload_unknown_field)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw_unknown),
            ("--context-json", &context_path)
        ]
    )?);
    Ok(())
}

#[test]
fn duplicate_set_entries_and_secret_material_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("secret-rejection")?;
    let context_path = workspace.write_json("context.json", &context())?;

    let mut duplicate = credential_payload();
    duplicate["capabilities"] = json!(["workers.read", "workers.read"]);
    let raw = workspace.write_json("duplicate.json", &duplicate)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut secret_field = credential_payload();
    secret_field["api_token"] = json!("redacted-for-test");
    let raw = workspace.write_json("secret-field.json", &secret_field)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut bearer = credential_payload();
    bearer["provider_metadata_identifier"] = json!("Bearer redacted-for-test");
    let raw = workspace.write_json("bearer.json", &bearer)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);

    let mut private_key = credential_payload();
    private_key["provider_metadata_identifier"] = json!(
        "-----BEGIN PRIVATE KEY-----redacted-----END PRIVATE KEY-----"
    );
    let raw = workspace.write_json("private-key.json", &private_key)?;
    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
            ("--context-json", &context_path)
        ]
    )?);
    Ok(())
}

#[test]
fn malformed_sha_ref_timestamp_and_effect_flags_fail_closed() -> TestResult {
    let workspace = TempWorkspace::new("context-validation")?;
    let raw = workspace.write_json("raw.json", &credential_payload())?;

    let mut cases = Vec::new();
    let mut value = context();
    value["source_sha"] = json!("ABCDEF");
    cases.push(value);
    let mut value = context();
    value["source_ref"] = json!("main");
    cases.push(value);
    let mut value = context();
    value["observed_at"] = json!("2026-08-21 18:00:00Z");
    cases.push(value);
    let mut value = context();
    value["workflow"]["run_id"] = json!(0);
    cases.push(value);
    let mut value = context();
    value["workflow"]["run_attempt"] = json!(0);
    cases.push(value);
    let mut value = context();
    value["environment"] = json!("production");
    value["production_mutation"] = json!(true);
    cases.push(value);
    let mut value = context();
    value["environment"] = json!("production");
    value["provider_mutation"] = json!(true);
    cases.push(value);

    for (index, case) in cases.iter().enumerate() {
        let name = format!("invalid-context-{index}.json");
        let path = workspace.write_json(&name, case)?;
        let output = run_evidence(
            "build",
            &[
                ("--raw-observation", &raw),
                ("--context-json", &path),
            ],
        )?;
        assert_failure(&output);
    }
    Ok(())
}

#[test]
fn oversized_json_is_rejected_before_policy_parsing() -> TestResult {
    let workspace = TempWorkspace::new("oversized")?;
    let oversized = vec![b' '; MAX_JSON_BYTES + 1];
    let evidence_path = workspace.write_bytes("oversized.json", &oversized)?;
    let output = run_evidence("validate", &[("--evidence-json", &evidence_path)])?;
    assert_failure(&output);
    Ok(())
}

#[test]
fn observational_payloads_cannot_claim_provider_mutation() -> TestResult {
    let workspace = TempWorkspace::new("observational-effects")?;
    for kind in ["credential_readiness", "hosted_resource_state"] {
        let mut context_value = context();
        context_value["evidence_kind"] = json!(kind);
        context_value["provider_mutation"] = json!(true);
        let payload = if kind == "credential_readiness" {
            credential_payload()
        } else {
            json!({
                "provider": "github",
                "resource_type": "workflow",
                "resource_id": "hosted-evidence-publish.yml",
                "state": "ACTIVE",
                "revision": null,
                "enabled": true
            })
        };
        let context_path = workspace.write_json(&format!("{kind}-context.json"), &context_value)?;
        let raw = workspace.write_json(&format!("{kind}-raw.json"), &payload)?;
        let output = run_evidence(
            "build",
            &[
                ("--raw-observation", &raw),
                ("--context-json", &context_path),
            ],
        )?;
        assert_failure(&output);
    }
    Ok(())
}

#[test]
fn hosted_resource_state_variant_builds_and_verifies() -> TestResult {
    let workspace = TempWorkspace::new("resource-state")?;
    let mut context_value = context();
    context_value["evidence_kind"] = json!("hosted_resource_state");
    let payload = json!({
        "provider": "github",
        "resource_type": "workflow",
        "resource_id": ".github/workflows/hosted-evidence-publish.yml",
        "state": "ACTIVE",
        "revision": "v1",
        "enabled": true
    });
    let (evidence_path, _) = build_to_file(&workspace, &context_value, &payload)?;
    let output = verify_with_context(
        &workspace,
        &evidence_path,
        &context_value,
        "expected-resource.json",
    )?;
    assert_success(&output);
    Ok(())
}

#[test]
fn release_transition_no_change_blocked_and_mutation_semantics_are_typed() -> TestResult {
    let workspace = TempWorkspace::new("release-effects")?;

    for (index, (decision, compatibility)) in [
        ("NO_CHANGE", "INCOMPATIBLE"),
        ("BLOCKED", "UNKNOWN"),
    ]
    .iter()
    .enumerate()
    {
        let context_value = release_context(false);
        let payload = release_payload(decision, compatibility);
        let (evidence_path, _) = build_to_file(&workspace, &context_value, &payload)?;
        let output = verify_with_context(
            &workspace,
            &evidence_path,
            &context_value,
            &format!("expected-nonmutating-{index}.json"),
        )?;
        assert_success(&output);
    }

    for (index, decision) in ["APPLIED", "ROLLED_BACK"].iter().enumerate() {
        let context_value = release_context(true);
        let incompatible = release_payload(decision, "INCOMPATIBLE");
        let context_path = workspace.write_json(&format!("mutating-context-{index}.json"), &context_value)?;
        let raw = workspace.write_json(&format!("incompatible-{index}.json"), &incompatible)?;
        assert_failure(&run_evidence(
            "build",
            &[
                ("--raw-observation", &raw),
                ("--context-json", &context_path)
            ]
        )?);

        let compatible = release_payload(decision, "COMPATIBLE");
        let (_, bytes) = build_to_file(&workspace, &context_value, &compatible)?;
        assert!(!bytes.is_empty());
    }

    let mut production_context = release_context(true);
    production_context["environment"] = json!("production");
    production_context["production_mutation"] = json!(true);
    let (_, bytes) = build_to_file(
        &workspace,
        &production_context,
        &release_payload("APPLIED", "COMPATIBLE"),
    )?;
    assert!(!bytes.is_empty());
    Ok(())
}

#[test]
fn cli_rejects_forbidden_argument_combinations_and_unknown_actions() -> TestResult {
    let workspace = TempWorkspace::new("cli-matrix")?;
    let context_path = workspace.write_json("context.json", &context())?;
    let raw = workspace.write_json("raw.json", &credential_payload())?;
    let (evidence_path, _) = build_to_file(&workspace, &context(), &credential_payload())?;

    assert_failure(&run_evidence(
        "build",
        &[
            ("--raw-observation", &raw),
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
            ("--raw-observation", &raw),
        ],
    )?);
    assert_failure(&run_evidence(
        "verify",
        &[
            ("--evidence-json", &evidence_path),
            ("--context-json", &context_path),
            ("--raw-observation", &raw),
        ],
    )?);
    assert_failure(&run_evidence("build", &[("--raw-observation", &raw)])?);
    assert_failure(&run_evidence("validate", &[])?);
    assert_failure(&run_evidence("publish", &[])?);
    Ok(())
}

#[test]
fn evidence_policy_module_contains_no_provider_network_or_process_authority() {
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
