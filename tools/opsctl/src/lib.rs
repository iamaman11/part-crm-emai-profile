#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const HELP: &str = "opsctl — project-specific read-only operations interface\n\nUSAGE:\n    opsctl [--root PATH] <COMMAND>\n\nCOMMANDS:\n    inventory   Print the current canonical architecture inventory JSON\n    plan        Report whether the current inventory/documentation model is converged\n    doctor      Validate repository markers and the current architecture inventory authority\n    drift       Fail closed when architecture inventory/documentation drift is detected\n\nOPTIONS:\n    --root PATH  Explicit repository root\n    -h, --help   Print help\n    -V, --version\n                 Print version\n\nThis foundation is intentionally read-only. It performs no Cloudflare, provider, database, secret,\nor customer-state mutation.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadCommand {
    Inventory,
    Plan,
    Doctor,
    Drift,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    Help,
    Version,
    Run {
        root: Option<PathBuf>,
        command: ReadCommand,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpsctlError {
    message: String,
}

impl OpsctlError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for OpsctlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for OpsctlError {}

impl From<std::io::Error> for OpsctlError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

pub fn parse_invocation<I>(args: I) -> Result<Invocation, OpsctlError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut iterator = args.into_iter();
    let _program = iterator.next();
    let mut root: Option<PathBuf> = None;
    let mut command: Option<ReadCommand> = None;

    while let Some(argument) = iterator.next() {
        let text = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("opsctl flags and command names must be valid UTF-8"))?;

        match text {
            "-h" | "--help" => {
                if command.is_some() {
                    return Err(OpsctlError::new("--help cannot follow a command"));
                }
                return Ok(Invocation::Help);
            }
            "-V" | "--version" => {
                if command.is_some() {
                    return Err(OpsctlError::new("--version cannot follow a command"));
                }
                return Ok(Invocation::Version);
            }
            "--root" => {
                if root.is_some() {
                    return Err(OpsctlError::new("--root may be supplied only once"));
                }
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("--root requires a path"))?;
                root = Some(PathBuf::from(value));
            }
            value => {
                if command.is_some() {
                    return Err(OpsctlError::new(format!(
                        "unexpected extra argument: {value}"
                    )));
                }
                command = Some(parse_command(value)?);
            }
        }
    }

    let command = command.ok_or_else(|| OpsctlError::new("missing command; use --help"))?;
    Ok(Invocation::Run { root, command })
}

fn parse_command(value: &str) -> Result<ReadCommand, OpsctlError> {
    match value {
        "inventory" => Ok(ReadCommand::Inventory),
        "plan" => Ok(ReadCommand::Plan),
        "doctor" => Ok(ReadCommand::Doctor),
        "drift" => Ok(ReadCommand::Drift),
        other => Err(OpsctlError::new(format!(
            "unsupported command {other:?}; this foundation intentionally exposes read-only inventory/plan/doctor/drift only"
        ))),
    }
}

pub fn execute(invocation: Invocation) -> Result<String, OpsctlError> {
    match invocation {
        Invocation::Help => Ok(HELP.to_owned()),
        Invocation::Version => Ok(format!("opsctl {}\n", env!("CARGO_PKG_VERSION"))),
        Invocation::Run { root, command } => {
            let repo_root = resolve_repo_root(root.as_deref())?;
            match command {
                ReadCommand::Inventory => inventory(&repo_root),
                ReadCommand::Plan => plan(&repo_root),
                ReadCommand::Doctor => doctor(&repo_root),
                ReadCommand::Drift => drift(&repo_root),
            }
        }
    }
}

fn resolve_repo_root(explicit_root: Option<&Path>) -> Result<PathBuf, OpsctlError> {
    if let Some(root) = explicit_root {
        let canonical = fs::canonicalize(root).map_err(|error| {
            OpsctlError::new(format!(
                "cannot resolve explicit repository root {}: {error}",
                root.display()
            ))
        })?;
        if is_repo_root(&canonical) {
            return Ok(canonical);
        }
        return Err(OpsctlError::new(format!(
            "{} is not a repository root with Cargo.toml, architecture/inventory.json and the canonical inventory generator",
            canonical.display()
        )));
    }

    let current = fs::canonicalize(env::current_dir()?)?;
    for candidate in current.ancestors() {
        if is_repo_root(candidate) {
            return Ok(candidate.to_path_buf());
        }
    }

    Err(OpsctlError::new(
        "repository root not found; run inside the repository or provide --root PATH",
    ))
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("architecture/inventory.json").is_file()
        && path
            .join("scripts/generate-architecture-inventory.py")
            .is_file()
}

fn inventory(root: &Path) -> Result<String, OpsctlError> {
    let path = root.join("architecture/inventory.json");
    let mut contents = fs::read_to_string(&path).map_err(|error| {
        OpsctlError::new(format!("cannot read {}: {error}", path.display()))
    })?;
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}

fn plan(root: &Path) -> Result<String, OpsctlError> {
    let output = run_inventory_validator(root)?;
    if output.status.success() {
        return Ok("NO_CHANGE\n".to_owned());
    }

    let details = process_details(&output);
    Ok(format!(
        "PLAN\nstatus=DRIFT_DETECTED\nmutation_executed=false\ncurrent_validator=scripts/generate-architecture-inventory.py --check\ncurrent_write_path=scripts/generate-architecture-inventory.py --write\n{details}"
    ))
}

fn doctor(root: &Path) -> Result<String, OpsctlError> {
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "architecture/inventory.json",
        "scripts/generate-architecture-inventory.py",
        "scripts/check-documentation-authority.py",
        "docs/status.json",
        "docs/DEVELOPMENT_PLAN.md",
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            return Err(OpsctlError::new(format!(
                "doctor failed: required repository authority is missing: {relative}"
            )));
        }
    }

    let output = run_inventory_validator(root)?;
    if !output.status.success() {
        return Err(OpsctlError::new(format!(
            "doctor failed: canonical architecture inventory/documentation validator is not green\n{}",
            process_details(&output)
        )));
    }

    Ok("doctor=ok mode=read-only remote_mutation=false authority=existing-canonical-validator\n"
        .to_owned())
}

fn drift(root: &Path) -> Result<String, OpsctlError> {
    let output = run_inventory_validator(root)?;
    if output.status.success() {
        return Ok("NO_DRIFT\n".to_owned());
    }

    Err(OpsctlError::new(format!(
        "DRIFT_DETECTED\n{}",
        process_details(&output)
    )))
}

fn run_inventory_validator(root: &Path) -> Result<Output, OpsctlError> {
    let python = env::var_os("OPSCTL_PYTHON").unwrap_or_else(|| OsString::from("python"));
    let script = root.join("scripts/generate-architecture-inventory.py");
    Command::new(&python)
        .arg(script)
        .arg("--check")
        .current_dir(root)
        .output()
        .map_err(|error| {
            OpsctlError::new(format!(
                "failed to execute canonical inventory validator with {:?}: {error}; set OPSCTL_PYTHON if Python uses another executable name",
                python
            ))
        })
}

fn process_details(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut details = String::new();

    if !stdout.trim().is_empty() {
        details.push_str("stdout:\n");
        details.push_str(stdout.trim());
        details.push('\n');
    }
    if !stderr.trim().is_empty() {
        details.push_str("stderr:\n");
        details.push_str(stderr.trim());
        details.push('\n');
    }
    if details.is_empty() {
        details.push_str("validator produced no diagnostic output\n");
    }
    details
}

#[cfg(test)]
mod tests {
    use super::{Invocation, ReadCommand, parse_invocation};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().copied().map(OsString::from).collect()
    }

    #[test]
    fn parses_read_only_command() {
        let invocation = parse_invocation(args(&["opsctl", "doctor"]));
        assert_eq!(
            invocation,
            Ok(Invocation::Run {
                root: None,
                command: ReadCommand::Doctor,
            })
        );
    }

    #[test]
    fn parses_explicit_root_before_command() {
        let invocation = parse_invocation(args(&["opsctl", "--root", "/repo", "drift"]));
        assert_eq!(
            invocation,
            Ok(Invocation::Run {
                root: Some(PathBuf::from("/repo")),
                command: ReadCommand::Drift,
            })
        );
    }

    #[test]
    fn rejects_mutation_command() {
        let error = parse_invocation(args(&["opsctl", "provision"]));
        assert!(error.is_err());
    }

    #[test]
    fn help_does_not_require_repository_context() {
        let invocation = parse_invocation(args(&["opsctl", "--help"]));
        assert_eq!(invocation, Ok(Invocation::Help));
    }
}
