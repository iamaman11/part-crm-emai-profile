use opsctl::canonical::{canonical_json, sha256_hex};
use opsctl::d1::{
    D1ContractTransitionVerificationRequest, contract_transition_verify, repository_projection,
};
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PREDECESSOR_REVISION: &str = "0031_device_binding_governance.sql";
const CONTRACT_REVISION: &str = "0032_pas2_payload_fingerprint_contract.sql";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opsctl-d1-contract-transition-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn canonical_sha256(value: &Value) -> Result<String, Box<dyn Error>> {
    Ok(sha256_hex(canonical_json(value)?.as_bytes()))
}

fn catalog<'a>(projection: &'a Value) -> Result<&'a Value, Box<dyn Error>> {
    projection["components"]
        .as_array()
        .and_then(|components| {
            components
                .iter()
                .find(|component| component["component_id"] == "catalog")
        })
        .ok_or_else(|| "typed Catalog projection is missing".into())
}

fn ledger(names: &[String]) -> Value {
    json!({
        "rows": names
            .iter()
            .enumerate()
            .map(|(index, name)| json!({"id": index + 1, "name": name}))
            .collect::<Vec<_>>()
    })
}

#[test]
fn public_post_contract_verify_accepts_exact_0032_and_rejects_non_transition()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let projection: Value = serde_json::from_str(&repository_projection(&root)?)?;
    let catalog = catalog(&projection)?;
    let sources = catalog["executable_migration_sources"]
        .as_array()
        .ok_or("Catalog executable migration sources are missing")?;
    let names = sources
        .iter()
        .map(|source| {
            source["migration_file"]
                .as_str()
                .map(str::to_owned)
                .ok_or("Catalog migration filename is missing")
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(names.len(), 32);
    assert_eq!(names[30], PREDECESSOR_REVISION);
    assert_eq!(names[31], CONTRACT_REVISION);

    let predecessor_ledger = ledger(&names[..31]);
    let post_ledger = ledger(&names);
    let release_manifest = json!({
        "schema_contract": catalog["release_schema_contract"].clone()
    });
    let repository_identity_sha256 = projection["repository_identity_sha256"]
        .as_str()
        .ok_or("repository identity is missing")?
        .to_owned();
    let release_manifest_sha256 = canonical_sha256(&release_manifest)?;
    let predecessor_ledger_sha256 = canonical_sha256(&predecessor_ledger)?;
    let source_sha = "e".repeat(40);
    let release_set_id = format!("release-set-v3-sha256-{}", "d".repeat(64));
    let evidence = json!({
        "schema_version": 1,
        "kind": "D1_CONTRACT_TRANSITION_EVIDENCE",
        "environment": "staging",
        "component": "catalog",
        "predecessor_revision": PREDECESSOR_REVISION,
        "contract_revision": CONTRACT_REVISION,
        "recovery_strategy": "FAIL_FORWARD_ONLY",
        "release_manifest_sha256": release_manifest_sha256,
        "repository_identity_sha256": repository_identity_sha256,
        "ledger_sha256": predecessor_ledger_sha256,
        "observed_at_unix_seconds": 100,
        "deployment": {
            "active_version_ids": ["worker-version-1"],
            "quiescent": true,
            "release_set_id": release_set_id,
            "single_version": true,
            "source_sha": source_sha,
            "traffic_percent": 100.0
        },
        "preconditions": {
            "request_digest_readers_writers_retired": true,
            "server_owned_payload_fingerprint_active": true
        }
    });

    let temp = TempDirectory::new()?;
    let predecessor_path = temp.path("predecessor-ledger.json");
    let post_path = temp.path("post-ledger.json");
    let unchanged_path = temp.path("unchanged-ledger.json");
    let release_path = temp.path("release.json");
    let evidence_path = temp.path("evidence.json");
    write_json(&predecessor_path, &predecessor_ledger)?;
    write_json(&post_path, &post_ledger)?;
    write_json(&unchanged_path, &predecessor_ledger)?;
    write_json(&release_path, &release_manifest)?;
    write_json(&evidence_path, &evidence)?;

    let output = contract_transition_verify(D1ContractTransitionVerificationRequest {
        root: &root,
        predecessor_ledger_json: &predecessor_path,
        ledger_json: &post_path,
        release_manifest: &release_path,
        evidence_json: &evidence_path,
        evaluated_at_unix_seconds: 101,
        expected_source_sha: &source_sha,
        expected_release_set_id: &release_set_id,
    })?;
    let verified: Value = serde_json::from_str(&output)?;
    assert_eq!(verified["command"], "d1 contract-transition verify");
    assert_eq!(verified["decision"], "SAFE");
    assert_eq!(verified["predecessor_revision"], PREDECESSOR_REVISION);
    assert_eq!(verified["remote_revision"], CONTRACT_REVISION);
    assert_eq!(verified["runtime_target_revision"], PREDECESSOR_REVISION);
    assert_eq!(verified["supported_schema_min"], PREDECESSOR_REVISION);
    assert_eq!(verified["supported_schema_max"], CONTRACT_REVISION);
    assert_eq!(verified["transition_migrations"], json!([CONTRACT_REVISION]));
    assert_eq!(verified["planned_migrations"], json!([]));
    assert_eq!(verified["recovery_strategy"], "FAIL_FORWARD_ONLY");
    assert_eq!(verified["allowed"], true);
    assert_eq!(verified["mutation_executed"], false);

    let error = contract_transition_verify(D1ContractTransitionVerificationRequest {
        root: &root,
        predecessor_ledger_json: &predecessor_path,
        ledger_json: &unchanged_path,
        release_manifest: &release_path,
        evidence_json: &evidence_path,
        evaluated_at_unix_seconds: 101,
        expected_source_sha: &source_sha,
        expected_release_set_id: &release_set_id,
    })
    .expect_err("unchanged 0031 ledger must not pass post-contract verification");
    assert!(error.to_string().contains("exactly one canonical 0031 -> 0032 transition"));
    Ok(())
}
