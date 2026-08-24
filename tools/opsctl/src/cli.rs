use crate::OpsctlError;
use crate::d1;
use crate::hosted_evidence::HostedEvidenceAction;
use crate::promotion;
use crate::release;
use std::ffi::OsString;
use std::path::PathBuf;

pub const HELP: &str = include_str!("help.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadCommand {
    Doctor,
}

impl ReadCommand {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
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
    Status {
        root: Option<PathBuf>,
        acceptance_evidence_json: PathBuf,
    },
    Credentials {
        root: Option<PathBuf>,
        action: CredentialsAction,
    },
    HostedEvidence {
        root: Option<PathBuf>,
        action: HostedEvidenceAction,
        input_json: PathBuf,
        evaluated_at_unix_seconds: i64,
        expected_subject: String,
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
    },
    D1Repository {
        root: Option<PathBuf>,
    },
    ReleaseFinalize {
        root: Option<PathBuf>,
        request_json: PathBuf,
    },
    Release {
        root: Option<PathBuf>,
        action: release::ReleaseAction,
        release_set: PathBuf,
        source_root: Option<PathBuf>,
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
        source_root: Option<PathBuf>,
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

    if command == "credentials" {
        return parse_credentials_invocation(root, iterator);
    }

    match command.as_str() {
        "status" => parse_status_invocation(root, iterator),
        "hosted-evidence" => parse_hosted_evidence_invocation(root, iterator),
        "d1" => parse_d1_invocation(root, iterator),
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

fn parse_hosted_evidence_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let consumer = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("hosted-evidence", "missing hosted evidence consumer"))?
        .into_string()
        .map_err(|_| OpsctlError::new("hosted-evidence", "consumer must be valid UTF-8"))?;
    if consumer != "operational-credential" {
        return Err(OpsctlError::new(
            "hosted-evidence",
            format!("unsupported hosted evidence consumer: {consumer}"),
        ));
    }

    let action_value = iterator
        .next()
        .ok_or_else(|| OpsctlError::new("hosted-evidence", "missing hosted evidence action"))?;
    let action_text = action_value
        .to_str()
        .ok_or_else(|| OpsctlError::new("hosted-evidence", "action must be valid UTF-8"))?;
    let action = match action_text {
        "seal" => HostedEvidenceAction::SealOperationalCredential,
        "verify" => HostedEvidenceAction::VerifyOperationalCredential,
        other => {
            return Err(OpsctlError::new(
                "hosted-evidence",
                format!("unsupported hosted evidence action: {other}"),
            ));
        }
    };

    let mut observation_json: Option<PathBuf> = None;
    let mut artifact_json: Option<PathBuf> = None;
    let mut evaluated_at_unix_seconds: Option<i64> = None;
    let mut expected_subject: Option<String> = None;

    while let Some(argument) = iterator.next() {
        let flag = argument.to_str().ok_or_else(|| {
            OpsctlError::new(
                "hosted-evidence",
                "hosted evidence flags must be valid UTF-8",
            )
        })?;
        match flag {
            "--observation-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("hosted-evidence", "--observation-json requires a path")
                })?;
                set_once(
                    &mut observation_json,
                    PathBuf::from(value),
                    "--observation-json",
                )?;
            }
            "--artifact-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("hosted-evidence", "--artifact-json requires a path")
                })?;
                set_once(&mut artifact_json, PathBuf::from(value), "--artifact-json")?;
            }
            "--evaluated-at-unix-seconds" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| {
                        OpsctlError::new(
                            "hosted-evidence",
                            "--evaluated-at-unix-seconds requires a value",
                        )
                    })?
                    .into_string()
                    .map_err(|_| {
                        OpsctlError::new(
                            "hosted-evidence",
                            "evaluation timestamp must be valid UTF-8",
                        )
                    })?;
                let value = value.parse::<i64>().map_err(|_| {
                    OpsctlError::new(
                        "hosted-evidence",
                        "--evaluated-at-unix-seconds must be a signed integer",
                    )
                })?;
                set_once(
                    &mut evaluated_at_unix_seconds,
                    value,
                    "--evaluated-at-unix-seconds",
                )?;
            }
            "--expected-subject" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| {
                        OpsctlError::new("hosted-evidence", "--expected-subject requires a value")
                    })?
                    .into_string()
                    .map_err(|_| {
                        OpsctlError::new("hosted-evidence", "expected subject must be valid UTF-8")
                    })?;
                set_once(&mut expected_subject, value, "--expected-subject")?;
            }
            other => {
                return Err(OpsctlError::new(
                    "hosted-evidence",
                    format!("unsupported hosted evidence argument: {other}"),
                ));
            }
        }
    }

    let input_json = match action {
        HostedEvidenceAction::SealOperationalCredential => {
            if artifact_json.is_some() {
                return Err(OpsctlError::new(
                    "hosted-evidence",
                    "hosted-evidence operational-credential seal accepts --observation-json only",
                ));
            }
            observation_json.ok_or_else(|| {
                OpsctlError::new(
                    "hosted-evidence",
                    "hosted-evidence operational-credential seal requires --observation-json",
                )
            })?
        }
        HostedEvidenceAction::VerifyOperationalCredential => {
            if observation_json.is_some() {
                return Err(OpsctlError::new(
                    "hosted-evidence",
                    "hosted-evidence operational-credential verify accepts --artifact-json only",
                ));
            }
            artifact_json.ok_or_else(|| {
                OpsctlError::new(
                    "hosted-evidence",
                    "hosted-evidence operational-credential verify requires --artifact-json",
                )
            })?
        }
    };
    let evaluated_at_unix_seconds = evaluated_at_unix_seconds.ok_or_else(|| {
        OpsctlError::new(
            "hosted-evidence",
            "hosted evidence requires --evaluated-at-unix-seconds from the outer clock observation",
        )
    })?;
    let expected_subject = expected_subject.ok_or_else(|| {
        OpsctlError::new(
            "hosted-evidence",
            "hosted evidence requires --expected-subject from the exact accepted-source checkout",
        )
    })?;

    Ok(Invocation::HostedEvidence {
        root,
        action,
        input_json,
        evaluated_at_unix_seconds,
        expected_subject,
    })
}

fn parse_status_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let mut acceptance_evidence_json: Option<PathBuf> = None;
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("status", "status flags must be valid UTF-8"))?;
        match flag {
            "--acceptance-evidence-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("status", "--acceptance-evidence-json requires a path")
                })?;
                set_once(
                    &mut acceptance_evidence_json,
                    PathBuf::from(value),
                    "--acceptance-evidence-json",
                )?;
            }
            other => {
                return Err(OpsctlError::new(
                    "status",
                    format!("unsupported status argument: {other}"),
                ));
            }
        }
    }
    let acceptance_evidence_json = acceptance_evidence_json.ok_or_else(|| {
        OpsctlError::new(
            "status",
            "status requires --acceptance-evidence-json from the outer observation shell",
        )
    })?;
    Ok(Invocation::Status {
        root,
        acceptance_evidence_json,
    })
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
    if action_text == "repository" {
        if let Some(extra) = iterator.next() {
            return Err(OpsctlError::new(
                "d1",
                format!(
                    "unexpected d1 repository argument: {}",
                    extra.to_string_lossy()
                ),
            ));
        }
        return Ok(Invocation::D1Repository { root });
    }
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
            "d1 status accepts only component and ledger inputs",
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
    if action_text == "finalize" {
        return parse_release_finalize_invocation(root, iterator);
    }
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
    let mut source_root: Option<PathBuf> = None;
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
            "--source-root" => {
                let value = iterator
                    .next()
                    .ok_or_else(|| OpsctlError::new("release", "--source-root requires a path"))?;
                set_once(&mut source_root, PathBuf::from(value), "--source-root")?;
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
            reject_if_present(&source_root, "--source-root", action)?;
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
        source_root,
        artifact_root,
        profile_id,
        environment,
        evidence_json,
        current_release_set,
    })
}

fn parse_release_finalize_invocation<I>(
    root: Option<PathBuf>,
    mut iterator: I,
) -> Result<Invocation, OpsctlError>
where
    I: Iterator<Item = OsString>,
{
    let mut request_json: Option<PathBuf> = None;
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| OpsctlError::new("release", "release flags must be valid UTF-8"))?;
        match flag {
            "--request-json" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("release", "--request-json requires a value")
                })?;
                set_once(&mut request_json, PathBuf::from(value), "--request-json")?;
            }
            other => {
                return Err(OpsctlError::new(
                    "release",
                    format!("unsupported release finalize argument: {other}"),
                ));
            }
        }
    }
    let request_json = request_json
        .ok_or_else(|| OpsctlError::new("release", "release finalize requires --request-json"))?;
    Ok(Invocation::ReleaseFinalize { root, request_json })
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
    let mut source_root: Option<PathBuf> = None;
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
            "--source-root" => {
                let value = iterator.next().ok_or_else(|| {
                    OpsctlError::new("promotion", "--source-root requires a path")
                })?;
                set_once(&mut source_root, PathBuf::from(value), "--source-root")?;
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
        source_root,
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
    use super::{CredentialsAction, Invocation, ReadCommand, parse_invocation};
    use crate::d1::D1Action;
    use crate::hosted_evidence::HostedEvidenceAction;
    use crate::release::ReleaseAction;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().copied().map(OsString::from).collect()
    }

    #[test]
    fn parses_doctor_surface() {
        assert_eq!(
            parse_invocation(args(&["opsctl", "doctor"])),
            Ok(Invocation::Run {
                root: None,
                command: ReadCommand::Doctor,
            })
        );
    }

    #[test]
    fn parses_status_with_explicit_acceptance_evidence() {
        assert_eq!(
            parse_invocation(args(&[
                "opsctl",
                "--root",
                "/repo",
                "status",
                "--acceptance-evidence-json",
                "evidence.json",
            ])),
            Ok(Invocation::Status {
                root: Some(PathBuf::from("/repo")),
                acceptance_evidence_json: PathBuf::from("evidence.json"),
            })
        );
    }

    #[test]
    fn status_rejects_implicit_or_hidden_lifecycle_input() {
        assert!(parse_invocation(args(&["opsctl", "status"])).is_err());
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "status",
                "--lifecycle-json",
                "legacy.json",
            ]))
            .is_err()
        );
    }

    #[test]
    fn retired_tracked_inventory_surface_is_rejected() {
        assert!(parse_invocation(args(&["opsctl", "inventory"])).is_err());
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
    fn parses_bounded_operational_credential_hosted_evidence_surface() {
        assert_eq!(
            parse_invocation(args(&[
                "opsctl",
                "--root",
                "/repo",
                "hosted-evidence",
                "operational-credential",
                "seal",
                "--observation-json",
                "observation.json",
                "--evaluated-at-unix-seconds",
                "1700000010",
                "--expected-subject",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ])),
            Ok(Invocation::HostedEvidence {
                root: Some(PathBuf::from("/repo")),
                action: HostedEvidenceAction::SealOperationalCredential,
                input_json: PathBuf::from("observation.json"),
                evaluated_at_unix_seconds: 1_700_000_010,
                expected_subject: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            })
        );
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "hosted-evidence",
                "generic-provider",
                "seal",
            ]))
            .is_err()
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
            })
        );
    }

    #[test]
    fn parses_repository_derived_d1_projection_without_observation_inputs() {
        assert_eq!(
            parse_invocation(args(&["opsctl", "--root", "/repo", "d1", "repository"])),
            Ok(Invocation::D1Repository {
                root: Some(PathBuf::from("/repo")),
            })
        );
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "d1",
                "repository",
                "--component",
                "catalog",
            ]))
            .is_err()
        );
    }

    #[test]
    fn d1_authority_override_is_rejected() {
        let removed_flag = ["--", "authority"].concat();
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "d1",
                "status",
                "--component",
                "catalog",
                "--ledger-json",
                "ledger.json",
                removed_flag.as_str(),
                "legacy.json",
            ]))
            .is_err()
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
    fn d1_plan_requires_release_manifest() {
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "d1",
                "plan",
                "--component",
                "catalog",
                "--ledger-json",
                "ledger.json",
            ]))
            .is_err()
        );
    }

    #[test]
    fn activates_release_finalize_without_release_set_override() {
        assert_eq!(
            parse_invocation(args(&[
                "opsctl",
                "--root",
                "/repo",
                "release",
                "finalize",
                "--request-json",
                "request.json",
            ])),
            Ok(Invocation::ReleaseFinalize {
                root: Some(PathBuf::from("/repo")),
                request_json: PathBuf::from("request.json"),
            })
        );
        assert!(
            parse_invocation(args(&[
                "opsctl",
                "release",
                "finalize",
                "--request-json",
                "request.json",
                "--release-set",
                "override.json",
            ]))
            .is_err()
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
                source_root: None,
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
    fn release_compatibility_accepts_distinct_source_root() {
        let invocation = parse_invocation(args(&[
            "opsctl",
            "--root",
            "/policy",
            "release",
            "compatibility",
            "--release-set",
            "release-set.json",
            "--source-root",
            "/historical-source",
            "--profile",
            "rehearsal-core-v1",
            "--environment",
            "staging",
            "--evidence-json",
            "evidence.json",
        ]));
        assert!(matches!(
            invocation,
            Ok(Invocation::Release {
                root: Some(policy),
                source_root: Some(source),
                ..
            }) if policy.as_path() == std::path::Path::new("/policy")
                && source.as_path() == std::path::Path::new("/historical-source")
        ));
    }

    #[test]
    fn promotion_namespace_remains_fail_closed_until_activated() {
        let invocation = parse_invocation(args(&["opsctl", "promotion", "plan"]));
        assert!(invocation.is_err());
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
