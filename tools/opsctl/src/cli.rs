use crate::OpsctlError;
use crate::d1;
use crate::promotion;
use crate::release;
use std::ffi::OsString;
use std::path::PathBuf;

pub const HELP: &str = "opsctl — project-specific operations policy interface\n\nUSAGE:\n    opsctl [--root PATH] <COMMAND>\n    opsctl [--root PATH] credentials <ACTION>\n    opsctl [--root PATH] d1 <ACTION> --component COMPONENT --ledger-json PATH [D1 OPTIONS]\n    opsctl [--root PATH] release <ACTION> --release-set PATH [RELEASE OPTIONS]\n    opsctl [--root PATH] promotion <ACTION> --release-set PATH --profile ID --environment ENV --snapshot PATH --evidence-json PATH [PROMOTION OPTIONS]\n\nCOMMANDS:\n    doctor                     Validate canonical repository authorities\n    status                     Print canonical docs/status.json\n    inventory                  Print canonical architecture/inventory.json\n    credentials status         Print canonical credential lifecycle metadata\n    credentials rotation-plan  Print canonical operator rotation/recovery metadata\n    d1 status                  Classify a saved D1 migration ledger against canonical history\n    d1 plan                    Build a deterministic migration/rollback plan\n    d1 compatibility           Evaluate runtime/schema compatibility\n    d1 verify                  Verify a post-apply ledger against a release schema contract\n    release inspect            Parse and inspect one immutable Release Set\n    release verify             Verify Release Set identity and exact artifact bytes\n    release compatibility      Evaluate Release Set + Capability Profile compatibility\n    promotion plan             Build a deterministic no-mutation transition plan\n    promotion preflight        Fail-closed gate before provider credential exposure\n    promotion verify           Verify observed state after provider mutation\n\nD1 OPTIONS:\n    --component ID              catalog or resolver\n    --ledger-json PATH          Saved machine-readable Wrangler D1 ledger query result\n    --release-manifest PATH     Target release; required for plan/compatibility/verify\n    --current-manifest PATH     Current runtime schema contract; plan rollback context\n    --known-good-manifest PATH  Known-good rollback release schema contract\n    --preconditions-json PATH   Metadata-only CONTRACT precondition evidence\n    --authority PATH            Optional D1 evolution authority override for fixtures\n\nRELEASE OPTIONS:\n    --release-set PATH          Target content-addressed Release Set manifest\n    --artifact-root PATH        Exact artifact tree; required for release verify\n    --profile ID                Target Capability Profile; compatibility only\n    --environment ID            rehearsal, staging, or production; compatibility only\n    --evidence-json PATH        Saved compatibility evidence; compatibility only\n    --current-release-set PATH  Optional current Release Set for rollback context\n\nPROMOTION OPTIONS:\n    --release-set PATH          Target immutable Release Set\n    --profile ID                Target Capability Profile\n    --environment ID            rehearsal, staging, or production\n    --snapshot PATH             Saved metadata-only DeploymentSnapshot\n    --evidence-json PATH        Saved release compatibility evidence\n    --current-release-set PATH  Optional current Release Set\n    --known-good-release-set PATH  Rollback candidate for preflight\n    --expected-current ID       Stale-plan fence (use NONE for fresh state)\n\nGLOBAL OPTIONS:\n    --root PATH  Explicit repository root\n    -h, --help   Print help\n    -V, --version\n                 Print version\n\nAR-11 release commands remain local, read-only and metadata/artifact-verification-only. opsctl never executes Python, Node, npx, Wrangler, provider APIs, database mutation, secret access, deployment, or customer-state mutation. GitHub Actions/Environments retain orchestration/approval authority and provider executors retain actual mutation authority.\n";

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
    Release {
        root: Option<PathBuf>,
        action: release::ReleaseAction,
        release_set: PathBuf,
        artifact_root: Option<PathBuf>,
        profile_id: Option<String>,
        environment: Option<String>,
        evidence_json: Option<PathBuf>,
        current_release_set: Option<PathBuf>,
    },
    Promotion {
        root: Option<PathBuf>,
        action: promotion::PromotionAction,
        release_set: PathBuf,
        profile_id: String,
        environment: String,
        snapshot: PathBuf,
        evidence_json: PathBuf,
        current_release_set: Option<PathBuf>,
        known_good_release_set: Option<PathBuf>,
        expected_current_release_set_id: Option<String>,
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

    match command.as_str() {
        "d1" => parse_d1_invocation(root, iterator),
        "credentials" => parse_credentials_invocation(root, iterator),
        "release" => parse_release_invocation(root, iterator),
        "promotion" => parse_promotion_invocation(root, iterator),
        _ => {
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
    }
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
    if !matches!(component.as_str(), "catalog" | "resolver") {
        return Err(OpsctlError::new(
            "d1",
            "--component must be catalog or resolver",
        ));
    }
    let ledger_json =
        ledger_json.ok_or_else(|| OpsctlError::new("d1", "--ledger-json is required"))?;

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

fn parse_release_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("release", "missing release action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("release", "release action must be valid UTF-8"))?;
    let action = match action_text {
        "inspect" => release::ReleaseAction::Inspect,
        "verify" => release::ReleaseAction::Verify,
        "compatibility" => release::ReleaseAction::Compatibility,
        other => {
            return Err(OpsctlError::new(
                "release",
                format!("unsupported release action: {other}"),
            ));
        }
    };

    let mut release_set: Option<PathBuf> = None;
    let mut artifact_root: Option<PathBuf> = None;
    let mut profile_id: Option<String> = None;
    let mut environment: Option<String> = None;
    let mut evidence_json: Option<PathBuf> = None;
    let mut current_release_set: Option<PathBuf> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("release", "release flags must be valid UTF-8"))?;
        match flag {
            "--release-set" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("release", "--release-set requires a value"))?;
                set_once(&mut release_set, PathBuf::from(value), "--release-set")?;
            }
            "--artifact-root" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("release", "--artifact-root requires a value")
                })?;
                set_once(&mut artifact_root, PathBuf::from(value), "--artifact-root")?;
            }
            "--profile" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("release", "--profile requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("release", "profile must be valid UTF-8"))?;
                set_once(&mut profile_id, value, "--profile")?;
            }
            "--environment" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("release", "--environment requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("release", "environment must be valid UTF-8"))?;
                set_once(&mut environment, value, "--environment")?;
            }
            "--evidence-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("release", "--evidence-json requires a value")
                })?;
                set_once(&mut evidence_json, PathBuf::from(value), "--evidence-json")?;
            }
            "--current-release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("release", "--current-release-set requires a value")
                })?;
                set_once(
                    &mut current_release_set,
                    PathBuf::from(value),
                    "--current-release-set",
                )?;
            }
            other => {
                return Err(OpsctlError::new(
                    "release",
                    format!("unsupported release argument: {other}"),
                ));
            }
        }
    }

    let release_set =
        release_set.ok_or_else(|| OpsctlError::new("release", "--release-set is required"))?;
    match action {
        release::ReleaseAction::Inspect => {
            reject_if_present(&artifact_root, "--artifact-root", action)?;
            reject_if_present(&profile_id, "--profile", action)?;
            reject_if_present(&environment, "--environment", action)?;
            reject_if_present(&evidence_json, "--evidence-json", action)?;
            reject_if_present(&current_release_set, "--current-release-set", action)?;
        }
        release::ReleaseAction::Verify => {
            if artifact_root.is_none() {
                return Err(OpsctlError::new(
                    "release",
                    "release verify requires --artifact-root",
                ));
            }
            reject_if_present(&profile_id, "--profile", action)?;
            reject_if_present(&environment, "--environment", action)?;
            reject_if_present(&evidence_json, "--evidence-json", action)?;
            reject_if_present(&current_release_set, "--current-release-set", action)?;
        }
        release::ReleaseAction::Compatibility => {
            if profile_id.is_none() || environment.is_none() || evidence_json.is_none() {
                return Err(OpsctlError::new(
                    "release",
                    "release compatibility requires --profile, --environment, and --evidence-json",
                ));
            }
            reject_if_present(&artifact_root, "--artifact-root", action)?;
        }
    }

    Ok(Invocation::Release {
        root,
        action,
        release_set,
        artifact_root,
        profile_id,
        environment,
        evidence_json,
        current_release_set,
    })
}

fn reject_if_present<T>(
    value: &Option<T>,
    flag: &str,
    action: release::ReleaseAction,
) -> Result<(), OpsctlError> {
    if value.is_some() {
        Err(OpsctlError::new(
            "release",
            format!("{flag} is not valid for release {}", action.name()),
        ))
    } else {
        Ok(())
    }
}

fn parse_promotion_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("promotion", "missing promotion action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("promotion", "promotion action must be valid UTF-8"))?;
    let action = match action_text {
        "plan" => promotion::PromotionAction::Plan,
        "preflight" => promotion::PromotionAction::Preflight,
        "verify" => promotion::PromotionAction::Verify,
        other => {
            return Err(OpsctlError::new(
                "promotion",
                format!("unsupported promotion action: {other}"),
            ));
        }
    };

    let mut release_set: Option<PathBuf> = None;
    let mut profile_id: Option<String> = None;
    let mut environment: Option<String> = None;
    let mut snapshot: Option<PathBuf> = None;
    let mut evidence_json: Option<PathBuf> = None;
    let mut current_release_set: Option<PathBuf> = None;
    let mut known_good_release_set: Option<PathBuf> = None;
    let mut expected_current_release_set_id: Option<String> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("promotion", "promotion flags must be valid UTF-8"))?;
        match flag {
            "--release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--release-set requires a value")
                })?;
                set_once(&mut release_set, PathBuf::from(value), "--release-set")?;
            }
            "--profile" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--profile requires a value"))?
                    .into_string()
                    .map_err(|_| OpsctlError::new("promotion", "profile must be valid UTF-8"))?;
                set_once(&mut profile_id, value, "--profile")?;
            }
            "--environment" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--environment requires a value"))?
                    .into_string()
                    .map_err(|_| {
                        OpsctlError::new("promotion", "environment must be valid UTF-8")
                    })?;
                set_once(&mut environment, value, "--environment")?;
            }
            "--snapshot" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("promotion", "--snapshot requires a value"))?;
                set_once(&mut snapshot, PathBuf::from(value), "--snapshot")?;
            }
            "--evidence-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--evidence-json requires a value")
                })?;
                set_once(&mut evidence_json, PathBuf::from(value), "--evidence-json")?;
            }
            "--current-release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--current-release-set requires a value")
                })?;
                set_once(
                    &mut current_release_set,
                    PathBuf::from(value),
                    "--current-release-set",
                )?;
            }
            "--known-good-release-set" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--known-good-release-set requires a value")
                })?;
                set_once(
                    &mut known_good_release_set,
                    PathBuf::from(value),
                    "--known-good-release-set",
                )?;
            }
            "--expected-current" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| {
                        OpsctlError::new("promotion", "--expected-current requires a value")
                    })?
                    .into_string()
                    .map_err(|_| {
                        OpsctlError::new("promotion", "expected current must be valid UTF-8")
                    })?;
                set_once(
                    &mut expected_current_release_set_id,
                    value,
                    "--expected-current",
                )?;
            }
            other => {
                return Err(OpsctlError::new(
                    "promotion",
                    format!("unsupported promotion argument: {other}"),
                ));
            }
        }
    }

    let release_set =
        release_set.ok_or_else(|| OpsctlError::new("promotion", "--release-set is required"))?;
    let profile_id =
        profile_id.ok_or_else(|| OpsctlError::new("promotion", "--profile is required"))?;
    let environment =
        environment.ok_or_else(|| OpsctlError::new("promotion", "--environment is required"))?;
    if !matches!(environment.as_str(), "rehearsal" | "staging" | "production") {
        return Err(OpsctlError::new(
            "promotion",
            "--environment must be rehearsal, staging, or production",
        ));
    }
    let snapshot =
        snapshot.ok_or_else(|| OpsctlError::new("promotion", "--snapshot is required"))?;
    let evidence_json = evidence_json
        .ok_or_else(|| OpsctlError::new("promotion", "--evidence-json is required"))?;

    match action {
        promotion::PromotionAction::Plan => {
            reject_if_present_promotion(
                &known_good_release_set,
                "--known-good-release-set",
                action,
            )?;
        }
        promotion::PromotionAction::Preflight => {}
        promotion::PromotionAction::Verify => {
            reject_if_present_promotion(&current_release_set, "--current-release-set", action)?;
            reject_if_present_promotion(
                &known_good_release_set,
                "--known-good-release-set",
                action,
            )?;
            reject_if_present_promotion(
                &expected_current_release_set_id,
                "--expected-current",
                action,
            )?;
        }
    }

    Ok(Invocation::Promotion {
        root,
        action,
        release_set,
        profile_id,
        environment,
        snapshot,
        evidence_json,
        current_release_set,
        known_good_release_set,
        expected_current_release_set_id,
    })
}

fn reject_if_present_promotion<T>(
    value: &Option<T>,
    flag: &str,
    action: promotion::PromotionAction,
) -> Result<(), OpsctlError> {
    if value.is_some() {
        Err(OpsctlError::new(
            "promotion",
            format!("{flag} is not valid for promotion {}", action.name()),
        ))
    } else {
        Ok(())
    }
}

fn parse_command(value: &str) -> Result<ReadCommand, OpsctlError> {
    match value {
        "doctor" => Ok(ReadCommand::Doctor),
        "status" => Ok(ReadCommand::Status),
        "inventory" => Ok(ReadCommand::Inventory),
        other => Err(OpsctlError::new(
            "parse",
            format!("unsupported command: {other}"),
        )),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), OpsctlError> {
    if slot.is_some() {
        Err(OpsctlError::new(
            "parse",
            format!("{flag} may be supplied only once"),
        ))
    } else {
        *slot = Some(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Invocation, ReadCommand, parse_invocation};
    use crate::release::ReleaseAction;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_read_command_with_root() {
        let invocation = parse_invocation(args(&["opsctl", "--root", ".", "inventory"]));
        assert_eq!(
            invocation,
            Ok(Invocation::Run {
                root: Some(PathBuf::from(".")),
                command: ReadCommand::Inventory,
            })
        );
    }

    #[test]
    fn activates_release_inspect_in_ar11() {
        let invocation = parse_invocation(args(&[
            "opsctl",
            "release",
            "inspect",
            "--release-set",
            "release-set.json",
        ]));
        assert_eq!(
            invocation,
            Ok(Invocation::Release {
                root: None,
                action: ReleaseAction::Inspect,
                release_set: PathBuf::from("release-set.json"),
                artifact_root: None,
                profile_id: None,
                environment: None,
                evidence_json: None,
                current_release_set: None,
            })
        );
    }

    #[test]
    fn release_verify_requires_artifact_root() {
        let invocation = parse_invocation(args(&[
            "opsctl",
            "release",
            "verify",
            "--release-set",
            "release-set.json",
        ]));
        assert!(invocation.is_err());
    }

    #[test]
    fn release_compatibility_requires_complete_inputs() {
        let invocation = parse_invocation(args(&[
            "opsctl",
            "release",
            "compatibility",
            "--release-set",
            "release-set.json",
            "--profile",
            "rehearsal-core-v1",
        ]));
        assert!(invocation.is_err());
    }

    #[test]
    fn promotion_namespace_remains_fail_closed_until_activated() {
        let invocation = parse_invocation(args(&["opsctl", "promotion", "plan"]));
        assert!(invocation.is_err());
    }
}
