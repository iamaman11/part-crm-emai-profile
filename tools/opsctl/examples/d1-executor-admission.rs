#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::executor_admission::{
    ExecutorAdmissionExpectation, bind_executor_admission, serialize_executor_admission,
};
use opsctl::d1::transaction::{TargetIdentity, TransactionPhase, TransactionProjection};
use serde_json::Value;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

#[derive(Default)]
struct Args {
    transaction_json: Option<PathBuf>,
    authorization_json: Option<PathBuf>,
    evaluated_at_unix_seconds: Option<i64>,
    expected_transaction_id: Option<String>,
    expected_source_sha: Option<String>,
    expected_tree_sha: Option<String>,
    expected_environment: Option<String>,
    expected_account_id: Option<String>,
    expected_database_name: Option<String>,
    expected_database_id: Option<String>,
    expected_phase: Option<TransactionPhase>,
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

fn parse_phase(raw: String) -> Result<TransactionPhase, Box<dyn Error>> {
    match raw.as_str() {
        "ORDINARY" => Ok(TransactionPhase::Ordinary),
        "CONTRACT" => Ok(TransactionPhase::Contract),
        _ => Err("--expected-phase must be ORDINARY or CONTRACT".into()),
    }
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut iterator = env::args_os();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("executor-admission flags must be valid UTF-8")?;
        match flag {
            "--transaction-json" => set_once(
                &mut args.transaction_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--authorization-json" => set_once(
                &mut args.authorization_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--evaluated-at-unix-seconds" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<i64>()?;
                set_once(&mut args.evaluated_at_unix_seconds, value, flag)?;
            }
            "--expected-transaction-id" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_transaction_id, value, flag)?;
            }
            "--expected-source-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_source_sha, value, flag)?;
            }
            "--expected-tree-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_tree_sha, value, flag)?;
            }
            "--expected-environment" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_environment, value, flag)?;
            }
            "--expected-account-id" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_account_id, value, flag)?;
            }
            "--expected-database-name" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_database_name, value, flag)?;
            }
            "--expected-database-id" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_database_id, value, flag)?;
            }
            "--expected-phase" => {
                let value = parse_phase(utf8_value(&mut iterator, flag)?)?;
                set_once(&mut args.expected_phase, value, flag)?;
            }
            other => return Err(format!("unsupported executor-admission argument: {other}").into()),
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
    let transaction_value = read_strict(
        required(args.transaction_json, "--transaction-json")?,
        "prepared transaction",
    )?;
    let transaction: TransactionProjection =
        serde_json::from_value(transaction_value).map_err(|error| {
            format!("prepared transaction does not match the typed contract: {error}")
        })?;
    let authorization = read_strict(
        required(args.authorization_json, "--authorization-json")?,
        "transaction authorization",
    )?;
    let expectation = ExecutorAdmissionExpectation {
        transaction_id: required(args.expected_transaction_id, "--expected-transaction-id")?,
        source_sha: required(args.expected_source_sha, "--expected-source-sha")?,
        tree_sha: required(args.expected_tree_sha, "--expected-tree-sha")?,
        target: TargetIdentity {
            environment: required(args.expected_environment, "--expected-environment")?,
            account_id: required(args.expected_account_id, "--expected-account-id")?,
            database_name: required(args.expected_database_name, "--expected-database-name")?,
            database_id: required(args.expected_database_id, "--expected-database-id")?,
        },
        phase: required(args.expected_phase, "--expected-phase")?,
    };
    let binding = bind_executor_admission(
        &transaction,
        &authorization,
        required(
            args.evaluated_at_unix_seconds,
            "--evaluated-at-unix-seconds",
        )?,
        &expectation,
    )?;
    println!("{}", serialize_executor_admission(&binding)?);
    Ok(())
}
