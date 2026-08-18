use serde_json::{Value, json};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_AUTHORITY: &str = "architecture/d1-evolution-ar9.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D1Action {
    Status,
    Plan,
    Compatibility,
    Verify,
}

impl D1Action {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Plan => "plan",
            Self::Compatibility => "compatibility",
            Self::Verify => "verify",
        }
    }

    #[must_use]
    pub const fn requires_release_manifest(self) -> bool {
        !matches!(self, Self::Status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LedgerState {
    Exact,
    BehindKnownPrefix,
    AheadKnownCompatible,
    AheadKnownIncompatible,
    Diverged,
    UnknownMigration,
    CorruptLedger,
}

impl LedgerState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::BehindKnownPrefix => "BEHIND_KNOWN_PREFIX",
            Self::AheadKnownCompatible => "AHEAD_KNOWN_COMPATIBLE",
            Self::AheadKnownIncompatible => "AHEAD_KNOWN_INCOMPATIBLE",
            Self::Diverged => "DIVERGED",
            Self::UnknownMigration => "UNKNOWN_MIGRATION",
            Self::CorruptLedger => "CORRUPT_LEDGER",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Safe,
    MigrationRequired,
    CodeRollbackSafe,
    CodeRollbackBlocked,
    RecoveryRequired,
}

impl Decision {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::MigrationRequired => "MIGRATION_REQUIRED",
            Self::CodeRollbackSafe => "CODE_ROLLBACK_SAFE",
            Self::CodeRollbackBlocked => "CODE_ROLLBACK_BLOCKED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct D1Error {
    message: String,
}

impl D1Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for D1Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for D1Error {}

#[derive(Debug, Clone)]
struct ComponentAuthority {
    component_id: String,
    ordered_history: Vec<String>,
    current_repository_revision: String,
    ordered_set_identity: String,
}

#[derive(Debug, Clone)]
struct ReleaseSchemaContract {
    database_component: String,
    target_schema_revision: String,
    supported_schema_min: String,
    supported_schema_max: String,
    migration_history_digest: Option<String>,
    compatibility_policy_digest: Option<String>,
}

#[derive(Debug, Clone)]
struct Evaluation {
    ledger_state: LedgerState,
    decision: Decision,
    remote_revision: Option<String>,
    target_revision: String,
    planned_migrations: Vec<String>,
    allowed: bool,
}

pub fn run(
    root: &Path,
    action: D1Action,
    component: &str,
    ledger_json: &Path,
    release_manifest: Option<&Path>,
    authority_path: Option<&Path>,
) -> Result<String, D1Error> {
    let authority = load_component_authority(
        root,
        authority_path.unwrap_or_else(|| Path::new(DEFAULT_AUTHORITY)),
        component,
    )?;
    let remote_names = load_wrangler_ledger(ledger_json)?;
    let contract = match release_manifest {
        Some(path) => Some(load_release_contract(path, component)?),
        None if action.requires_release_manifest() => {
            return Err(D1Error::new(format!(
                "d1 {} requires --release-manifest",
                action.name()
            )));
        }
        None => None,
    };

    let evaluation = evaluate(action, &authority, &remote_names, contract.as_ref())?;
    let output = json!({
        "schema_version": 1,
        "command": format!("d1 {}", action.name()),
        "status": if evaluation.allowed { "ok" } else { "blocked" },
        "mode": "read-only",
        "mutation_executed": false,
        "component": authority.component_id,
        "ledger_state": evaluation.ledger_state.as_str(),
        "decision": evaluation.decision.as_str(),
        "remote_revision": evaluation.remote_revision,
        "target_revision": evaluation.target_revision,
        "current_repository_revision": authority.current_repository_revision,
        "history_identity": authority.ordered_set_identity,
        "planned_migrations": evaluation.planned_migrations,
        "allowed": evaluation.allowed
    });
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize d1 result: {error}")))
}

fn load_component_authority(
    root: &Path,
    authority_path: &Path,
    component: &str,
) -> Result<ComponentAuthority, D1Error> {
    let path = resolve_input(root, authority_path);
    let document = read_json(&path, "D1 evolution authority")?;
    let object = document
        .as_object()
        .ok_or_else(|| D1Error::new("D1 evolution authority must be one JSON object"))?;
    if object.get("kind").and_then(Value::as_str) != Some("D1_EVOLUTION_AUTHORITY") {
        return Err(D1Error::new("D1 evolution authority kind is invalid"));
    }
    let components = object
        .get("components")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("D1 evolution authority components are missing"))?;
    let selected = components
        .iter()
        .find(|entry| entry.get("component_id").and_then(Value::as_str) == Some(component))
        .ok_or_else(|| D1Error::new(format!("unknown D1 component: {component}")))?;

    let historical = selected
        .get("historical_epoch")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("component historical_epoch is missing"))?;
    let ordered = historical
        .get("ordered_history")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("component ordered_history is missing"))?;
    let mut ordered_history = Vec::with_capacity(ordered.len());
    for entry in ordered {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| D1Error::new("historical migration name is missing"))?;
        ordered_history.push(name.to_owned());
    }
    if ordered_history.is_empty() {
        return Err(D1Error::new("component ordered_history must not be empty"));
    }
    ensure_unique(&ordered_history, "canonical migration history")?;

    let current_repository_revision = selected
        .get("current_repository_revision")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("current_repository_revision is missing"))?
        .to_owned();
    if ordered_history.last() != Some(&current_repository_revision) {
        return Err(D1Error::new(
            "current_repository_revision must equal the final canonical migration",
        ));
    }
    let ordered_set_identity = historical
        .get("ordered_set_identity")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("historical ordered_set_identity is missing"))?
        .to_owned();

    Ok(ComponentAuthority {
        component_id: component.to_owned(),
        ordered_history,
        current_repository_revision,
        ordered_set_identity,
    })
}

fn load_wrangler_ledger(path: &Path) -> Result<Vec<String>, D1Error> {
    let document = read_json(path, "Wrangler D1 ledger JSON")?;
    if let Some(rows) = document.get("rows").and_then(Value::as_array) {
        return ledger_names(rows);
    }

    let results = document
        .as_array()
        .filter(|items| items.len() == 1)
        .and_then(|items| items.first())
        .and_then(Value::as_object)
        .ok_or_else(|| {
            D1Error::new(
                "Wrangler D1 ledger JSON must be a one-result execute --json array or a fixture object",
            )
        })?;
    if results.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(D1Error::new("Wrangler D1 ledger query did not succeed"));
    }
    let rows = results
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("Wrangler D1 ledger results are missing"))?;
    ledger_names(rows)
}

fn ledger_names(rows: &[Value]) -> Result<Vec<String>, D1Error> {
    let mut names = Vec::with_capacity(rows.len());
    let mut last_id: Option<i64> = None;
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| D1Error::new("D1 ledger row must be an object"))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| D1Error::new("D1 ledger row is missing migration name"))?
            .to_owned();
        if let Some(id) = object.get("id").and_then(Value::as_i64) {
            if let Some(previous) = last_id {
                if id <= previous {
                    return Err(D1Error::new(
                        "D1 ledger ids must be strictly increasing in query order",
                    ));
                }
            }
            last_id = Some(id);
        }
        names.push(name);
    }
    Ok(names)
}

fn load_release_contract(path: &Path, component: &str) -> Result<ReleaseSchemaContract, D1Error> {
    let document = read_json(path, "release manifest")?;
    let contract = document
        .get("schema_contract")
        .or_else(|| document.get("d1_schema"))
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("release manifest schema_contract is missing"))?;

    let database_component = required_string(contract, "database_component")?;
    if database_component != component {
        return Err(D1Error::new(format!(
            "release schema component {database_component:?} does not match requested component {component:?}"
        )));
    }
    Ok(ReleaseSchemaContract {
        database_component,
        target_schema_revision: required_string(contract, "target_schema_revision")?,
        supported_schema_min: required_string(contract, "supported_schema_min")?,
        supported_schema_max: required_string(contract, "supported_schema_max")?,
        migration_history_digest: optional_string(contract, "migration_history_digest")?,
        compatibility_policy_digest: optional_string(contract, "compatibility_policy_digest")?,
    })
}

fn evaluate(
    action: D1Action,
    authority: &ComponentAuthority,
    remote_names: &[String],
    contract: Option<&ReleaseSchemaContract>,
) -> Result<Evaluation, D1Error> {
    let base_state = classify_prefix(remote_names, &authority.ordered_history);
    if matches!(
        base_state,
        LedgerState::Diverged | LedgerState::UnknownMigration | LedgerState::CorruptLedger
    ) {
        return Ok(Evaluation {
            ledger_state: base_state,
            decision: Decision::RecoveryRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: contract
                .map(|value| value.target_schema_revision.clone())
                .unwrap_or_else(|| authority.current_repository_revision.clone()),
            planned_migrations: Vec::new(),
            allowed: false,
        });
    }

    let Some(contract) = contract else {
        let state = if remote_names.len() == authority.ordered_history.len() {
            LedgerState::Exact
        } else {
            LedgerState::BehindKnownPrefix
        };
        return Ok(Evaluation {
            ledger_state: state,
            decision: if state == LedgerState::Exact {
                Decision::Safe
            } else {
                Decision::MigrationRequired
            },
            remote_revision: remote_names.last().cloned(),
            target_revision: authority.current_repository_revision.clone(),
            planned_migrations: if state == LedgerState::BehindKnownPrefix {
                authority.ordered_history[remote_names.len()..].to_vec()
            } else {
                Vec::new()
            },
            allowed: true,
        });
    };

    if contract.database_component != authority.component_id {
        return Err(D1Error::new("release schema contract component mismatch"));
    }
    if let Some(digest) = &contract.migration_history_digest {
        if digest != &authority.ordered_set_identity {
            return Ok(Evaluation {
                ledger_state: LedgerState::CorruptLedger,
                decision: Decision::RecoveryRequired,
                remote_revision: remote_names.last().cloned(),
                target_revision: contract.target_schema_revision.clone(),
                planned_migrations: Vec::new(),
                allowed: false,
            });
        }
    }
    if let Some(policy_digest) = &contract.compatibility_policy_digest {
        if policy_digest.is_empty() {
            return Err(D1Error::new(
                "compatibility_policy_digest must not be an empty string",
            ));
        }
    }

    let target = revision_index(&authority.ordered_history, &contract.target_schema_revision)?;
    let minimum = revision_index(&authority.ordered_history, &contract.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &contract.supported_schema_max)?;
    if minimum > target || target > maximum {
        return Err(D1Error::new(
            "release schema window must satisfy supported_min <= target <= supported_max",
        ));
    }

    let remote_count = remote_names.len();
    let target_count = target + 1;
    let state = if remote_count == target_count {
        LedgerState::Exact
    } else if remote_count < target_count {
        LedgerState::BehindKnownPrefix
    } else {
        let remote_index = remote_count - 1;
        if remote_index >= minimum && remote_index <= maximum {
            LedgerState::AheadKnownCompatible
        } else {
            LedgerState::AheadKnownIncompatible
        }
    };

    let (decision, allowed) = match (action, state) {
        (_, LedgerState::Exact) => (Decision::Safe, true),
        (D1Action::Status, LedgerState::BehindKnownPrefix) => (Decision::MigrationRequired, true),
        (D1Action::Plan, LedgerState::BehindKnownPrefix) => (Decision::MigrationRequired, true),
        (D1Action::Compatibility, LedgerState::BehindKnownPrefix) => {
            (Decision::MigrationRequired, true)
        }
        (D1Action::Verify, LedgerState::BehindKnownPrefix) => {
            (Decision::MigrationRequired, false)
        }
        (D1Action::Compatibility, LedgerState::AheadKnownCompatible) => {
            (Decision::CodeRollbackSafe, true)
        }
        (D1Action::Verify, LedgerState::AheadKnownCompatible) => (Decision::Safe, true),
        (_, LedgerState::AheadKnownCompatible) => (Decision::Safe, true),
        (_, LedgerState::AheadKnownIncompatible) => (Decision::CodeRollbackBlocked, false),
        (_, LedgerState::Diverged | LedgerState::UnknownMigration | LedgerState::CorruptLedger) => {
            (Decision::RecoveryRequired, false)
        }
    };

    let planned_migrations = if remote_count < target_count {
        authority.ordered_history[remote_count..target_count].to_vec()
    } else {
        Vec::new()
    };

    Ok(Evaluation {
        ledger_state: state,
        decision,
        remote_revision: remote_names.last().cloned(),
        target_revision: contract.target_schema_revision.clone(),
        planned_migrations,
        allowed,
    })
}

fn classify_prefix(remote: &[String], canonical: &[String]) -> LedgerState {
    if ensure_unique(remote, "remote migration ledger").is_err() {
        return LedgerState::CorruptLedger;
    }
    let known: HashSet<&str> = canonical.iter().map(String::as_str).collect();
    if remote.iter().any(|name| !known.contains(name.as_str())) {
        return LedgerState::UnknownMigration;
    }
    if remote.len() > canonical.len() {
        return LedgerState::UnknownMigration;
    }
    if remote
        .iter()
        .zip(canonical.iter())
        .any(|(observed, expected)| observed != expected)
    {
        return LedgerState::Diverged;
    }
    if remote.len() == canonical.len() {
        LedgerState::Exact
    } else {
        LedgerState::BehindKnownPrefix
    }
}

fn revision_index(history: &[String], revision: &str) -> Result<usize, D1Error> {
    history
        .iter()
        .position(|candidate| candidate == revision)
        .ok_or_else(|| D1Error::new(format!("unknown schema revision in release contract: {revision}")))
}

fn ensure_unique(values: &[String], label: &str) -> Result<(), D1Error> {
    let mut observed = HashSet::with_capacity(values.len());
    for value in values {
        if !observed.insert(value.as_str()) {
            return Err(D1Error::new(format!("{label} contains duplicate entry: {value}")));
        }
    }
    Ok(())
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, D1Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("release schema contract {key} is missing")))
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, D1Error> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(D1Error::new(format!(
            "release schema contract {key} must be a non-empty string when present"
        ))),
    }
}

fn resolve_input(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_json(path: &Path, label: &str) -> Result<Value, D1Error> {
    let text = fs::read_to_string(path)
        .map_err(|error| D1Error::new(format!("cannot read {label} {}: {error}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|error| D1Error::new(format!("cannot parse {label} {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::{ComponentAuthority, D1Action, D1Error, LedgerState, ReleaseSchemaContract, classify_prefix, evaluate};

    fn authority() -> ComponentAuthority {
        ComponentAuthority {
            component_id: "catalog".to_owned(),
            ordered_history: vec![
                "0001_a.sql".to_owned(),
                "0002_b.sql".to_owned(),
                "0003_c.sql".to_owned(),
            ],
            current_repository_revision: "0003_c.sql".to_owned(),
            ordered_set_identity: "history-digest".to_owned(),
        }
    }

    fn contract(maximum: &str) -> ReleaseSchemaContract {
        ReleaseSchemaContract {
            database_component: "catalog".to_owned(),
            target_schema_revision: "0002_b.sql".to_owned(),
            supported_schema_min: "0001_a.sql".to_owned(),
            supported_schema_max: maximum.to_owned(),
            migration_history_digest: Some("history-digest".to_owned()),
            compatibility_policy_digest: Some("policy-digest".to_owned()),
        }
    }

    #[test]
    fn classifies_exact_prefix_and_divergence() {
        let canonical = authority().ordered_history;
        assert_eq!(classify_prefix(&canonical, &canonical), LedgerState::Exact);
        assert_eq!(
            classify_prefix(&canonical[..2], &canonical),
            LedgerState::BehindKnownPrefix
        );
        assert_eq!(
            classify_prefix(
                &["0001_a.sql".to_owned(), "0003_c.sql".to_owned()],
                &canonical
            ),
            LedgerState::Diverged
        );
        assert_eq!(
            classify_prefix(
                &["0001_a.sql".to_owned(), "9999_unknown.sql".to_owned()],
                &canonical
            ),
            LedgerState::UnknownMigration
        );
        assert_eq!(
            classify_prefix(
                &["0001_a.sql".to_owned(), "0001_a.sql".to_owned()],
                &canonical
            ),
            LedgerState::CorruptLedger
        );
    }

    #[test]
    fn known_ahead_schema_distinguishes_rollback_window() -> Result<(), D1Error> {
        let remote = authority().ordered_history;
        let compatible = evaluate(
            D1Action::Compatibility,
            &authority(),
            &remote,
            Some(&contract("0003_c.sql")),
        )?;
        assert_eq!(compatible.ledger_state, LedgerState::AheadKnownCompatible);
        assert!(compatible.allowed);

        let incompatible = evaluate(
            D1Action::Compatibility,
            &authority(),
            &remote,
            Some(&contract("0002_b.sql")),
        )?;
        assert_eq!(incompatible.ledger_state, LedgerState::AheadKnownIncompatible);
        assert!(!incompatible.allowed);
        Ok(())
    }

    #[test]
    fn verify_blocks_known_shorter_prefix() -> Result<(), D1Error> {
        let remote = vec!["0001_a.sql".to_owned()];
        let result = evaluate(
            D1Action::Verify,
            &authority(),
            &remote,
            Some(&contract("0003_c.sql")),
        )?;
        assert_eq!(result.ledger_state, LedgerState::BehindKnownPrefix);
        assert!(!result.allowed);
        assert_eq!(result.planned_migrations, vec!["0002_b.sql", "0003_c.sql"]);
        Ok(())
    }
}
