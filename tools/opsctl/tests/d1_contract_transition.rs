use opsctl::canonical::{canonical_json, sha256_hex};
use opsctl::d1::execution_control::{
    ExecutionEventInput, ExecutionEventKind, ExecutionReceiptSeed, TargetFenceLeaseInput,
    acquire_target_fence, append_execution_event, initialize_execution_receipt,
};
use opsctl::d1::executor_admission::{ExecutorAdmissionExpectation, bind_executor_admission};
use opsctl::d1::transaction::{
    ProviderObservationInput, TransactionPhase, build_transaction_projection,
};
use opsctl::d1::{
    D1Action, D1RunRequest, repository_projection, run, verify_execution_post_state,
    verify_migration_ledger,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const F54: &str =
    "release-set-v3-sha256-f54bc16cc657f32d86a5647dc8173d8495a9dc1483283b96fa3908ccc6c9cb30";
const SOURCE: &str = "f086a72fabe632b2d63bd6e4f9e7679189ae8226";
const TARGET: &str =
    "release-set-v3-sha256-d1789e36c690480c61721d8d34bc1e27c97d58ffbdb15a61303369b4bf274ec5";
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn write(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
fn python(root: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("python")
        .arg("-X")
        .arg("utf8")
        .env_remove("CLOUDFLARE_API_TOKEN")
        .env_remove("CLOUDFLARE_OBSERVE_API_TOKEN")
        .env_remove("CLOUDFLARE_ACCOUNT_ID")
        .args(args)
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Python adapter failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}
fn strpath(p: &Path) -> Result<&str, Box<dyn Error>> {
    p.to_str().ok_or_else(|| "test path is not UTF8".into())
}
fn canonical_digest(v: &Value) -> Result<String, Box<dyn Error>> {
    Ok(sha256_hex(canonical_json(v)?.as_bytes()))
}
fn plan(root: &Path, dir: &Path) -> Result<Value, Box<dyn Error>> {
    let text = run(D1RunRequest {
        root,
        action: D1Action::Plan,
        component: "catalog",
        ledger_json: &dir.join("ledger.json"),
        release_manifest: Some(&dir.join("target.json")),
        current_manifest: Some(&dir.join("current.json")),
        known_good_manifest: Some(&dir.join("current.json")),
        preconditions_json: Some(&dir.join("preconditions.json")),
    })?;
    Ok(serde_json::from_str(&text)?)
}

#[test]
fn actual_deployed_context_traverses_separate_sealed_contract_and_current_expand_with_data_preserved()
-> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let scratch =
        Scratch(std::env::temp_dir().join(format!("d1-staged-{}-{nonce}", std::process::id())));
    let dir = &scratch.0;
    fs::create_dir_all(dir)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let repository: Value = serde_json::from_str(&repository_projection(&root)?)?;
    let catalog = repository["components"]
        .as_array()
        .ok_or("repository components are missing")?
        .iter()
        .find(|c| c["component_id"] == "catalog")
        .ok_or("Catalog component is missing")?;
    let history = catalog["executable_migration_sources"]
        .as_array()
        .ok_or("Catalog migration sources are missing")?
        .iter()
        .map(|m| {
            m["migration_file"]
                .as_str()
                .map(str::to_owned)
                .ok_or("migration name is missing")
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(history.len(), 37);
    let target = json!({"source":{"authority":"accepted-main","repository":"iamaman11/part-crm-emai-profile","ref":"refs/heads/main","event":"push","commit_sha":SOURCE},"schema_contract":catalog["release_schema_contract"]});
    let mut current = target.clone();
    current["source"]["commit_sha"] = json!("fd6c29cc403053f10d9716ff7ad616887ab9850e");
    current["schema_contract"]["target_schema_revision"] = json!(history[33]);
    current["schema_contract"]["supported_schema_min"] = json!(history[33]);
    current["schema_contract"]["supported_schema_max"] = json!(history[34]);
    current["schema_contract"]["migration_history_digest"] =
        json!("f8915315dfacefaba7d122f7e79634a8c49f75993bc49b4fef8522ac470a68ae");
    let original_files = catalog["executable_migration_sources"]
        .as_array().ok_or("missing source projection")?[..35]
        .iter().map(|m| -> Result<Value, Box<dyn Error>> {
            let name = m["migration_file"].as_str().ok_or("missing migration name")?;
            let path = if name == history[34] {
                root.join("migrations/d1-successor/0032_pas2_payload_fingerprint_contract.sql")
            } else {
                root.join(m["source_root"].as_str().ok_or("missing source root")?).join(name)
            };
            let bytes = fs::read(path)?;
            Ok(json!({"path":format!("migrations/d1/{name}"),"sha256":sha256_hex(&bytes),"size":bytes.len()}))
        }).collect::<Result<Vec<_>, _>>()?;
    current["migrations"] = json!({"files":original_files});
    write(&dir.join("target.json"), &target)?;
    write(&dir.join("current.json"), &current)?;
    write(&dir.join("repository.json"), &repository)?;
    let current_bytes = fs::read(dir.join("current.json"))?;
    let native = PathBuf::from(env!("CARGO_BIN_EXE_opsctl"));
    let context = json!({"schema_contract":current["schema_contract"],"worker_name":"worker-1","deployed_manifest":current,
        "deployed_release_source_sha":current["source"]["commit_sha"],"deployed_release_set_id":F54,
        "first_deployment_observed_at_unix_seconds":now-40,"deployment_observed_at_unix_seconds":now,
        "deployment_identity_after":{"release_set_id":F54},
        "deployment_snapshot_before":{"id":"11111111-1111-4111-8111-111111111111","versions":[{"version_id":"22222222-2222-4222-8222-222222222222","percentage":100.0}]},
        "deployment_snapshot":{"id":"11111111-1111-4111-8111-111111111111","versions":[{"version_id":"22222222-2222-4222-8222-222222222222","percentage":100.0}]}});
    write(&dir.join("runtime-context.json"), &context)?;
    let mut prior_authorization: Option<Value> = None;
    for count in [34, 35] {
        let ledger = json!([{"success":true,"results":history[..count].iter().enumerate().map(|(i,n)|json!({"id":i+1,"name":n})).collect::<Vec<_>>()}]);
        write(&dir.join("ledger.json"), &ledger)?;
        let observed = json!({"schema_version":1,"target":{"environment":"staging","account_id":"account-1","database_name":"isolated-local-test","database_id":"database-1"},
            "observed_at_unix_seconds":now,"observation_source":"fixture:exact-provider","remote_ledger_sha256":canonical_digest(&json!({"remote_migrations":history[..count]}))?,
            "remote_migrations":history[..count],"wrangler_pending_migrations":history[count..],"deployment_identity":F54,"time_travel_bookmark_capable":true});
        write(
            &dir.join("provider-observation.json"),
            &json!({"provider_observation_input":observed}),
        )?;
        python(
            &root,
            &[
                "scripts/d1-prepare.py",
                "transition-evidence",
                "--runtime-context",
                strpath(&dir.join("runtime-context.json"))?,
                "--observation",
                strpath(&dir.join("provider-observation.json"))?,
                "--release-manifest",
                strpath(&dir.join("target.json"))?,
                "--repository-json",
                strpath(&dir.join("repository.json"))?,
                "--release-id",
                TARGET,
                "--component",
                "catalog",
                "--output",
                strpath(&dir.join("preconditions.json"))?,
            ],
        )?;
        let native_plan = plan(&root, dir)?;
        assert_eq!(native_plan["planned_migrations"], json!([history[count]]));
        assert_eq!(native_plan["target_revision"], history[count]);
        assert_eq!(native_plan["desired_target_revision"], history[36]);
        write(&dir.join("native-stage.json"), &native_plan)?;
        write(
            &dir.join("deployment-snapshot.json"),
            &context["deployment_snapshot"],
        )?;
        write(
            &dir.join("deployment-identity.json"),
            &json!({"release_set_id":F54}),
        )?;
        python(
            &root,
            &[
                "scripts/d1-prepare.py",
                "verify-deployment",
                "--plan",
                strpath(&dir.join("native-stage.json"))?,
                "--deployment",
                strpath(&dir.join("deployment-snapshot.json"))?,
                "--identity",
                strpath(&dir.join("deployment-identity.json"))?,
            ],
        )?;
        let mut changed_deployment = context["deployment_snapshot"].clone();
        changed_deployment["id"] = json!("33333333-3333-4333-8333-333333333333");
        write(&dir.join("deployment-snapshot.json"), &changed_deployment)?;
        let rejected = Command::new("python")
            .arg(root.join("scripts/d1-prepare.py"))
            .args(["verify-deployment", "--plan"])
            .arg(dir.join("native-stage.json"))
            .arg("--deployment")
            .arg(dir.join("deployment-snapshot.json"))
            .arg("--identity")
            .arg(dir.join("deployment-identity.json"))
            .env_remove("CLOUDFLARE_API_TOKEN")
            .env_remove("CLOUDFLARE_OBSERVE_API_TOKEN")
            .env_remove("CLOUDFLARE_ACCOUNT_ID")
            .output()?;
        assert_eq!(rejected.status.code(), Some(2));
        let comparison: Value = serde_json::from_slice(&rejected.stdout)?;
        assert_eq!(comparison["matched"], false);
        assert_eq!(comparison["policy_verdict"], Value::Null);
        let phase = if count == 34 {
            TransactionPhase::Contract
        } else {
            TransactionPhase::Ordinary
        };
        assert_eq!(native_plan["transaction_policy"]["phase"], json!(phase));
        assert_eq!(
            native_plan["transaction_policy"]["recovery_strategy"],
            if count == 34 {
                "FAIL_FORWARD_ONLY"
            } else {
                "ROLL_FORWARD"
            }
        );
        assert_eq!(native_plan["rollback_context_complete"], false);
        python(
            &root,
            &[
                "scripts/d1-prepare.py",
                "prepare",
                "--root",
                strpath(&root)?,
                "--opsctl",
                strpath(&native)?,
                "--component",
                "catalog",
                "--ledger-json",
                strpath(&dir.join("ledger.json"))?,
                "--release-manifest",
                strpath(&dir.join("target.json"))?,
                "--current-manifest",
                strpath(&dir.join("current.json"))?,
                "--known-good-manifest",
                strpath(&dir.join("current.json"))?,
                "--preconditions-json",
                strpath(&dir.join("preconditions.json"))?,
                "--output",
                strpath(&dir.join("prepare.json"))?,
            ],
        )?;
        let prepared: Value = serde_json::from_slice(&fs::read(dir.join("prepare.json"))?)?;
        assert_eq!(prepared["status"], "PREPARE_READY");
        assert_eq!(
            prepared["compatibility"]["runtime_compatibility"]["compatible"],
            false
        );
        python(
            &root,
            &[
                "scripts/d1-prepare.py",
                "operator-input",
                "--root",
                strpath(&root)?,
                "--opsctl",
                strpath(&native)?,
                "--preflight-dir",
                strpath(dir)?,
                "--source-sha",
                SOURCE,
                "--tree-sha",
                &"a".repeat(40),
                "--release-id",
                TARGET,
                "--release-manifest",
                strpath(&dir.join("target.json"))?,
                "--evidence-ref",
                "fixture:current-observation",
                "--output",
                strpath(&dir.join("inputs.json"))?,
            ],
        )?;
        let inputs: Value = serde_json::from_slice(&fs::read(dir.join("inputs.json"))?)?;
        assert_eq!(inputs["inputs"]["repository_projection_json"], "{}");
        let dispatch = json!({"ref":"main","inputs":inputs["inputs"]});
        assert!(serde_json::to_vec(&dispatch)?.len() <= 65535);
        let generated = Command::new(&native)
            .arg("--root")
            .arg(&root)
            .args(["d1", "repository"])
            .output()?;
        assert!(
            generated.status.success(),
            "native Prepare repository derivation failed"
        );
        let prepared_repository: Value = serde_json::from_slice(&generated.stdout)?;
        assert_eq!(prepared_repository, repository);
        let decode = |k: &str| -> Result<Value, Box<dyn Error>> {
            Ok(serde_json::from_str(
                inputs["inputs"][k]
                    .as_str()
                    .ok_or("operator input field is missing")?,
            )?)
        };
        let tx_input = decode("transaction_input_json")?;
        let tx = build_transaction_projection(
            &prepared,
            &decode("provider_observation_json")?,
            &prepared_repository,
            &tx_input,
            &fs::read(dir.join("target.json"))?,
        )?;
        assert_eq!(tx.transaction_plan.phase, phase);
        assert_eq!(tx.transaction_plan.schema_target, history[count]);
        let auth = json!({"schema_version":1,"transaction_id":tx.transaction_id,"target":tx.transaction_plan.target,"phase":phase,
            "authorized_provider_effects":tx.transaction_plan.allowed_provider_effects,"issued_at_unix_seconds":now,"expires_at_unix_seconds":now+800,
            "observation_fresh_until_unix_seconds":now+900,"authorization_reference":"fixture:exact-human-authorization"});
        let expect = ExecutorAdmissionExpectation {
            transaction_id: tx.transaction_id.clone(),
            source_sha: SOURCE.to_owned(),
            tree_sha: "a".repeat(40),
            component: "catalog".into(),
            target: tx.transaction_plan.target.clone(),
            phase,
        };
        let admission = bind_executor_admission(&tx, &auth, now + 1, &expect)?;
        if let Some(prior) = &prior_authorization {
            assert!(bind_executor_admission(&tx, prior, now + 1, &expect).is_err());
        }
        prior_authorization = Some(auth.clone());
        let mut wrong_stop = prepared.clone();
        wrong_stop["plan"]["target_revision"] = json!(history[if count == 34 { 35 } else { 34 }]);
        let mut wrong_stop_input = tx_input.clone();
        wrong_stop_input["expected_post_state"]["revision"] =
            wrong_stop["plan"]["target_revision"].clone();
        wrong_stop_input["expected_post_state"]["stage_plan"] = wrong_stop["plan"].clone();
        assert!(
            build_transaction_projection(
                &wrong_stop,
                &decode("provider_observation_json")?,
                &repository,
                &wrong_stop_input,
                &fs::read(dir.join("target.json"))?
            )
            .is_err()
        );
        let mut substituted_phase = tx_input.clone();
        substituted_phase["phase"] = json!(if phase == TransactionPhase::Contract {
            "ORDINARY"
        } else {
            "CONTRACT"
        });
        assert!(
            build_transaction_projection(
                &prepared,
                &decode("provider_observation_json")?,
                &repository,
                &substituted_phase,
                &fs::read(dir.join("target.json"))?
            )
            .is_err()
        );
        let mut downgraded = prepared.clone();
        downgraded["plan"]["transaction_policy"] = json!({"schema_version":1,"transaction_kind":"D1_MIGRATION","phase":"ORDINARY","observation_freshness_max_age_seconds":3600,"recovery_strategy":"NOOP_RETRY"});
        let mut ungoverned = tx_input.clone();
        ungoverned["phase"] = json!("ORDINARY");
        ungoverned["recovery_strategy"] = json!("NOOP_RETRY");
        ungoverned["freshness_max_age_seconds"] = json!(3600);
        assert!(
            build_transaction_projection(
                &downgraded,
                &decode("provider_observation_json")?,
                &repository,
                &ungoverned,
                &fs::read(dir.join("target.json"))?
            )
            .is_err()
        );

        let phase_dir = dir.join(count.to_string());
        fs::create_dir(&phase_dir)?;
        write(
            &phase_dir.join("plan.json"),
            &serde_json::to_value(&admission.execution_plan)?,
        )?;
        python(
            &root,
            &[
                "scripts/d1-executor-plan.py",
                "materialize",
                "--repo-root",
                strpath(&root)?,
                "--repository",
                strpath(&dir.join("repository.json"))?,
                "--plan",
                strpath(&phase_dir.join("plan.json"))?,
                "--ledger",
                strpath(&dir.join("ledger.json"))?,
                "--component",
                "catalog",
                "--output-dir",
                strpath(&phase_dir.join("bounded"))?,
                "--expected-pending",
                strpath(&phase_dir.join("expected.json"))?,
                "--normalized-ledger",
                strpath(&phase_dir.join("ledger.json"))?,
                "--require-plan-digests",
            ],
        )?;
        let lease = acquire_target_fence(TargetFenceLeaseInput {
            schema_version: 1,
            target: expect.target.clone(),
            phase,
            operation_identity: tx.transaction_id.clone(),
            transaction_id: Some(tx.transaction_id.clone()),
            authorization_digest: Some(admission.authorization_digest.clone()),
            source_sha: SOURCE.into(),
            executor_run_id: count as u64,
            fence_epoch: count as u64,
            run_attempt: 1,
            acquired_at_unix_seconds: now + 1,
        })?;
        let mut receipt = initialize_execution_receipt(
            ExecutionReceiptSeed {
                schema_version: 1,
                target: expect.target.clone(),
                phase,
                operation_identity: tx.transaction_id.clone(),
                transaction_id: Some(tx.transaction_id.clone()),
                authorization_digest: Some(admission.authorization_digest.clone()),
                source_sha: SOURCE.into(),
                recovery_strategy: tx.transaction_plan.recovery_strategy,
                fence: lease,
            },
            now + 1,
            now + 1,
        )?;
        for kind in [
            ExecutionEventKind::PrewriteFencePass,
            ExecutionEventKind::MutationStarted,
        ] {
            receipt = append_execution_event(
                &receipt,
                ExecutionEventInput {
                    kind,
                    occurred_at_unix_seconds: now + 2,
                    migration_id: None,
                },
            )?;
        }
        let failed = append_execution_event(
            &receipt,
            ExecutionEventInput {
                kind: ExecutionEventKind::RecoveryRequired,
                occurred_at_unix_seconds: now + 2,
                migration_id: None,
            },
        )?;
        let unchanged: ProviderObservationInput = serde_json::from_value(observed.clone())?;
        let mut fresh_unchanged = unchanged.clone();
        fresh_unchanged.observed_at_unix_seconds = now + 3;
        assert!(
            !verify_execution_post_state(&tx, &failed, &fresh_unchanged, now + 3)?
                .expected_post_state_reached
        );
        // The actual SQL is applied only to a disposable SQLite database; provider credentials are absent.
        python(
            &root,
            &[
                "-c",
                SQL_PROOF,
                strpath(&root)?,
                strpath(dir)?,
                strpath(&phase_dir.join("bounded"))?,
                &count.to_string(),
            ],
        )?;
        receipt = append_execution_event(
            &receipt,
            ExecutionEventInput {
                kind: ExecutionEventKind::MigrationApplied,
                occurred_at_unix_seconds: now + 3,
                migration_id: Some(history[count].clone()),
            },
        )?;
        let actual_ledger: Vec<String> =
            serde_json::from_slice(&fs::read(dir.join("observed-ledger.json"))?)?;
        assert_eq!(actual_ledger, history[..count + 1]);
        verify_migration_ledger(&tx, &actual_ledger)?;
        assert!(verify_migration_ledger(&tx, &history[..count]).is_err());
        for kind in [
            ExecutionEventKind::PostObserved,
            ExecutionEventKind::Verified,
            ExecutionEventKind::Completed,
        ] {
            receipt = append_execution_event(
                &receipt,
                ExecutionEventInput {
                    kind,
                    occurred_at_unix_seconds: now + 4,
                    migration_id: None,
                },
            )?;
        }
        let mut post: ProviderObservationInput = serde_json::from_value(observed)?;
        post.observed_at_unix_seconds = now + 5;
        post.remote_migrations = actual_ledger;
        post.wrangler_pending_migrations = history[count + 1..].to_vec();
        post.remote_ledger_sha256 =
            canonical_digest(&json!({"remote_migrations":post.remote_migrations}))?;
        assert!(
            verify_execution_post_state(&tx, &receipt, &post, now + 5)?.expected_post_state_reached
        );
        // Replayed/altered authority, phase, recovery, source and plan must not admit a second effect.
        let mut bad_auth = auth.clone();
        bad_auth["transaction_id"] = json!("0".repeat(64));
        assert!(bind_executor_admission(&tx, &bad_auth, now + 1, &expect).is_err());
        assert!(bind_executor_admission(&tx, &auth, now + 901, &expect).is_err());
        for field in ["phase", "recovery_strategy", "source_sha"] {
            let mut altered = tx_input.clone();
            altered[field] = json!("FORGED");
            assert!(
                build_transaction_projection(
                    &prepared,
                    &decode("provider_observation_json")?,
                    &repository,
                    &altered,
                    &fs::read(dir.join("target.json"))?
                )
                .is_err()
            );
        }
        let original_evidence: Value =
            serde_json::from_slice(&fs::read(dir.join("preconditions.json"))?)?;
        for damage in ["missing_files", "future_digest", "compiled_prefix"] {
            let mut changed = current.clone();
            match damage {
                "missing_files" => {
                    changed
                        .as_object_mut()
                        .ok_or("current object")?
                        .remove("migrations");
                }
                "future_digest" => {
                    changed["migrations"]["files"][34]["sha256"] = json!("a".repeat(64))
                }
                _ => {
                    changed["migrations"]["files"][33]["sha256"] = json!("a".repeat(64));
                    let identity = changed["migrations"]["files"]
                        .as_array()
                        .ok_or("files")?
                        .iter()
                        .enumerate()
                        .map(|(index, m)| json!({"name":history[index],"sha256":m["sha256"]}))
                        .collect::<Vec<_>>();
                    changed["schema_contract"]["migration_history_digest"] =
                        json!(canonical_digest(&json!(identity))?);
                }
            }
            write(&dir.join("current.json"), &changed)?;
            let error = plan(&root, dir)
                .err()
                .ok_or("damaged runtime evidence was accepted")?;
            let expected = match damage {
                "missing_files" => "original runtime migration file evidence is missing",
                "future_digest" => {
                    "original runtime migration history digest does not match its files"
                }
                _ => "original runtime compiled migration prefix has changed",
            };
            assert!(
                error.to_string().contains(expected),
                "wrong owner diagnostic for {damage}: {error}"
            );
        }
        write(&dir.join("current.json"), &current)?;
        for path in [
            "current_manifest_sha256",
            "target_manifest_sha256",
            "repository_identity_sha256",
            "ledger_sha256",
            "target_source_sha",
        ] {
            let mut changed = original_evidence.clone();
            changed[path] = json!("0".repeat(64));
            write(&dir.join("preconditions.json"), &changed)?;
            assert!(plan(&root, dir).is_err(), "forged {path}");
        }
        let mut mixed = original_evidence.clone();
        mixed["deployment"]["versions"] =
            json!([{"version_id":"one","percentage":50},{"version_id":"two","percentage":50}]);
        write(&dir.join("preconditions.json"), &mixed)?;
        assert!(plan(&root, dir).is_err());
        let mut stale = original_evidence.clone();
        stale["evaluated_at_unix_seconds"] = json!(now + 901);
        write(&dir.join("preconditions.json"), &stale)?;
        assert!(plan(&root, dir).is_err());
        write(&dir.join("preconditions.json"), &original_evidence)?;
    }
    assert_eq!(fs::read(dir.join("current.json"))?, current_bytes);
    write(
        &dir.join("ledger.json"),
        &json!({"rows":history.iter().enumerate().map(|(i,n)|json!({"id":i+1,"name":n})).collect::<Vec<_>>()}),
    )?;
    let compatible: Value = serde_json::from_str(&run(D1RunRequest {
        root: &root,
        action: D1Action::Compatibility,
        component: "catalog",
        ledger_json: &dir.join("ledger.json"),
        release_manifest: Some(&dir.join("target.json")),
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
    })?)?;
    assert_eq!(compatible["runtime_compatibility"]["compatible"], true);
    Ok(())
}

const SQL_PROOF: &str = r#"
import sys,json,sqlite3,pathlib,runpy
root,scratch,bounded,count=pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),pathlib.Path(sys.argv[3]),int(sys.argv[4])
dbfile=scratch/'catalog.sqlite';initial=not dbfile.exists();db=sqlite3.connect(dbfile);db.execute('PRAGMA foreign_keys=ON')
repository=json.loads((scratch/'repository.json').read_text())
catalog=next(c for c in repository['components'] if c['component_id']=='catalog')
sources=catalog['executable_migration_sources']
def snapshot():
 return {table:db.execute('SELECT * FROM '+table+' ORDER BY 1,2').fetchall() for table in ['tenants','memberships','clients','browser_profiles']}
if initial:
 db.execute('CREATE TABLE d1_migrations(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE)')
 for i,m in enumerate(sources[:34]):
  db.executescript((root/m['source_root']/m['migration_file']).read_text())
  db.execute('INSERT INTO d1_migrations(id,name) VALUES(?,?)',(i+1,m['migration_file']))
 db.commit()
 runpy.run_path(str(root/'scripts/test-d1-schema.py'))['seed_catalog'](db)
 (scratch/'before.json').write_text(json.dumps(snapshot(),sort_keys=True))
db.execute('PRAGMA case_sensitive_like=OFF')
if count==34:
 try:
  db.executescript((root/'migrations/d1-successor/0032_pas2_payload_fingerprint_contract.sql').read_text())
  raise AssertionError('original CONTRACT unexpectedly accepted the D1 LIKE registration')
 except sqlite3.OperationalError as error:
  assert 'non-deterministic functions prohibited in partial index WHERE clauses' in str(error),str(error)
 # A failed ALTER invalidates SQLite's cached schema. A fresh connection is
 # required to inspect rollback before reproducing the connection setup again.
 db.close();db=sqlite3.connect(dbfile);db.execute('PRAGMA foreign_keys=ON')
 # The first ALTER failed atomically, and the corrected SQL must retain all rows.
 assert db.execute("SELECT count(*) FROM sqlite_master WHERE name='idempotency_records_pas2_contract_legacy'").fetchone()==(0,)
 db.execute('PRAGMA case_sensitive_like=OFF')
for m in sources[count:count+1]:
 db.executescript((bounded/m['migration_file']).read_text())
 db.execute('INSERT INTO d1_migrations(id,name) VALUES(?,?)',(count+1,m['migration_file']))
(scratch/'observed-ledger.json').write_text(json.dumps([r[0] for r in db.execute('SELECT name FROM d1_migrations ORDER BY id')]))
assert json.loads((scratch/'before.json').read_text())==json.loads(json.dumps(snapshot()))
assert db.execute('PRAGMA foreign_key_check').fetchall()==[]
assert db.execute('PRAGMA quick_check').fetchone()==('ok',)
assert 'request_digest' not in [r[1] for r in db.execute('PRAGMA table_info(idempotency_records)')]
if count==35:assert 'initial_device_id' in [r[1] for r in db.execute('PRAGMA table_info(profile_generation_register_commands)')]
if count==34:
 # Retain both ACTIVE uniqueness boundaries and the original LIKE wildcard /
 # ASCII case-folding behavior; inactive rows remain independent.
 examples=['mtls_cert_sha256:a','MTLS_CERT_SHA256:a','mtlsXcertYsha256:a','mtls_cert_sha256:',
           'p256_spki_der:a','P256_SPKI_DER:a','p256XspkiYder:a','other:a','p256_spki_der','']
 for name,old_pattern in [('device_actor_bindings_one_active_certificate','mtls_cert_sha256:%'),
                          ('device_actor_bindings_one_active_p256_key','p256_spki_der:%')]:
  sql=db.execute('SELECT sql FROM sqlite_master WHERE name=?',(name,)).fetchone()[0]
  predicate=sql.split('WHERE ',1)[1]
  guard=sqlite3.connect(':memory:')
  guard.execute('CREATE TABLE probe(status TEXT,evidence_reference TEXT)')
  guard.execute('CREATE UNIQUE INDEX probe_guard ON probe(evidence_reference) WHERE '+predicate)
  for text in examples:
   expected=guard.execute('SELECT ? LIKE ?',(text,old_pattern)).fetchone()[0]
   actual=guard.execute('SELECT '+predicate+' FROM (SELECT ? AS status,? AS evidence_reference)',('ACTIVE',text)).fetchone()[0]
   assert expected==actual,(name,text,expected,actual)
   guard.execute('DELETE FROM probe')
   guard.execute('INSERT INTO probe VALUES(?,?)',('ACTIVE',text))
   try:
    guard.execute('INSERT INTO probe VALUES(?,?)',('ACTIVE',text))
    assert not expected,(name,text,'ACTIVE duplicate admitted')
   except sqlite3.IntegrityError:
    assert expected,(name,text,'unrelated duplicate rejected')
   guard.execute('INSERT INTO probe VALUES(?,?)',('REVOKED',text))
  guard.close()
db.commit();db.close()
"#;
