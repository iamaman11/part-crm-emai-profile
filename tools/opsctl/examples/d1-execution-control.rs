#![forbid(unsafe_code)]

use opsctl::canonical::parse_strict_json;
use opsctl::d1::execution_control::{
    ExecutionEventInput, ExecutionReceipt, ExecutionReceiptSeed, TargetFenceLease,
    TargetFenceLeaseInput, TargetFenceObservation, acquire_target_fence, append_execution_event,
    initialize_execution_receipt, serialize_execution_receipt, serialize_target_fence_lease,
    serialize_target_fence_verification, verify_target_fence,
};
use serde::de::DeserializeOwned;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    AcquireFence,
    VerifyFence,
    InitializeReceipt,
    AppendReceipt,
}

impl Command {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "acquire-fence" => Ok(Self::AcquireFence),
            "verify-fence" => Ok(Self::VerifyFence),
            "initialize-receipt" => Ok(Self::InitializeReceipt),
            "append-receipt" => Ok(Self::AppendReceipt),
            _ => Err(format!("unsupported d1 execution-control command: {value}").into()),
        }
    }
}

#[derive(Default)]
struct Args {
    input: Option<PathBuf>,
    lease: Option<PathBuf>,
    observation: Option<PathBuf>,
    seed: Option<PathBuf>,
    receipt: Option<PathBuf>,
    event: Option<PathBuf>,
    prepared_at_unix_seconds: Option<i64>,
    authorized_at_unix_seconds: Option<i64>,
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

fn parse_args() -> Result<(Command, Args), Box<dyn Error>> {
    let mut iterator = env::args_os();
    let _program = iterator.next();
    let command = iterator
        .next()
        .ok_or("d1 execution-control command is required")?
        .into_string()
        .map_err(|_| "d1 execution-control command must be valid UTF-8")?;
    let command = Command::parse(&command)?;
    let mut args = Args::default();

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("d1 execution-control flags must be valid UTF-8")?;
        match flag {
            "--input" => set_once(
                &mut args.input,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--lease" => set_once(
                &mut args.lease,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--observation" => set_once(
                &mut args.observation,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--seed" => set_once(
                &mut args.seed,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--receipt" => set_once(
                &mut args.receipt,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--event" => set_once(
                &mut args.event,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--prepared-at-unix-seconds" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<i64>()?;
                set_once(&mut args.prepared_at_unix_seconds, value, flag)?;
            }
            "--authorized-at-unix-seconds" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<i64>()?;
                set_once(&mut args.authorized_at_unix_seconds, value, flag)?;
            }
            other => return Err(format!("unsupported d1 execution-control argument: {other}").into()),
        }
    }

    Ok((command, args))
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn reject_present<T>(value: &Option<T>, flag: &str, command: &str) -> Result<(), Box<dyn Error>> {
    if value.is_some() {
        return Err(format!("{flag} is not valid for {command}").into());
    }
    Ok(())
}

fn read_typed<T>(path: PathBuf, label: &str) -> Result<T, Box<dyn Error>>
where
    T: DeserializeOwned,
{
    let raw = fs::read_to_string(path)?;
    let value = parse_strict_json(&raw)
        .map_err(|error| format!("{label} is not strict bounded JSON: {error}"))?;
    serde_json::from_value(value)
        .map_err(|error| format!("{label} does not match the typed contract: {error}").into())
}

fn acquire(args: Args) -> Result<String, Box<dyn Error>> {
    reject_present(&args.lease, "--lease", "acquire-fence")?;
    reject_present(&args.observation, "--observation", "acquire-fence")?;
    reject_present(&args.seed, "--seed", "acquire-fence")?;
    reject_present(&args.receipt, "--receipt", "acquire-fence")?;
    reject_present(&args.event, "--event", "acquire-fence")?;
    reject_present(
        &args.prepared_at_unix_seconds,
        "--prepared-at-unix-seconds",
        "acquire-fence",
    )?;
    reject_present(
        &args.authorized_at_unix_seconds,
        "--authorized-at-unix-seconds",
        "acquire-fence",
    )?;
    let input: TargetFenceLeaseInput = read_typed(required(args.input, "--input")?, "target fence input")?;
    let lease = acquire_target_fence(input)?;
    Ok(serialize_target_fence_lease(&lease)?)
}

fn verify(args: Args) -> Result<String, Box<dyn Error>> {
    reject_present(&args.input, "--input", "verify-fence")?;
    reject_present(&args.seed, "--seed", "verify-fence")?;
    reject_present(&args.receipt, "--receipt", "verify-fence")?;
    reject_present(&args.event, "--event", "verify-fence")?;
    reject_present(
        &args.prepared_at_unix_seconds,
        "--prepared-at-unix-seconds",
        "verify-fence",
    )?;
    reject_present(
        &args.authorized_at_unix_seconds,
        "--authorized-at-unix-seconds",
        "verify-fence",
    )?;
    let lease: TargetFenceLease = read_typed(required(args.lease, "--lease")?, "target fence lease")?;
    let observation: TargetFenceObservation = read_typed(
        required(args.observation, "--observation")?,
        "target fence observation",
    )?;
    let verification = verify_target_fence(&lease, &observation)?;
    Ok(serialize_target_fence_verification(&verification)?)
}

fn initialize(args: Args) -> Result<String, Box<dyn Error>> {
    reject_present(&args.input, "--input", "initialize-receipt")?;
    reject_present(&args.lease, "--lease", "initialize-receipt")?;
    reject_present(&args.observation, "--observation", "initialize-receipt")?;
    reject_present(&args.receipt, "--receipt", "initialize-receipt")?;
    reject_present(&args.event, "--event", "initialize-receipt")?;
    let seed: ExecutionReceiptSeed =
        read_typed(required(args.seed, "--seed")?, "execution receipt seed")?;
    let receipt = initialize_execution_receipt(
        seed,
        required(
            args.prepared_at_unix_seconds,
            "--prepared-at-unix-seconds",
        )?,
        required(
            args.authorized_at_unix_seconds,
            "--authorized-at-unix-seconds",
        )?,
    )?;
    Ok(serialize_execution_receipt(&receipt)?)
}

fn append(args: Args) -> Result<String, Box<dyn Error>> {
    reject_present(&args.input, "--input", "append-receipt")?;
    reject_present(&args.lease, "--lease", "append-receipt")?;
    reject_present(&args.observation, "--observation", "append-receipt")?;
    reject_present(&args.seed, "--seed", "append-receipt")?;
    reject_present(
        &args.prepared_at_unix_seconds,
        "--prepared-at-unix-seconds",
        "append-receipt",
    )?;
    reject_present(
        &args.authorized_at_unix_seconds,
        "--authorized-at-unix-seconds",
        "append-receipt",
    )?;
    let receipt: ExecutionReceipt =
        read_typed(required(args.receipt, "--receipt")?, "execution receipt")?;
    let event: ExecutionEventInput =
        read_typed(required(args.event, "--event")?, "execution receipt event")?;
    let next = append_execution_event(&receipt, event)?;
    Ok(serialize_execution_receipt(&next)?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let (command, args) = parse_args()?;
    let output = match command {
        Command::AcquireFence => acquire(args)?,
        Command::VerifyFence => verify(args)?,
        Command::InitializeReceipt => initialize(args)?,
        Command::AppendReceipt => append(args)?,
    };
    println!("{output}");
    Ok(())
}
