#![forbid(unsafe_code)]

mod d1;

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const HELP: &str = "opsctl — project-specific read-only operations interface\n\nUSAGE:\n    opsctl [--root PATH] <COMMAND>\n    opsctl [--root PATH] d1 <ACTION> --component COMPONENT --ledger-json PATH [--release-manifest PATH] [--authority PATH]\n\nCOMMANDS:\n    doctor                Validate canonical repository authorities\n    status                Print canonical docs/status.json\n    inventory             Print canonical architecture/inventory.json\n    credential-lifecycle  Print canonical credential lifecycle metadata\n    rotation-plan         Print canonical operator rotation/recovery contract\n    d1 status             Classify a saved D1 migration ledger against canonical history\n    d1 plan               Build a deterministic migration plan for a release schema contract\n    d1 compatibility      Evaluate runtime/schema compatibility and rollback safety\n    d1 verify             Verify a post-apply ledger against a release schema contract\n\nD1 OPTIONS:\n    --component ID          catalog or resolver\n    --ledger-json PATH      Saved machine-readable Wrangler D1 ledger query result\n    --release-manifest PATH Required for plan/compatibility/verify\n    --authority PATH        Optional D1 evolution authority override for fixtures\n\nGLOBAL OPTIONS:\n    --root PATH  Explicit repository root\n    -h, --help   Print help\n    -V, --version\n                 Print version\n\nThis interface is permanently read-only and metadata-only. D1 commands parse saved provider output and never execute Python, Node, npx, Wrangler, provider APIs, database mutation, secret access, deployment, or customer-state mutation.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadCommand {
    Doctor,
    Status,
    Inventory,
    CredentialLifecycle,
    RotationPlan,
}

impl ReadCommand {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::Status => "status",
            Self::Inventory => "inventory",
            Self::CredentialLifecycle => "credential-lifecycle",
            Self::RotationPlan => "rotation-plan",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    Help,
    Version,
    Run {
        root: Option<PathBuf>,
        command: ReadCommand,
    },
    D1 {
        root: Option<PathBuf>,
        action: d1::D1Action,
        component: String,
        ledger_json: PathBuf,
        release_manifest: Option<PathBuf>,
        authority: Option<PathBuf>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpsctlError {
    command: &'static str,
    message: String,
}

impl OpsctlError {
    fn new(command: &'static str, message: impl Into<String>) -> Self {
        Self {
            command,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn json(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"command\":\"{}\",\"status\":\"error\",\"mode\":\"read-only\",\"mutation_executed\":false,\"error\":\"{}\"}}\n",
            json_escape(self.command),
            json_escape(&self.message)
        )
    }
}

impl fmt::Display for OpsctlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for OpsctlError {}

pub fn parse_invocation<I>(args: I) -> Result<Invocation, OpsctlError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut iterator = args.into_iter();
    let _program = iterator.next();
    let mut root: Option<PathBuf> = None;

    let command = loop {
        let argument = iterator
            .next()
            .ok_or_else(|| OpsctlError::new("parse", "missing command; use --help"))?;
        let text = argument.to_str().ok_or_else(|| {
            OpsctlError::new("parse", "flags and command names must be valid UTF-8")
        })?;
        match text {
            "-h" | "--help" => return Ok(Invocation::Help),
            "-V" | "--version" => return Ok(Invocation::Version),
            "--root" => {
                if root.is_some() {
                    return Err(OpsctlError::new(
                        "parse",
                        "--root may be supplied only once",
                    ));
                }
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("parse", "--root requires a path"))?;
                root = Some(PathBuf::from(value));
            }
            value => break value.to_owned(),
        }
    };

    if command == "d1" {
        return parse_d1_invocation(root, iterator);
    }

    let read_command = parse_command(&command)?;
    if let Some(extra) = iterator.next() {
        return Err(OpsctlError::new(
            "parse",
            format!("unexpected extra argument: {}", extra.to_string_lossy()),
        ));
    }
    Ok(Invocation::Run {
        root,
        command: read_command,
    })
}

fn parse_d1_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("d1", "missing D1 action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("d1", "D1 action must be valid UTF-8"))?;
    let action = match action_text {
        "status" => d1::D1Action::Status,
        "plan" => d1::D1Action::Plan,
        "compatibility" => d1::D1Action::Compatibility,
        "verify" => d1::D1Action::Verify,
        other => {
            return Err(OpsctlError::new(
                "d1",
                format!("unsupported D1 action: {other}"),
            ));
        }
    };

    let mut component: Option<String> = None;
    let mut ledger_json: Option<PathBuf> = None;
    let mut release_manifest: Option<PathBuf> = None;
    let mut authority: Option<PathBuf> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("d1", "D1 flags must be valid UTF-8"))?;
        let value = match flag {
            "--component" | "--ledger-json" | "--release-manifest" | "--authority" => iterator
                .next()
                .ok_or_else(|| OpsctlError::new("d1", format!("{flag} requires a value")))?,
            other => {
                return Err(OpsctlError::new(
                    "d1",
                    format!("unsupported D1 argument: {other}"),
                ));
            }
        };
        match flag {
            "--component" => {
                set_once(
                    &mut component,
                    value
                        .into_string()
                        .map_err(|_| OpsctlError::new("d1", "component must be valid UTF-8"))?,
                    "--component",
                )?;
            }
            "--ledger-json" => set_once(&mut ledger_json, PathBuf::from(value), "--ledger-json")?,
            "--release-manifest" => set_once(
                &mut release_manifest,
                PathBuf::from(value),
                "--release-manifest",
            )?,
            "--authority" => set_once(&mut authority, PathBuf::from(value), "--authority")?,
            _ => unreachable!("D1 flag was matched above"),
        }
    }

    let component = component.ok_or_else(|| OpsctlError::new("d1", "--component is required"))?;
    if component != "catalog" && component != "resolver" {
        return Err(OpsctlError::new(
            "d1",
            "--component must be catalog or resolver",
        ));
    }
    let ledger_json = ledger_json.ok_or_else(|| OpsctlError::new("d1", "--ledger-json is required"))?;
    if action.requires_release_manifest() && release_manifest.is_none() {
        return Err(OpsctlError::new(
            "d1",
            format!("d1 {} requires --release-manifest", action.name()),
        ));
    }
    if action == d1::D1Action::Status && release_manifest.is_some() {
        return Err(OpsctlError::new(
            "d1",
            "d1 status does not accept --release-manifest",
        ));
    }

    Ok(Invocation::D1 {
        root,
        action,
        component,
        ledger_json,
        release_manifest,
        authority,
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), OpsctlError> {
    if slot.is_some() {
        return Err(OpsctlError::new(
            "d1",
            format!("{flag} may be supplied only once"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_command(value: &str) -> Result<ReadCommand, OpsctlError> {
    match value {
        "doctor" => Ok(ReadCommand::Doctor),
        "status" => Ok(ReadCommand::Status),
        "inventory" => Ok(ReadCommand::Inventory),
        "credential-lifecycle" => Ok(ReadCommand::CredentialLifecycle),
        "rotation-plan" => Ok(ReadCommand::RotationPlan),
        other => Err(OpsctlError::new(
            "parse",
            format!(
                "unsupported command {other:?}; opsctl exposes read-only metadata commands only"
            ),
        )),
    }
}

pub fn execute(invocation: Invocation) -> Result<String, OpsctlError> {
    match invocation {
        Invocation::Help => Ok(HELP.to_owned()),
        Invocation::Version => Ok(format!("opsctl {}\n", env!("CARGO_PKG_VERSION"))),
        Invocation::Run { root, command } => {
            let repo_root = resolve_repo_root(root.as_deref(), command.name())?;
            match command {
                ReadCommand::Doctor => doctor(&repo_root),
                ReadCommand::Status => {
                    canonical_json_document(&repo_root, "docs/status.json", "status")
                }
                ReadCommand::Inventory => {
                    canonical_json_document(&repo_root, "architecture/inventory.json", "inventory")
                }
                ReadCommand::CredentialLifecycle => canonical_json_document(
                    &repo_root,
                    "architecture/credential-lifecycle.json",
                    "credential-lifecycle",
                ),
                ReadCommand::RotationPlan => canonical_json_document(
                    &repo_root,
                    "architecture/operator-contract.json",
                    "rotation-plan",
                ),
            }
        }
        Invocation::D1 {
            root,
            action,
            component,
            ledger_json,
            release_manifest,
            authority,
        } => {
            let repo_root = resolve_repo_root(root.as_deref(), "d1")?;
            d1::run(
                &repo_root,
                action,
                &component,
                &ledger_json,
                release_manifest.as_deref(),
                authority.as_deref(),
            )
            .map_err(|error| OpsctlError::new("d1", error.to_string()))
        }
    }
}

fn resolve_repo_root(
    explicit: Option<&Path>,
    command: &'static str,
) -> Result<PathBuf, OpsctlError> {
    if let Some(root) = explicit {
        let canonical = fs::canonicalize(root).map_err(|error| {
            OpsctlError::new(
                command,
                format!("cannot resolve repository root {}: {error}", root.display()),
            )
        })?;
        if is_repo_root(&canonical) {
            return Ok(canonical);
        }
        return Err(OpsctlError::new(
            command,
            "explicit path is not the canonical repository root",
        ));
    }

    let current = fs::canonicalize(
        env::current_dir().map_err(|error| OpsctlError::new(command, error.to_string()))?,
    )
    .map_err(|error| OpsctlError::new(command, error.to_string()))?;
    for candidate in current.ancestors() {
        if is_repo_root(candidate) {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(OpsctlError::new(
        command,
        "repository root not found; provide --root PATH",
    ))
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("architecture/inventory.json").is_file()
        && path.join("architecture/python-estate-ar6.json").is_file()
        && path
            .join("architecture/credential-authority.json")
            .is_file()
        && path
            .join("architecture/credential-lifecycle.json")
            .is_file()
        && path.join("architecture/profile-security.json").is_file()
        && path.join("architecture/operator-contract.json").is_file()
        && path
            .join("scripts/generate-architecture-inventory.py")
            .is_file()
        && path.join("scripts/python-estate-ar6.py").is_file()
}

fn canonical_json_document(
    root: &Path,
    relative: &str,
    command: &'static str,
) -> Result<String, OpsctlError> {
    let path = root.join(relative);
    let mut contents = fs::read_to_string(&path)
        .map_err(|error| OpsctlError::new(command, format!("cannot read {relative}: {error}")))?;
    let trimmed = contents.trim_start();
    if !trimmed.starts_with('{') || !contents.trim_end().ends_with('}') {
        return Err(OpsctlError::new(
            command,
            format!("canonical JSON authority is malformed: {relative}"),
        ));
    }
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}

fn doctor(root: &Path) -> Result<String, OpsctlError> {
    for relative in [
        "docs/status.json",
        "architecture/inventory.json",
        "architecture/python-estate-ar6.json",
        "architecture/credential-authority.json",
        "architecture/credential-lifecycle.json",
        "architecture/profile-security.json",
        "architecture/operator-contract.json",
        "scripts/generate-architecture-inventory.py",
        "scripts/python-estate-ar6.py",
    ] {
        if !root.join(relative).is_file() {
            return Err(OpsctlError::new(
                "doctor",
                format!("required canonical authority is missing: {relative}"),
            ));
        }
    }

    for relative in [
        "architecture/credential-authority.json",
        "architecture/credential-lifecycle.json",
        "architecture/profile-security.json",
        "architecture/operator-contract.json",
    ] {
        let _ = canonical_json_document(root, relative, "doctor")?;
    }

    run_python_check(
        root,
        "scripts/generate-architecture-inventory.py",
        &["--check"],
    )?;
    run_python_check(root, "scripts/python-estate-ar6.py", &["--check"])?;
    Ok("{\"schema_version\":1,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"read-only\",\"mutation_executed\":false,\"authorities\":[\"architecture/inventory.json\",\"architecture/python-estate-ar6.json\",\"architecture/credential-authority.json\",\"architecture/credential-lifecycle.json\",\"architecture/profile-security.json\",\"architecture/operator-contract.json\",\"docs/status.json\"]}\n".to_owned())
}

fn run_python_check(root: &Path, script: &str, arguments: &[&str]) -> Result<(), OpsctlError> {
    let python = env::var_os("OPSCTL_PYTHON").unwrap_or_else(|| OsString::from("python"));
    let output = Command::new(&python)
        .arg(root.join(script))
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| OpsctlError::new("doctor", format!("cannot execute {script}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(OpsctlError::new(
        "doctor",
        format!("canonical validator failed: {script}: {detail}"),
    ))
}

fn json_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() => output.push_str(&format!("\\u{:04x}", c as u32)),
            c => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{Invocation, OpsctlError, ReadCommand, execute, json_escape, parse_invocation};
    use crate::d1::D1Action;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().copied().map(OsString::from).collect()
    }

    #[test]
    fn parses_existing_read_only_surface() {
        for (name, expected) in [
            ("doctor", ReadCommand::Doctor),
            ("status", ReadCommand::Status),
            ("inventory", ReadCommand::Inventory),
            ("credential-lifecycle", ReadCommand::CredentialLifecycle),
            ("rotation-plan", ReadCommand::RotationPlan),
        ] {
            assert_eq!(
                parse_invocation(args(&["opsctl", name])),
                Ok(Invocation::Run {
                    root: None,
                    command: expected
                })
            );
        }
    }

    #[test]
    fn parses_explicit_root() {
        assert_eq!(
            parse_invocation(args(&["opsctl", "--root", "/repo", "status"])),
            Ok(Invocation::Run {
                root: Some(PathBuf::from("/repo")),
                command: ReadCommand::Status
            })
        );
    }

    #[test]
    fn parses_native_d1_surface() {
        assert_eq!(
            parse_invocation(args(&[
                "opsctl",
                "--root",
                "/repo",
                "d1",
                "plan",
                "--component",
                "catalog",
                "--ledger-json",
                "ledger.json",
                "--release-manifest",
                "release.json",
            ])),
            Ok(Invocation::D1 {
                root: Some(PathBuf::from("/repo")),
                action: D1Action::Plan,
                component: "catalog".to_owned(),
                ledger_json: PathBuf::from("ledger.json"),
                release_manifest: Some(PathBuf::from("release.json")),
                authority: None,
            })
        );
    }

    #[test]
    fn d1_status_rejects_release_manifest() {
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "d1",
                "status",
                "--component",
                "catalog",
                "--ledger-json",
                "ledger.json",
                "--release-manifest",
                "release.json",
            ]))
            .is_err()
        );
    }

    #[test]
    fn credential_lifecycle_reads_subject_authority() -> Result<(), OpsctlError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = execute(Invocation::Run {
            root: Some(root),
            command: ReadCommand::CredentialLifecycle,
        })?;
        assert!(output.contains("\"kind\": \"CREDENTIAL_LIFECYCLE_AUTHORITY\""));
        assert!(output.contains("\"routine_release_rotates_runtime_secrets\": false"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }

    #[test]
    fn rotation_plan_reads_subject_operator_contract() -> Result<(), OpsctlError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = execute(Invocation::Run {
            root: Some(root),
            command: ReadCommand::RotationPlan,
        })?;
        assert!(output.contains("\"kind\": \"OPERATOR_CONTRACT_AUTHORITY\""));
        assert!(output.contains("\"mode\": \"READ_ONLY_METADATA_ONLY\""));
        assert!(output.contains("\"production_mutation\": false"));
        assert!(!output.contains("\"secret_value\":"));
        Ok(())
    }

    #[test]
    fn rejects_mutation_commands() {
        for command in [
            "deploy",
            "provision",
            "promote",
            "delete",
            "rotate",
            "migrate",
        ] {
            assert!(parse_invocation(args(&["opsctl", command])).is_err());
        }
    }

    #[test]
    fn json_errors_are_escaped() {
        assert_eq!(json_escape("a\n\"b\\c"), "a\\n\\\"b\\\\c");
    }
}
