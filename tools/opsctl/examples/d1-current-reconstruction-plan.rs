#![forbid(unsafe_code)]

use opsctl::d1::{D1CurrentReconstructionPlanRequest, current_reconstruction_plan};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Default)]
struct Args {
    root: Option<PathBuf>,
    ledger_json: Option<PathBuf>,
    release_manifest: Option<PathBuf>,
    target_json: Option<PathBuf>,
    source_sha: Option<String>,
    tree_sha: Option<String>,
    release_set_id: Option<String>,
    observed_at_unix_seconds: Option<i64>,
    observation_source: Option<String>,
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

fn utf8_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    next_value(iterator, flag)?
        .into_string()
        .map_err(|_| format!("{flag} value must be valid UTF-8").into())
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut iterator = env::args_os();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("reconstruction-plan flags must be valid UTF-8")?;
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
            "--target-json" => set_once(
                &mut args.target_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--source-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.source_sha, value, flag)?;
            }
            "--tree-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.tree_sha, value, flag)?;
            }
            "--release-set-id" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.release_set_id, value, flag)?;
            }
            "--observed-at-unix-seconds" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<i64>()?;
                set_once(&mut args.observed_at_unix_seconds, value, flag)?;
            }
            "--observation-source" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.observation_source, value, flag)?;
            }
            other => {
                return Err(format!("unsupported reconstruction-plan argument: {other}").into());
            }
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn run() -> Result<(), Box<dyn Error>> {
    let Args {
        root,
        ledger_json,
        release_manifest,
        target_json,
        source_sha,
        tree_sha,
        release_set_id,
        observed_at_unix_seconds,
        observation_source,
    } = parse_args()?;

    let root = required(root, "--root")?;
    let ledger_json = required(ledger_json, "--ledger-json")?;
    let release_manifest = required(release_manifest, "--release-manifest")?;
    let target_json = required(target_json, "--target-json")?;
    let source_sha = required(source_sha, "--source-sha")?;
    let tree_sha = required(tree_sha, "--tree-sha")?;
    let release_set_id = required(release_set_id, "--release-set-id")?;
    let observed_at_unix_seconds =
        required(observed_at_unix_seconds, "--observed-at-unix-seconds")?;
    let observation_source = required(observation_source, "--observation-source")?;

    let output = current_reconstruction_plan(D1CurrentReconstructionPlanRequest {
        root: &root,
        ledger_json: &ledger_json,
        release_manifest: &release_manifest,
        target_json: &target_json,
        source_sha: &source_sha,
        tree_sha: &tree_sha,
        release_set_id: &release_set_id,
        observed_at_unix_seconds,
        observation_source: &observation_source,
    })?;
    println!("{output}");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    run()
}
