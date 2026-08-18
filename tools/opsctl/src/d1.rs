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
    DeployFirst,
    MigrateFirst,
    CodeRollbackSafe,
    CodeRollbackBlocked,
    FailForwardRequired,
    ContractBlocked,
    RecoveryRequired,
}

impl Decision {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::MigrationRequired => "MIGRATION_REQUIRED",
            Self::DeployFirst => "DEPLOY_FIRST",
            Self::MigrateFirst => "MIGRATE_FIRST",
            Self::CodeRollbackSafe => "CODE_ROLLBACK_SAFE",
            Self::CodeRollbackBlocked => "CODE_ROLLBACK_BLOCKED",
            Self::FailForwardRequired => "FAIL_FORWARD_REQUIRED",
            Self::ContractBlocked => "CONTRACT_BLOCKED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MigrationClass {
    Expand,
    Backfill,
    Contract,
    Repair,
}

impl MigrationClass {
    fn parse(value: &str) -> Result<Self, D1Error> {
        match value {
            "EXPAND" => Ok(Self::Expand),
            "BACKFILL" => Ok(Self::Backfill),
            "CONTRACT" => Ok(Self::Contract),
            "REPAIR" => Ok(Self::Repair),
            other => Err(D1Error::new(format!("unknown migration class: {other}"))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Expand => "EXPAND",
            Self::Backfill => "BACKFILL",
            Self::Contract => "CONTRACT",
            Self::Repair => "REPAIR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RolloutOrder {
    MigrateBeforeCode,
    CodeBeforeMigrate,
    Either,
    SeparateContractRelease,
}

impl RolloutOrder {
    fn parse(value: &str) -> Result<Self, D1Error> {
        match value {
            "MIGRATE_BEFORE_CODE" => Ok(Self::MigrateBeforeCode),
            "CODE_BEFORE_MIGRATE" => Ok(Self::CodeBeforeMigrate),
            "EITHER" => Ok(Self::Either),
            "SEPARATE_CONTRACT_RELEASE" => Ok(Self::SeparateContractRelease),
            other => Err(D1Error::new(format!("unknown rollout order: {other}"))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::MigrateBeforeCode => "MIGRATE_BEFORE_CODE",
            Self::CodeBeforeMigrate => "CODE_BEFORE_MIGRATE",
            Self::Either => "EITHER",
            Self::SeparateContractRelease => "SEPARATE_CONTRACT_RELEASE",
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
struct MigrationContract {
    migration_file: String,
    migration_class: MigrationClass,
    rollout_order: RolloutOrder,
    fail_forward_required: bool,
    destructive: bool,
    code_rollback_allowed: bool,
    contract_preconditions: Vec<String>,
}

#[derive(Debug, Clone)]
struct ComponentAuthority {
    component_id: String,
    historical_len: usize,
    ordered_history: Vec<String>,
    post_epoch: Vec<MigrationContract>,
    current_repository_revision: String,
    history_digest: String,
}

#[derive(Debug, Clone)]
struct ReleaseSchemaContract {
    database_component: String,
    target_schema_revision: String,
    supported_schema_min: String,
    supported_schema_max: String,
    migration_history_digest: String,
    compatibility_policy_digest: String,
}

#[derive(Debug, Clone, Default)]
struct Preconditions {
    completed: HashSet<String>,
}

#[derive(Debug, Clone)]
struct Evaluation {
    ledger_state: LedgerState,
    decision: Decision,
    remote_revision: Option<String>,
    target_revision: String,
    planned_migrations: Vec<String>,
    planned_contracts: Vec<Value>,
    reason_codes: Vec<String>,
    rollback_context_complete: bool,
    allowed: bool,
}

pub struct D1RunRequest<'a> {
    pub root: &'a Path,
    pub action: D1Action,
    pub component: &'a str,
    pub ledger_json: &'a Path,
    pub release_manifest: Option<&'a Path>,
    pub current_manifest: Option<&'a Path>,
    pub known_good_manifest: Option<&'a Path>,
    pub preconditions_json: Option<&'a Path>,
    pub authority_path: Option<&'a Path>,
}

pub fn run(request: D1RunRequest<'_>) -> Result<String, D1Error> {
    let authority = load_component_authority(
        request.root,
        request
            .authority_path
            .unwrap_or_else(|| Path::new(DEFAULT_AUTHORITY)),
        request.component,
    )?;
    let ledger_path = resolve_input(request.root, request.ledger_json);
    let remote_names = load_wrangler_ledger(&ledger_path)?;
    let target = load_optional_release(
        request.root,
        request.release_manifest,
        request.component,
        request.action.requires_release_manifest(),
        request.action.name(),
    )?;
    let current = load_optional_release(
        request.root,
        request.current_manifest,
        request.component,
        false,
        request.action.name(),
    )?;
    let known_good = load_optional_release(
        request.root,
        request.known_good_manifest,
        request.component,
        false,
        request.action.name(),
    )?;
    let preconditions = match request.preconditions_json {
        Some(path) => load_preconditions(&resolve_input(request.root, path), request.component)?,
        None => Preconditions::default(),
    };

    let evaluation = evaluate(
        request.action,
        &authority,
        &remote_names,
        target.as_ref(),
        current.as_ref(),
        known_good.as_ref(),
        &preconditions,
    )?;
    let output = json!({
        "schema_version": 1,
        "command": format!("d1 {}", request.action.name()),
        "status": if evaluation.allowed { "ok" } else { "blocked" },
        "mode": "read-only",
        "mutation_executed": false,
        "component": authority.component_id,
        "ledger_state": evaluation.ledger_state.as_str(),
        "decision": evaluation.decision.as_str(),
        "remote_revision": evaluation.remote_revision,
        "target_revision": evaluation.target_revision,
        "current_repository_revision": authority.current_repository_revision,
        "history_digest": authority.history_digest,
        "planned_migrations": evaluation.planned_migrations,
        "planned_migration_contracts": evaluation.planned_contracts,
        "rollback_context_complete": evaluation.rollback_context_complete,
        "reason_codes": evaluation.reason_codes,
        "allowed": evaluation.allowed
    });
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize d1 result: {error}")))
}

fn load_optional_release(
    root: &Path,
    path: Option<&Path>,
    component: &str,
    required: bool,
    action: &str,
) -> Result<Option<ReleaseSchemaContract>, D1Error> {
    match path {
        Some(value) => {
            let resolved = resolve_input(root, value);
            Ok(Some(load_release_contract(&resolved, component)?))
        }
        None if required => Err(D1Error::new(format!(
            "d1 {action} requires --release-manifest"
        ))),
        None => Ok(None),
    }
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
        ordered_history.push(required_value_string(
            entry,
            "name",
            "historical migration",
        )?);
    }
    if ordered_history.is_empty() {
        return Err(D1Error::new("component ordered_history must not be empty"));
    }
    let historical_len = ordered_history.len();

    let post_epoch_values = selected
        .get("post_epoch_migrations")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("component post_epoch_migrations is missing"))?;
    let mut post_epoch = Vec::with_capacity(post_epoch_values.len());
    for value in post_epoch_values {
        let contract = parse_migration_contract(value, component)?;
        ordered_history.push(contract.migration_file.clone());
        post_epoch.push(contract);
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
    let history_digest = selected
        .get("history_digest")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("component history_digest is missing"))?
        .to_owned();

    Ok(ComponentAuthority {
        component_id: component.to_owned(),
        historical_len,
        ordered_history,
        post_epoch,
        current_repository_revision,
        history_digest,
    })
}

fn parse_migration_contract(value: &Value, component: &str) -> Result<MigrationContract, D1Error> {
    let object = value
        .as_object()
        .ok_or_else(|| D1Error::new("post-epoch migration contract must be an object"))?;
    if required_string(object, "component")? != component {
        return Err(D1Error::new("post-epoch migration component mismatch"));
    }
    let migration_file = required_string(object, "migration_file")?;
    let revision = required_string(object, "migration_revision")?;
    if revision != migration_file {
        return Err(D1Error::new(
            "migration_revision must equal the canonical migration filename",
        ));
    }
    let migration_class = MigrationClass::parse(&required_string(object, "migration_class")?)?;
    let rollout_order = RolloutOrder::parse(&required_string(object, "rollout_order")?)?;
    let fail_forward_required = required_bool(object, "fail_forward_required")?;
    let destructive = required_bool(object, "destructive")?;
    let code_rollback_allowed = required_bool(object, "code_rollback_allowed")?;
    let contract_preconditions = required_string_array(object, "contract_preconditions")?;
    if destructive && code_rollback_allowed {
        return Err(D1Error::new(
            "destructive migration cannot claim code rollback safety",
        ));
    }
    if migration_class == MigrationClass::Contract
        && rollout_order != RolloutOrder::SeparateContractRelease
    {
        return Err(D1Error::new(
            "CONTRACT migration must use SEPARATE_CONTRACT_RELEASE",
        ));
    }
    Ok(MigrationContract {
        migration_file,
        migration_class,
        rollout_order,
        fail_forward_required,
        destructive,
        code_rollback_allowed,
        contract_preconditions,
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
        migration_history_digest: required_string(contract, "migration_history_digest")?,
        compatibility_policy_digest: required_string(contract, "compatibility_policy_digest")?,
    })
}

fn load_preconditions(path: &Path, component: &str) -> Result<Preconditions, D1Error> {
    let document = read_json(path, "D1 contract preconditions")?;
    let object = document
        .as_object()
        .ok_or_else(|| D1Error::new("D1 contract preconditions must be an object"))?;
    if required_string(object, "component")? != component {
        return Err(D1Error::new("D1 contract precondition component mismatch"));
    }
    Ok(Preconditions {
        completed: required_string_array(object, "completed")?
            .into_iter()
            .collect(),
    })
}

fn evaluate(
    action: D1Action,
    authority: &ComponentAuthority,
    remote_names: &[String],
    target: Option<&ReleaseSchemaContract>,
    current: Option<&ReleaseSchemaContract>,
    known_good: Option<&ReleaseSchemaContract>,
    preconditions: &Preconditions,
) -> Result<Evaluation, D1Error> {
    let base_state = classify_prefix(remote_names, &authority.ordered_history);
    if matches!(
        base_state,
        LedgerState::Diverged | LedgerState::UnknownMigration | LedgerState::CorruptLedger
    ) {
        return Ok(blocked_evaluation(
            base_state,
            Decision::RecoveryRequired,
            remote_names,
            target
                .map(|value| value.target_schema_revision.clone())
                .unwrap_or_else(|| authority.current_repository_revision.clone()),
            "LEDGER_NOT_CANONICAL",
        ));
    }

    let Some(target) = target else {
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
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    };

    validate_release_contract(authority, target)?;
    if let Some(value) = current {
        validate_release_contract(authority, value)?;
    }
    if let Some(value) = known_good {
        validate_release_contract(authority, value)?;
    }

    let target_index = revision_index(&authority.ordered_history, &target.target_schema_revision)?;
    let minimum = revision_index(&authority.ordered_history, &target.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &target.supported_schema_max)?;
    if minimum > target_index || target_index > maximum {
        return Err(D1Error::new(
            "release schema window must satisfy supported_min <= target <= supported_max",
        ));
    }

    let remote_count = remote_names.len();
    let target_count = target_index + 1;
    let state = classify_relative_state(remote_count, target_count, minimum, maximum);

    if action == D1Action::Verify {
        if state == LedgerState::Exact {
            return Ok(Evaluation {
                ledger_state: state,
                decision: Decision::Safe,
                remote_revision: remote_names.last().cloned(),
                target_revision: target.target_schema_revision.clone(),
                planned_migrations: Vec::new(),
                planned_contracts: Vec::new(),
                reason_codes: Vec::new(),
                rollback_context_complete: false,
                allowed: true,
            });
        }
        return Ok(blocked_evaluation(
            state,
            Decision::RecoveryRequired,
            remote_names,
            target.target_schema_revision.clone(),
            "POST_APPLY_TARGET_MISMATCH",
        ));
    }

    if state == LedgerState::AheadKnownCompatible {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackSafe,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations: Vec::new(),
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    }
    if state == LedgerState::AheadKnownIncompatible {
        return Ok(blocked_evaluation(
            state,
            Decision::CodeRollbackBlocked,
            remote_names,
            target.target_schema_revision.clone(),
            "RUNTIME_SCHEMA_WINDOW_EXCEEDED",
        ));
    }
    if state == LedgerState::Exact {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::Safe,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations: Vec::new(),
            planned_contracts: Vec::new(),
            reason_codes: Vec::new(),
            rollback_context_complete: current.is_some() && known_good.is_some(),
            allowed: true,
        });
    }

    let planned_migrations = authority.ordered_history[remote_count..target_count].to_vec();
    let planned_contracts = planned_contract_values(authority, remote_count, target_count);
    if action == D1Action::Compatibility {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::MigrationRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: Vec::new(),
            rollback_context_complete: false,
            allowed: true,
        });
    }

    if remote_count < authority.historical_len {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::MigrationRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["HISTORICAL_COMPATIBILITY_UNKNOWN".to_owned()],
            rollback_context_complete: current.is_some() && known_good.is_some(),
            allowed: false,
        });
    }

    let Some(current) = current else {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["CURRENT_RUNTIME_CONTEXT_MISSING".to_owned()],
            rollback_context_complete: false,
            allowed: false,
        });
    };
    let Some(known_good) = known_good else {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::CodeRollbackBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["KNOWN_GOOD_RUNTIME_CONTEXT_MISSING".to_owned()],
            rollback_context_complete: false,
            allowed: false,
        });
    };

    if !runtime_supports_remote(authority, current, remote_count)? {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::RecoveryRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["CURRENT_RUNTIME_ALREADY_SCHEMA_INCOMPATIBLE".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let contracts = post_epoch_slice(authority, remote_count, target_count)?;
    if let Some(missing) = missing_contract_precondition(&contracts, preconditions) {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::ContractBlocked,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec![format!("CONTRACT_PRECONDITION_MISSING:{missing}")],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let rollback_supported = runtime_supports_index(authority, known_good, target_index)?;
    let fail_forward = contracts
        .iter()
        .any(|contract| contract.fail_forward_required);
    if !rollback_supported {
        return Ok(Evaluation {
            ledger_state: state,
            decision: if fail_forward {
                Decision::FailForwardRequired
            } else {
                Decision::CodeRollbackBlocked
            },
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["KNOWN_GOOD_INCOMPATIBLE_AFTER_MIGRATION".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }
    if fail_forward {
        return Ok(Evaluation {
            ledger_state: state,
            decision: Decision::FailForwardRequired,
            remote_revision: remote_names.last().cloned(),
            target_revision: target.target_schema_revision.clone(),
            planned_migrations,
            planned_contracts,
            reason_codes: vec!["EXPLICIT_FAIL_FORWARD_TRANSITION".to_owned()],
            rollback_context_complete: true,
            allowed: false,
        });
    }

    let decision = aggregate_rollout_decision(&contracts)?;
    let allowed = !matches!(decision, Decision::DeployFirst);
    Ok(Evaluation {
        ledger_state: state,
        decision,
        remote_revision: remote_names.last().cloned(),
        target_revision: target.target_schema_revision.clone(),
        planned_migrations,
        planned_contracts,
        reason_codes: Vec::new(),
        rollback_context_complete: true,
        allowed,
    })
}

fn validate_release_contract(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
) -> Result<(), D1Error> {
    if contract.database_component != authority.component_id {
        return Err(D1Error::new("release schema contract component mismatch"));
    }
    if contract.migration_history_digest != authority.history_digest {
        return Err(D1Error::new(
            "release migration_history_digest differs from canonical component history",
        ));
    }
    if contract.compatibility_policy_digest.is_empty() {
        return Err(D1Error::new(
            "compatibility_policy_digest must not be empty",
        ));
    }
    let target = revision_index(&authority.ordered_history, &contract.target_schema_revision)?;
    let minimum = revision_index(&authority.ordered_history, &contract.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &contract.supported_schema_max)?;
    if minimum > target || target > maximum {
        return Err(D1Error::new(
            "release schema window must satisfy supported_min <= target <= supported_max",
        ));
    }
    Ok(())
}

fn classify_relative_state(
    remote_count: usize,
    target_count: usize,
    minimum: usize,
    maximum: usize,
) -> LedgerState {
    if remote_count == target_count {
        return LedgerState::Exact;
    }
    if remote_count < target_count {
        return LedgerState::BehindKnownPrefix;
    }
    let remote_index = remote_count - 1;
    if remote_index >= minimum && remote_index <= maximum {
        LedgerState::AheadKnownCompatible
    } else {
        LedgerState::AheadKnownIncompatible
    }
}

fn runtime_supports_remote(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
    remote_count: usize,
) -> Result<bool, D1Error> {
    if remote_count == 0 {
        return Ok(false);
    }
    runtime_supports_index(authority, contract, remote_count - 1)
}

fn runtime_supports_index(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
    index: usize,
) -> Result<bool, D1Error> {
    let minimum = revision_index(&authority.ordered_history, &contract.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &contract.supported_schema_max)?;
    Ok(index >= minimum && index <= maximum)
}

fn post_epoch_slice(
    authority: &ComponentAuthority,
    remote_count: usize,
    target_count: usize,
) -> Result<Vec<&MigrationContract>, D1Error> {
    if remote_count < authority.historical_len {
        return Err(D1Error::new(
            "post-epoch contract slice cannot include frozen historical migrations",
        ));
    }
    let start = remote_count - authority.historical_len;
    let end = target_count - authority.historical_len;
    authority
        .post_epoch
        .get(start..end)
        .map(|slice| slice.iter().collect())
        .ok_or_else(|| D1Error::new("post-epoch migration contract slice is invalid"))
}

fn missing_contract_precondition(
    contracts: &[&MigrationContract],
    evidence: &Preconditions,
) -> Option<String> {
    contracts
        .iter()
        .filter(|contract| contract.migration_class == MigrationClass::Contract)
        .flat_map(|contract| contract.contract_preconditions.iter())
        .find(|required| !evidence.completed.contains(required.as_str()))
        .cloned()
}

fn aggregate_rollout_decision(contracts: &[&MigrationContract]) -> Result<Decision, D1Error> {
    let mut migrate_first = false;
    let mut deploy_first = false;
    let mut separate_contract = false;
    for contract in contracts {
        match contract.rollout_order {
            RolloutOrder::MigrateBeforeCode => migrate_first = true,
            RolloutOrder::CodeBeforeMigrate => deploy_first = true,
            RolloutOrder::Either => {}
            RolloutOrder::SeparateContractRelease => separate_contract = true,
        }
    }
    if migrate_first && deploy_first {
        return Err(D1Error::new(
            "planned migration set contains contradictory rollout orders",
        ));
    }
    if separate_contract {
        return Ok(Decision::Safe);
    }
    if migrate_first {
        return Ok(Decision::MigrateFirst);
    }
    if deploy_first {
        return Ok(Decision::DeployFirst);
    }
    Ok(Decision::MigrationRequired)
}

fn planned_contract_values(
    authority: &ComponentAuthority,
    remote_count: usize,
    target_count: usize,
) -> Vec<Value> {
    if remote_count < authority.historical_len {
        return Vec::new();
    }
    let start = remote_count - authority.historical_len;
    let end = target_count.saturating_sub(authority.historical_len);
    authority
        .post_epoch
        .get(start..end)
        .map(|contracts| {
            contracts
                .iter()
                .map(|contract| {
                    json!({
                        "migration_file": contract.migration_file,
                        "migration_class": contract.migration_class.as_str(),
                        "rollout_order": contract.rollout_order.as_str(),
                        "fail_forward_required": contract.fail_forward_required,
                        "destructive": contract.destructive,
                        "code_rollback_allowed": contract.code_rollback_allowed,
                        "contract_preconditions": contract.contract_preconditions
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn blocked_evaluation(
    state: LedgerState,
    decision: Decision,
    remote_names: &[String],
    target_revision: String,
    reason: &str,
) -> Evaluation {
    Evaluation {
        ledger_state: state,
        decision,
        remote_revision: remote_names.last().cloned(),
        target_revision,
        planned_migrations: Vec::new(),
        planned_contracts: Vec::new(),
        reason_codes: vec![reason.to_owned()],
        rollback_context_complete: false,
        allowed: false,
    }
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
        .ok_or_else(|| {
            D1Error::new(format!(
                "unknown schema revision in release contract: {revision}"
            ))
        })
}

fn ensure_unique(values: &[String], label: &str) -> Result<(), D1Error> {
    let mut observed = HashSet::with_capacity(values.len());
    for value in values {
        if !observed.insert(value.as_str()) {
            return Err(D1Error::new(format!(
                "{label} contains duplicate entry: {value}"
            )));
        }
    }
    Ok(())
}

fn required_value_string(value: &Value, key: &str, label: &str) -> Result<String, D1Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("{label} {key} is missing")))
}

fn required_string(object: &serde_json::Map<String, Value>, key: &str) -> Result<String, D1Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("required string field {key} is missing")))
}

fn required_bool(object: &serde_json::Map<String, Value>, key: &str) -> Result<bool, D1Error> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| D1Error::new(format!("required boolean field {key} is missing")))
}

fn required_string_array(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, D1Error> {
    let values = object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new(format!("required array field {key} is missing")))?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let item = value
            .as_str()
            .filter(|item| !item.is_empty())
            .ok_or_else(|| D1Error::new(format!("{key} must contain non-empty strings")))?;
        result.push(item.to_owned());
    }
    ensure_unique(&result, key)?;
    Ok(result)
}

fn resolve_input(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_json(path: &Path, label: &str) -> Result<Value, D1Error> {
    let text = fs::read_to_string(path).map_err(|error| {
        D1Error::new(format!("cannot read {label} {}: {error}", path.display()))
    })?;
    serde_json::from_str(&text)
        .map_err(|error| D1Error::new(format!("cannot parse {label} {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentAuthority, D1Action, D1Error, LedgerState, Preconditions, ReleaseSchemaContract,
        classify_prefix, evaluate,
    };

    fn authority() -> ComponentAuthority {
        ComponentAuthority {
            component_id: "catalog".to_owned(),
            historical_len: 3,
            ordered_history: vec![
                "0001_a.sql".to_owned(),
                "0002_b.sql".to_owned(),
                "0003_c.sql".to_owned(),
            ],
            post_epoch: Vec::new(),
            current_repository_revision: "0003_c.sql".to_owned(),
            history_digest: "history-digest".to_owned(),
        }
    }

    fn contract(maximum: &str) -> ReleaseSchemaContract {
        ReleaseSchemaContract {
            database_component: "catalog".to_owned(),
            target_schema_revision: "0002_b.sql".to_owned(),
            supported_schema_min: "0001_a.sql".to_owned(),
            supported_schema_max: maximum.to_owned(),
            migration_history_digest: "history-digest".to_owned(),
            compatibility_policy_digest: "policy-digest".to_owned(),
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
            None,
            None,
            &Preconditions::default(),
        )?;
        assert_eq!(compatible.ledger_state, LedgerState::AheadKnownCompatible);
        assert!(compatible.allowed);

        let incompatible = evaluate(
            D1Action::Compatibility,
            &authority(),
            &remote,
            Some(&contract("0002_b.sql")),
            None,
            None,
            &Preconditions::default(),
        )?;
        assert_eq!(
            incompatible.ledger_state,
            LedgerState::AheadKnownIncompatible
        );
        assert!(!incompatible.allowed);
        Ok(())
    }

    #[test]
    fn historical_plan_is_visible_but_not_automatically_allowed() -> Result<(), D1Error> {
        let remote = vec!["0001_a.sql".to_owned()];
        let result = evaluate(
            D1Action::Plan,
            &authority(),
            &remote,
            Some(&contract("0003_c.sql")),
            Some(&contract("0003_c.sql")),
            Some(&contract("0003_c.sql")),
            &Preconditions::default(),
        )?;
        assert_eq!(result.ledger_state, LedgerState::BehindKnownPrefix);
        assert!(!result.allowed);
        assert_eq!(result.planned_migrations, vec!["0002_b.sql"]);
        assert_eq!(
            result.reason_codes,
            vec!["HISTORICAL_COMPATIBILITY_UNKNOWN"]
        );
        Ok(())
    }
}
