#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one cutover marker in {path}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


compatibility = ROOT / "tools/opsctl/src/release/compatibility.rs"
replace_once(
    compatibility,
    """    let windows_required = authority\n        .activation_units\n        .values()\n        .any(|unit| unit.requires_windows_profile_bridge && effective.is_enabled(&unit.id));\n""",
    """    let windows_delivery_present = authority\n        .activation_units\n        .values()\n        .any(|unit| unit.requires_windows_profile_bridge && effective.is_enabled(&unit.id));\n    let windows_required = environment == \"production\" && windows_delivery_present;\n""",
)

verify = ROOT / "tools/opsctl/src/promotion/verify.rs"
replace_once(
    verify,
    """        let required = dimension_required(name, &closure);\n""",
    """        let required = dimension_required(name, &closure, request.environment);\n""",
)
replace_once(
    verify,
    """fn dimension_required(name: &str, closure: &crate::promotion::authority::DeploymentClosure) -> bool {\n    match name {\n        \"resolver_d1\" | \"resolver_protocol\" => closure.required_resources.contains(\"resolver_d1\"),\n        \"windows_profile_bridge\" => closure.required_components.contains(\"profile_bridge\")\n            || closure.required_resources.contains(\"windows_profile_bridge\"),\n        _ => true,\n    }\n}\n""",
    """fn dimension_required(\n    name: &str,\n    closure: &crate::promotion::authority::DeploymentClosure,\n    environment: &str,\n) -> bool {\n    match name {\n        \"resolver_d1\" | \"resolver_protocol\" => closure.required_resources.contains(\"resolver_d1\"),\n        \"windows_profile_bridge\" => environment == \"production\"\n            && (closure.required_components.contains(\"profile_bridge\")\n                || closure.required_resources.contains(\"windows_profile_bridge\")),\n        _ => true,\n    }\n}\n""",
)

cli = ROOT / "tools/opsctl/src/cli.rs"
replace_once(cli, "use crate::release;\n", "use crate::promotion;\nuse crate::release;\n")
replace_once(
    cli,
    "    opsctl [--root PATH] release <ACTION> --release-set PATH [RELEASE OPTIONS]\\n\\nCOMMANDS:",
    "    opsctl [--root PATH] release <ACTION> --release-set PATH [RELEASE OPTIONS]\\n    opsctl [--root PATH] promotion <ACTION> --release-set PATH --profile ID --environment ENV --snapshot PATH --evidence-json PATH [PROMOTION OPTIONS]\\n\\nCOMMANDS:",
)
replace_once(
    cli,
    "    release compatibility      Evaluate Release Set + Capability Profile compatibility\\n\\nD1 OPTIONS:",
    "    release compatibility      Evaluate Release Set + Capability Profile compatibility\\n    promotion plan             Build a deterministic no-mutation transition plan\\n    promotion preflight        Fail-closed gate before provider credential exposure\\n    promotion verify           Verify observed state after provider mutation\\n\\nD1 OPTIONS:",
)
replace_once(
    cli,
    "    --current-release-set PATH  Optional current Release Set for rollback context\\n\\nGLOBAL OPTIONS:",
    "    --current-release-set PATH  Optional current Release Set for rollback context\\n\\nPROMOTION OPTIONS:\\n    --release-set PATH          Target immutable Release Set\\n    --profile ID                Target Capability Profile\\n    --environment ID            rehearsal, staging, or production\\n    --snapshot PATH             Saved metadata-only DeploymentSnapshot\\n    --evidence-json PATH        Saved release compatibility evidence\\n    --current-release-set PATH  Optional current Release Set\\n    --known-good-release-set PATH  Rollback candidate for preflight\\n    --expected-current ID       Stale-plan fence (use NONE for fresh state)\\n\\nGLOBAL OPTIONS:",
)
replace_once(
    cli,
    """        current_release_set: Option<PathBuf>,\n    },\n}\n""",
    """        current_release_set: Option<PathBuf>,\n    },\n    Promotion {\n        root: Option<PathBuf>,\n        action: promotion::PromotionAction,\n        release_set: PathBuf,\n        profile_id: String,\n        environment: String,\n        snapshot: PathBuf,\n        evidence_json: PathBuf,\n        current_release_set: Option<PathBuf>,\n        known_good_release_set: Option<PathBuf>,\n        expected_current_release_set_id: Option<String>,\n    },\n}\n""",
)
replace_once(
    cli,
    '        "release" => parse_release_invocation(root, iterator),\n',
    '        "release" => parse_release_invocation(root, iterator),\n        "promotion" => parse_promotion_invocation(root, iterator),\n',
)

promotion_parser = r'''
fn parse_promotion_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("promotion", "missing promotion action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("promotion", "promotion action must be valid UTF-8"))?;
    let action = match action_text {
        "plan" => promotion::PromotionAction::Plan,
        "preflight" => promotion::PromotionAction::Preflight,
        "verify" => promotion::PromotionAction::Verify,
        other => {
            return Err(OpsctlError::new(
                "promotion",
                format!("unsupported promotion action: {other}"),
            ));
        }
    };

    let mut release_set: Option<PathBuf> = None;
    let mut profile_id: Option<String> = None;
    let mut environment: Option<String> = None;
    let mut snapshot: Option<PathBuf> = None;
    let mut evidence_json: Option<PathBuf> = None;
    let mut current_release_set: Option<PathBuf> = None;
    let mut known_good_release_set: Option<PathBuf> = None;
    let mut expected_current_release_set_id: Option<String> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("promotion", "promotion flags must be valid UTF-8"))?;
        match flag {
            "--release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--release-set requires a value")
                })?;
                set_once(&mut release_set, PathBuf::from(value), "--release-set")?;
            }
            "--profile" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--profile requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("promotion", "profile must be valid UTF-8"))?;
                set_once(&mut profile_id, value, "--profile")?;
            }
            "--environment" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--environment requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("promotion", "environment must be valid UTF-8"))?;
                set_once(&mut environment, value, "--environment")?;
            }
            "--snapshot" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--snapshot requires a value")
                })?;
                set_once(&mut snapshot, PathBuf::from(value), "--snapshot")?;
            }
            "--evidence-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--evidence-json requires a value")
                })?;
                set_once(&mut evidence_json, PathBuf::from(value), "--evidence-json")?;
            }
            "--current-release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--current-release-set requires a value")
                })?;
                set_once(
                    &mut current_release_set,
                    PathBuf::from(value),
                    "--current-release-set",
                )?;
            }
            "--known-good-release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--known-good-release-set requires a value")
                })?;
                set_once(
                    &mut known_good_release_set,
                    PathBuf::from(value),
                    "--known-good-release-set",
                )?;
            }
            "--expected-current" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--expected-current requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("promotion", "expected current must be valid UTF-8"))?;
                set_once(
                    &mut expected_current_release_set_id,
                    value,
                    "--expected-current",
                )?;
            }
            other => {
                return Err(OpsctlError::new(
                    "promotion",
                    format!("unsupported promotion argument: {other}"),
                ));
            }
        }
    }

    let release_set = release_set
        .ok_or_else(|| OpsctlError::new("promotion", "--release-set is required"))?;
    let profile_id = profile_id
        .ok_or_else(|| OpsctlError::new("promotion", "--profile is required"))?;
    let environment = environment
        .ok_or_else(|| OpsctlError::new("promotion", "--environment is required"))?;
    if !matches!(environment.as_str(), "rehearsal" | "staging" | "production") {
        return Err(OpsctlError::new(
            "promotion",
            "--environment must be rehearsal, staging, or production",
        ));
    }
    let snapshot = snapshot
        .ok_or_else(|| OpsctlError::new("promotion", "--snapshot is required"))?;
    let evidence_json = evidence_json
        .ok_or_else(|| OpsctlError::new("promotion", "--evidence-json is required"))?;

    match action {
        promotion::PromotionAction::Plan => {
            reject_if_present_promotion(&known_good_release_set, "--known-good-release-set", action)?;
        }
        promotion::PromotionAction::Preflight => {}
        promotion::PromotionAction::Verify => {
            reject_if_present_promotion(&current_release_set, "--current-release-set", action)?;
            reject_if_present_promotion(&known_good_release_set, "--known-good-release-set", action)?;
            reject_if_present_promotion(
                &expected_current_release_set_id,
                "--expected-current",
                action,
            )?;
        }
    }

    Ok(Invocation::Promotion {
        root,
        action,
        release_set,
        profile_id,
        environment,
        snapshot,
        evidence_json,
        current_release_set,
        known_good_release_set,
        expected_current_release_set_id,
    })
}

fn reject_if_present_promotion<T>(
    value: &Option<T>,
    flag: &str,
    action: promotion::PromotionAction,
) -> Result<(), OpsctlError> {
    if value.is_some() {
        Err(OpsctlError::new(
            "promotion",
            format!("{flag} is not valid for promotion {}", action.name()),
        ))
    } else {
        Ok(())
    }
}

'''
cli_text = cli.read_text(encoding="utf-8")
marker = "fn parse_command(value: &str) -> Result<ReadCommand, OpsctlError> {\n"
if cli_text.count(marker) != 1:
    raise SystemExit("parse_command insertion marker drifted")
cli.write_text(cli_text.replace(marker, promotion_parser + marker, 1), encoding="utf-8")

lib = ROOT / "tools/opsctl/src/lib.rs"
lib_text = lib.read_text(encoding="utf-8")
marker = "\n    }\n}\n\n#[cfg(test)]\n"
index = lib_text.rfind(marker)
if index < 0:
    raise SystemExit("opsctl execute match tail marker drifted")
promotion_arm = r'''
        Invocation::Promotion {
            root,
            action,
            release_set,
            profile_id,
            environment,
            snapshot,
            evidence_json,
            current_release_set,
            known_good_release_set,
            expected_current_release_set_id,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "promotion")?;
            promotion::commands::run(promotion::commands::PromotionRunRequest {
                root: &repo_root,
                action,
                release_set: &release_set,
                profile_id: &profile_id,
                environment: &environment,
                snapshot: &snapshot,
                evidence_json: &evidence_json,
                current_release_set: current_release_set.as_deref(),
                known_good_release_set: known_good_release_set.as_deref(),
                expected_current_release_set_id: expected_current_release_set_id.as_deref(),
            })
            .map_err(|error| OpsctlError::new("promotion", error.to_string()))
        }
'''
lib.write_text(lib_text[:index] + "\n" + promotion_arm + lib_text[index:], encoding="utf-8")
