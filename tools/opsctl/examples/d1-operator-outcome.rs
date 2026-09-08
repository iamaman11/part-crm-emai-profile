#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::operator_outcome::{
    D1OperatorOutcomeContext, D1OperatorOutcomeKind, build_operator_outcome,
    serialize_operator_outcome,
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
            other => return Err(format!("unsupported d1-operator-outcome argument: {other}").into()),
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn read_strict(path: PathBuf, label: &str) -> Result<Value, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    parse_strict_json(&raw).map_err(|error| format!("{label} is not strict bounded JSON: {error}").into())
}

fn render(args: Args) -> Result<String, Box<dyn Error>> {
    let target = match args.target_json {
        Some(path) => Some(
            serde_json::from_value::<TargetIdentity>(read_strict(path, "operator target")?)
                .map_err(|error| format!("operator target does not match typed target contract: {error}"))?,
        ),
        None => None,
    };
    let owner_diagnostic = match args.owner_diagnostic_json {
        Some(path) => Some(read_strict(path, "owner diagnostic")?),
        None => None,
    };
    let outcome = build_operator_outcome(
        required(args.kind, "--kind")?,
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

    fn base_args(kind: D1OperatorOutcomeKind) -> Args {
        Args {
            kind: Some(kind),
            source_sha: Some("aa".repeat(20)),
            tree_sha: Some("bb".repeat(20)),
            current_stage_issue: Some(642),
            transaction_id: Some("cc".repeat(32)),
            target_json: None,
            owner_diagnostic_json: None,
            evidence_refs: BTreeMap::new(),
        }
    }

    #[test]
    fn all_required_operator_states_render_canonical_typed_json() -> Result<(), Box<dyn Error>> {
        for kind in D1OperatorOutcomeKind::ALL {
            let output = render(base_args(kind))?;
            let value: Value = serde_json::from_str(&output)?;
            assert_eq!(value["contract"], "D1_OPERATOR_OUTCOME_V1");
            assert_eq!(value["outcome"], kind.as_str());
            assert_eq!(value["reason_code"], kind.as_str());
            assert!(value["remediation"].as_str().is_some_and(|value| !value.is_empty()));
            assert_eq!(value["operator_has_provider_credentials"], false);
        }
        Ok(())
    }

    #[test]
    fn evidence_refs_are_stable_and_sorted() -> Result<(), Box<dyn Error>> {
        let mut args = base_args(D1OperatorOutcomeKind::CompletedVerified);
        args.evidence_refs.insert("receipt".to_owned(), "run:1:artifact:2".to_owned());
        args.evidence_refs.insert("post_state".to_owned(), "run:3:artifact:4".to_owned());
        let output = render(args)?;
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["evidence_refs"]["receipt"], "run:1:artifact:2");
        assert_eq!(value["evidence_refs"]["post_state"], "run:3:artifact:4");
        Ok(())
    }
}
