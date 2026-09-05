#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::transaction::{build_transaction_projection, serialize_transaction_projection};
use serde_json::Value;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

#[derive(Default)]
struct Args {
    prepare_json: Option<PathBuf>,
    observation_json: Option<PathBuf>,
    repository_json: Option<PathBuf>,
    transaction_input_json: Option<PathBuf>,
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
            .ok_or("transaction-plan flags must be valid UTF-8")?;
        match flag {
            "--prepare-json" => set_once(
                &mut args.prepare_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--observation-json" => set_once(
                &mut args.observation_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--repository-json" => set_once(
                &mut args.repository_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--transaction-input-json" => set_once(
                &mut args.transaction_input_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            other => return Err(format!("unsupported transaction-plan argument: {other}").into()),
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

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let prepare = read_strict(
        required(args.prepare_json, "--prepare-json")?,
        "prepare input",
    )?;
    let observation = read_strict(
        required(args.observation_json, "--observation-json")?,
        "provider observation",
    )?;
    let repository = read_strict(
        required(args.repository_json, "--repository-json")?,
        "D1 repository projection",
    )?;
    let transaction_input = read_strict(
        required(args.transaction_input_json, "--transaction-input-json")?,
        "transaction identity input",
    )?;
    let projection =
        build_transaction_projection(&prepare, &observation, &repository, &transaction_input)?;
    println!("{}", serialize_transaction_projection(&projection)?);
    Ok(())
}
