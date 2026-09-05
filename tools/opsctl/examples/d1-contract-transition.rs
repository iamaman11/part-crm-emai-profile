#![forbid(unsafe_code)]

use opsctl::d1::{D1ContractTransitionRequest, contract_transition};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Default)]
struct Args {
    root: Option<PathBuf>,
    ledger_json: Option<PathBuf>,
    release_manifest: Option<PathBuf>,
    evidence_json: Option<PathBuf>,
    evaluated_at_unix_seconds: Option<i64>,
    expected_source_sha: Option<String>,
    expected_release_set_id: Option<String>,
}

fn next_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<OsString, Box<dyn Error>> {
    iterator
        .next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), Box<dyn Error>> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be supplied only once").into());
    }
    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut iterator = env::args_os();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("contract-transition flags must be valid UTF-8")?;
        match flag {
            "--root" => set_once(
                &mut args.root,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--ledger-json" => set_once(
                &mut args.ledger_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--release-manifest" => set_once(
                &mut args.release_manifest,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--evidence-json" => set_once(
                &mut args.evidence_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--evaluated-at-unix-seconds" => {
                let value = next_value(&mut iterator, flag)?
                    .into_string()
                    .map_err(|_| "evaluated time must be valid UTF-8")?
                    .parse::<i64>()?;
                set_once(&mut args.evaluated_at_unix_seconds, value, flag)?;
            }
            "--expected-source-sha" => {
                let value = next_value(&mut iterator, flag)?
                    .into_string()
                    .map_err(|_| "expected source SHA must be valid UTF-8")?;
                set_once(&mut args.expected_source_sha, value, flag)?;
            }
            "--expected-release-set-id" => {
                let value = next_value(&mut iterator, flag)?
                    .into_string()
                    .map_err(|_| "expected Release Set id must be valid UTF-8")?;
                set_once(&mut args.expected_release_set_id, value, flag)?;
            }
            other => return Err(format!("unsupported contract-transition argument: {other}").into()),
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let root = required(args.root, "--root")?;
    let ledger_json = required(args.ledger_json, "--ledger-json")?;
    let release_manifest = required(args.release_manifest, "--release-manifest")?;
    let evidence_json = required(args.evidence_json, "--evidence-json")?;
    let evaluated_at_unix_seconds = required(
        args.evaluated_at_unix_seconds,
        "--evaluated-at-unix-seconds",
    )?;
    let expected_source_sha = required(args.expected_source_sha, "--expected-source-sha")?;
    let expected_release_set_id = required(
        args.expected_release_set_id,
        "--expected-release-set-id",
    )?;
    let output = contract_transition(D1ContractTransitionRequest {
        root: &root,
        ledger_json: &ledger_json,
        release_manifest: &release_manifest,
        evidence_json: &evidence_json,
        evaluated_at_unix_seconds,
        expected_source_sha: &expected_source_sha,
        expected_release_set_id: &expected_release_set_id,
    })?;
    print!("{output}");
    Ok(())
}
