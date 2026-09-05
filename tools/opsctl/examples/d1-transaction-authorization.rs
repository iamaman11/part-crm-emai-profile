#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::authorization::{bind_transaction_authorization, serialize_authorization_binding};
use opsctl::d1::transaction::TransactionProjection;
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
            .ok_or("transaction-authorization flags must be valid UTF-8")?;
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
                let raw = next_value(&mut iterator, flag)?;
                let value = raw
                    .to_str()
                    .ok_or("evaluation timestamp must be valid UTF-8")?
                    .parse::<i64>()?;
                set_once(&mut args.evaluated_at_unix_seconds, value, flag)?;
            }
            other => {
                return Err(
                    format!("unsupported transaction-authorization argument: {other}").into(),
                );
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
    let evaluated_at = required(
        args.evaluated_at_unix_seconds,
        "--evaluated-at-unix-seconds",
    )?;
    let binding = bind_transaction_authorization(&transaction, &authorization, evaluated_at)?;
    println!("{}", serialize_authorization_binding(&binding)?);
    Ok(())
}
