#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::operator_outcome::{
    D1OperatorOutcomeContext, D1OperatorOutcomeKind, build_operator_outcome,
    resolve_authorization_rejection_kind, serialize_operator_outcome,
};
use opsctl::d1::transaction::TargetIdentity;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Default)]
struct Args {
    kind: Option<D1OperatorOutcomeKind>,
    authorization_rejections: Vec<D1OperatorOutcomeKind>,
    source_sha: Option<String>,
    tree_sha: Option<String>,
    current_stage_issue: Option<u64>,
    transaction_id: Option<String>,
    target_json: Option<PathBuf>,
    owner_diagnostic_json: Option<PathBuf>,
    evidence_refs: BTreeMap<String, String>,
}

fn next_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<OsString, Box<dyn Error>> {
    iterator
        .next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn utf8_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    next_value(iterator, flag)?
        .into_string()
        .map_err(|_| format!("{flag} value must be valid UTF-8").into())
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), Box<dyn Error>> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be supplied only once").into());
    }
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Args, Box<dyn Error>>
where
    I: IntoIterator<Item = OsString>,
{
    let mut iterator = args.into_iter();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("d1-operator-outcome flags must be valid UTF-8")?;
        match flag {
            "--kind" => {
                let value = D1OperatorOutcomeKind::from_str(&utf8_value(&mut iterator, flag)?)?;
                set_once(&mut args.kind, value, flag)?;
            }
            "--authorization-rejection-kind" => {
                let value = D1OperatorOutcomeKind::from_str(&utf8_value(&mut iterator, flag)?)?;
                args.authorization_rejections.push(value);
            }
            "--source-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.source_sha, value, flag)?;
            }
            "--tree-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.tree_sha, value, flag)?;
            }
            "--current-stage-issue" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<u64>()?;
                set_once(&mut args.current_stage_issue, value, flag)?;
            }
            "--transaction-id" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.transaction_id, value, flag)?;
            }
            "--target-json" => set_once(
                &mut args.target_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--owner-diagnostic-json" => set_once(
                &mut args.owner_diagnostic_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--evidence-ref" => {
                let raw = utf8_value(&mut iterator, flag)?;
                let (name, reference) = raw
                    .split_once('=')
                    .ok_or("--evidence-ref requires NAME=REFERENCE")?;
                if name.trim().is_empty() || reference.trim().is_empty() {
                    return Err("--evidence-ref requires non-empty NAME=REFERENCE".into());
                }
                if args
                    .evidence_refs
                    .insert(name.to_owned(), reference.to_owned())
                    .is_some()
                {
                    return Err(format!("duplicate --evidence-ref name: {name}").into());
                }
            }
            other => {
                return Err(format!("unsupported d1-operator-outcome argument: {other}").into());
            }
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn read_strict(path: PathBuf, label: &str) -> Result<Value, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    parse_strict_json(&raw)
        .map_err(|error| format!("{label} is not strict bounded JSON: {error}").into())
}

fn diagnostic_kind(diagnostic: &Value) -> Result<Option<D1OperatorOutcomeKind>, Box<dyn Error>> {
    let reason = diagnostic
        .get("reason_code")
        .and_then(Value::as_str)
        .ok_or("owner diagnostic is missing string reason_code")?;
    Ok(D1OperatorOutcomeKind::from_str(reason).ok())
}

fn resolve_kind(
    explicit: Option<D1OperatorOutcomeKind>,
    authorization_rejections: &[D1OperatorOutcomeKind],
    owner_diagnostic: Option<&Value>,
) -> Result<D1OperatorOutcomeKind, Box<dyn Error>> {
    if !authorization_rejections.is_empty() {
        if explicit.is_some() || owner_diagnostic.is_some() {
            return Err("authorization rejection-set disposition cannot be combined with --kind or --owner-diagnostic-json".into());
        }
        return resolve_authorization_rejection_kind(authorization_rejections).map_err(Into::into);
    }
    match (explicit, owner_diagnostic) {
        (Some(kind), Some(diagnostic)) => {
            if let Some(diagnostic_kind) = diagnostic_kind(diagnostic)?
                && kind != diagnostic_kind
            {
                return Err(format!(
                    "--kind {} disagrees with operator-level owner diagnostic reason_code {}",
                    kind.as_str(),
                    diagnostic_kind.as_str()
                )
                .into());
            }
            Ok(kind)
        }
        (Some(kind), None) => Ok(kind),
        (None, Some(diagnostic)) => diagnostic_kind(diagnostic)?.ok_or_else(|| {
            "--kind is required when owner diagnostic reason_code is lower-level than the operator outcome vocabulary".into()
        }),
        (None, None) => Err("--kind is required when no owner diagnostic is supplied".into()),
    }
}

fn render(args: Args) -> Result<String, Box<dyn Error>> {
    let target = match args.target_json {
        Some(path) => Some(
            serde_json::from_value::<TargetIdentity>(read_strict(path, "operator target")?)
                .map_err(|error| {
                    format!("operator target does not match typed target contract: {error}")
                })?,
        ),
        None => None,
    };
    let owner_diagnostic = match args.owner_diagnostic_json {
        Some(path) => Some(read_strict(path, "owner diagnostic")?),
        None => None,
    };
    let kind = resolve_kind(
        args.kind,
        &args.authorization_rejections,
        owner_diagnostic.as_ref(),
    )?;
    let outcome = build_operator_outcome(
        kind,
        D1OperatorOutcomeContext {
            source_sha: required(args.source_sha, "--source-sha")?,
            tree_sha: required(args.tree_sha, "--tree-sha")?,
            current_stage_issue: required(args.current_stage_issue, "--current-stage-issue")?,
            transaction_id: args.transaction_id,
            target,
            owner_diagnostic,
            evidence_refs: args.evidence_refs,
        },
    )?;
    serialize_operator_outcome(&outcome).map_err(Into::into)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("{}", render(parse_args(env::args_os())?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn base_args(kind: D1OperatorOutcomeKind) -> Args {
        Args {
            kind: Some(kind),
            authorization_rejections: Vec::new(),
            source_sha: Some("aa".repeat(20)),
            tree_sha: Some("bb".repeat(20)),
            current_stage_issue: Some(642),
            transaction_id: Some("cc".repeat(32)),
            target_json: None,
            owner_diagnostic_json: None,
            evidence_refs: BTreeMap::new(),
        }
    }

    fn write_diagnostic(reason_code: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!("d1-operator-diagnostic-{unique}.json"));
        fs::write(
            &path,
            serde_json::to_string(&json!({
                "schema_version": 1,
                "reason_code": reason_code,
                "summary": "fixture",
                "remediation": "fixture remediation",
                "tool": {"name": "opsctl", "surface": "d1", "version": "fixture"}
            }))
            .expect("serialize diagnostic"),
        )
        .expect("write diagnostic");
        path
    }

    #[test]
    fn all_required_operator_states_render_canonical_typed_json() -> Result<(), Box<dyn Error>> {
        for kind in D1OperatorOutcomeKind::ALL {
            let output = render(base_args(kind))?;
            let value: Value = serde_json::from_str(&output)?;
            assert_eq!(value["contract"], "D1_OPERATOR_OUTCOME_V1");
            assert_eq!(value["outcome"], kind.as_str());
            assert_eq!(value["reason_code"], kind.as_str());
            assert!(
                value["remediation"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            );
            assert_eq!(value["operator_has_provider_credentials"], false);
        }
        Ok(())
    }

    #[test]
    fn rejection_set_disposition_is_order_independent() -> Result<(), Box<dyn Error>> {
        let mut left = base_args(D1OperatorOutcomeKind::CompletedVerified);
        left.kind = None;
        left.authorization_rejections = vec![
            D1OperatorOutcomeKind::StaleAuthorization,
            D1OperatorOutcomeKind::InvalidAuthorization,
        ];
        let mut right = base_args(D1OperatorOutcomeKind::CompletedVerified);
        right.kind = None;
        right.authorization_rejections = vec![
            D1OperatorOutcomeKind::InvalidAuthorization,
            D1OperatorOutcomeKind::StaleAuthorization,
        ];
        let left_value: Value = serde_json::from_str(&render(left)?)?;
        let right_value: Value = serde_json::from_str(&render(right)?)?;
        assert_eq!(left_value["outcome"], "INVALID_AUTHORIZATION");
        assert_eq!(right_value["outcome"], left_value["outcome"]);
        Ok(())
    }

    #[test]
    fn evidence_refs_are_stable_and_sorted() -> Result<(), Box<dyn Error>> {
        let mut args = base_args(D1OperatorOutcomeKind::CompletedVerified);
        args.evidence_refs
            .insert("receipt".to_owned(), "run:1:artifact:2".to_owned());
        args.evidence_refs
            .insert("post_state".to_owned(), "run:3:artifact:4".to_owned());
        let output = render(args)?;
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["evidence_refs"]["receipt"], "run:1:artifact:2");
        assert_eq!(value["evidence_refs"]["post_state"], "run:3:artifact:4");
        Ok(())
    }

    #[test]
    fn kind_is_derived_from_operator_level_owner_diagnostic() -> Result<(), Box<dyn Error>> {
        let path = write_diagnostic("STALE_OBSERVATION");
        let mut args = base_args(D1OperatorOutcomeKind::CompletedVerified);
        args.kind = None;
        args.owner_diagnostic_json = Some(path.clone());
        let output = render(args)?;
        fs::remove_file(path).ok();
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["outcome"], "STALE_OBSERVATION");
        assert_eq!(
            value["owner_diagnostic"]["reason_code"],
            "STALE_OBSERVATION"
        );
        Ok(())
    }

    #[test]
    fn explicit_kind_must_match_operator_level_owner_diagnostic() {
        let path = write_diagnostic("INVALID_AUTHORIZATION");
        let mut args = base_args(D1OperatorOutcomeKind::StaleAuthorization);
        args.owner_diagnostic_json = Some(path.clone());
        let result = render(args);
        fs::remove_file(path).ok();
        assert!(result.is_err());
    }

    #[test]
    fn lower_level_owner_diagnostic_is_preserved_under_explicit_operator_kind()
    -> Result<(), Box<dyn Error>> {
        let path = write_diagnostic("D1_PRECONDITION_BLOCKED");
        let mut args = base_args(D1OperatorOutcomeKind::PrepareBlocked);
        args.owner_diagnostic_json = Some(path.clone());
        let output = render(args)?;
        fs::remove_file(path).ok();
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["outcome"], "PREPARE_BLOCKED");
        assert_eq!(
            value["owner_diagnostic"]["reason_code"],
            "D1_PRECONDITION_BLOCKED"
        );
        Ok(())
    }
}
