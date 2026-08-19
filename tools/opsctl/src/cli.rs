use crate::OpsctlError;
use crate::d1;
use std::ffi::OsString;
use std::path::PathBuf;

pub const HELP: &str = "opsctl — project-specific read-only operations interface\n\nUSAGE:\n    opsctl [--root PATH] <COMMAND>\n    opsctl [--root PATH] credentials <ACTION>\n    opsctl [--root PATH] d1 <ACTION> --component COMPONENT --ledger-json PATH [D1 OPTIONS]\n\nCOMMANDS:\n    doctor                     Validate canonical repository authorities\n    status                     Print canonical docs/status.json\n    inventory                  Print canonical architecture/inventory.json\n    credentials status         Print canonical credential lifecycle metadata\n    credentials rotation-plan  Print canonical operator rotation/recovery metadata\n    d1 status                  Classify a saved D1 migration ledger against canonical history\n    d1 plan                    Build a deterministic migration/rollback plan\n    d1 compatibility           Evaluate runtime/schema compatibility\n    d1 verify                  Verify a post-apply ledger against a release schema contract\n\nD1 OPTIONS:\n    --component ID              catalog or resolver\n    --ledger-json PATH          Saved machine-readable Wrangler D1 ledger query result\n    --release-manifest PATH     Target release; required for plan/compatibility/verify\n    --current-manifest PATH     Current runtime schema contract; plan rollback context\n    --known-good-manifest PATH  Known-good rollback release schema contract\n    --preconditions-json PATH   Metadata-only CONTRACT precondition evidence\n    --authority PATH            Optional D1 evolution authority override for fixtures\n\nGLOBAL OPTIONS:\n    --root PATH  Explicit repository root\n    -h, --help   Print help\n    -V, --version\n                 Print version\n\nThis AR-10 interface is read-only and metadata-only. D1 commands parse saved provider output and credentials commands expose only the pre-existing repository metadata semantics. opsctl never executes Python, Node, npx, Wrangler, provider APIs, database mutation, secret access, deployment, or customer-state mutation. AR-13 owns rehearsal-backed credential readiness/rotation operational semantics; release/promotion/recovery/readiness namespaces remain source-reserved until their owning AR slices.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadCommand {
    Doctor,
    Status,
    Inventory,
}

impl ReadCommand {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::Status => "status",
            Self::Inventory => "inventory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialsAction {
    Status,
    RotationPlan,
}

impl CredentialsAction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
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
    Credentials {
        root: Option<PathBuf>,
        action: CredentialsAction,
    },
    D1 {
        root: Option<PathBuf>,
        action: d1::D1Action,
        component: String,
        ledger_json: PathBuf,
        release_manifest: Option<PathBuf>,
        current_manifest: Option<PathBuf>,
        known_good_manifest: Option<PathBuf>,
        preconditions_json: Option<PathBuf>,
        authority: Option<PathBuf>,
    },
}

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
    if command == "credentials" {
        return parse_credentials_invocation(root, iterator);
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

fn parse_credentials_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("credentials", "missing credentials action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("credentials", "credentials action must be valid UTF-8"))?;
    let action = match action_text {
        "status" => CredentialsAction::Status,
        "rotation-plan" => CredentialsAction::RotationPlan,
        other => {
            return Err(OpsctlError::new(
                "credentials",
                format!("unsupported credentials action: {other}"),
            ));
        }
    };
    if let Some(extra) = iterator.next() {
        return Err(OpsctlError::new(
            "credentials",
            format!(
                "unexpected credentials argument: {}",
                extra.to_string_lossy()
            ),
        ));
    }
    Ok(Invocation::Credentials { root, action })
}

fn parse_d1_invocation<I>(root: Option<PathBuf>, mut iterator: I) -> Result<Invocation, OpsctlError>
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
    let mut current_manifest: Option<PathBuf> = None;
    let mut known_good_manifest: Option<PathBuf> = None;
    let mut preconditions_json: Option<PathBuf> = None;
    let mut authority: Option<PathBuf> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("d1", "D1 flags must be valid UTF-8"))?;
        match flag {
            "--component" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("d1", "--component requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("d1", "component must be valid UTF-8"))?;
                set_once(&mut component, value, "--component")?;
            }
            "--ledger-json" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("d1", "--ledger-json requires a value"))?;
                set_once(&mut ledger_json, PathBuf::from(value), "--ledger-json")?;
            }
            "--release-manifest" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("d1", "--release-manifest requires a value"))?;
                set_once(
                    &mut release_manifest,
                    PathBuf::from(value),
                    "--release-manifest",
                )?;
            }
            "--current-manifest" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("d1", "--current-manifest requires a value"))?;
                set_once(
                    &mut current_manifest,
                    PathBuf::from(value),
                    "--current-manifest",
                )?;
            }
            "--known-good-manifest" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("d1", "--known-good-manifest requires a value")
                })?;
                set_once(
                    &mut known_good_manifest,
                    PathBuf::from(value),
                    "--known-good-manifest",
                )?;
            }
            "--preconditions-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("d1", "--preconditions-json requires a value")
                })?;
                set_once(
                    &mut preconditions_json,
                    PathBuf::from(value),
                    "--preconditions-json",
                )?;
            }
            "--authority" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("d1", "--authority requires a value"))?;
                set_once(&mut authority, PathBuf::from(value), "--authority")?;
            }
            other => {
                return Err(OpsctlError::new(
                    "d1",
                    format!("unsupported D1 argument: {other}"),
                ));
            }
        }
    }

    let component = component.ok_or_else(|| OpsctlError::new("d1", "--component is required"))?;
    if component != "catalog" && component != "resolver" {
        return Err(OpsctlError::new(
            "d1",
            "--component must be catalog or resolver",
        ));
    }
    let ledger_json =
        ledger_json.ok_or_else(|| OpsctlError::new("d1", "--ledger-json is required"))?;
    if action.requires_release_manifest() && release_manifest.is_none() {
        return Err(OpsctlError::new(
            "d1",
            format!("d1 {} requires --release-manifest", action.name()),
        ));
    }
    if action == d1::D1Action::Status
        && (release_manifest.is_some()
            || current_manifest.is_some()
            || known_good_manifest.is_some()
            || preconditions_json.is_some())
    {
        return Err(OpsctlError::new(
            "d1",
            "d1 status accepts only component, ledger and authority inputs",
        ));
    }

    Ok(Invocation::D1 {
        root,
        action,
        component,
        ledger_json,
        release_manifest,
        current_manifest,
        known_good_manifest,
        preconditions_json,
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
        other => Err(OpsctlError::new(
            "parse",
            format!(
                "unsupported command {other:?}; opsctl exposes accepted read-only metadata commands only"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialsAction, Invocation, ReadCommand, parse_invocation};
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
    fn parses_modular_credentials_metadata_surface() {
        assert_eq!(
            parse_invocation(args(&["opsctl", "credentials", "status"])),
            Ok(Invocation::Credentials {
                root: None,
                action: CredentialsAction::Status,
            })
        );
        assert_eq!(
            parse_invocation(args(&["opsctl", "credentials", "rotation-plan"])),
            Ok(Invocation::Credentials {
                root: None,
                action: CredentialsAction::RotationPlan,
            })
        );
    }

    #[test]
    fn legacy_flat_credentials_spellings_are_not_cli_authorities() {
        assert!(parse_invocation(args(&["opsctl", "credential-lifecycle"])).is_err());
        assert!(parse_invocation(args(&["opsctl", "rotation-plan"])).is_err());
    }

    #[test]
    fn credentials_readiness_remains_owned_by_ar13() {
        assert!(parse_invocation(args(&["opsctl", "credentials", "readiness"])).is_err());
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
                "target.json",
                "--current-manifest",
                "current.json",
                "--known-good-manifest",
                "known-good.json",
                "--preconditions-json",
                "preconditions.json",
            ])),
            Ok(Invocation::D1 {
                root: Some(PathBuf::from("/repo")),
                action: D1Action::Plan,
                component: "catalog".to_owned(),
                ledger_json: PathBuf::from("ledger.json"),
                release_manifest: Some(PathBuf::from("target.json")),
                current_manifest: Some(PathBuf::from("current.json")),
                known_good_manifest: Some(PathBuf::from("known-good.json")),
                preconditions_json: Some(PathBuf::from("preconditions.json")),
                authority: None,
            })
        );
    }

    #[test]
    fn d1_status_rejects_release_context() {
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "d1",
                "status",
                "--component",
                "catalog",
                "--ledger-json",
                "ledger.json",
                "--known-good-manifest",
                "known-good.json",
            ]))
            .is_err()
        );
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
}
