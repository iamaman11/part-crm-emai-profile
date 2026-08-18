use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::path::Path;

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
pub(super) enum LedgerState {
    Exact,
    BehindKnownPrefix,
    AheadKnownCompatible,
    AheadKnownIncompatible,
    Diverged,
    UnknownMigration,
    CorruptLedger,
}

impl LedgerState {
    pub(super) const fn as_str(self) -> &'static str {
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
pub(super) enum Decision {
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
    pub(super) const fn as_str(self) -> &'static str {
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
pub(super) enum MigrationClass {
    Expand,
    Backfill,
    Contract,
    Repair,
}

impl MigrationClass {
    pub(super) fn parse(value: &str) -> Result<Self, D1Error> {
        match value {
            "EXPAND" => Ok(Self::Expand),
            "BACKFILL" => Ok(Self::Backfill),
            "CONTRACT" => Ok(Self::Contract),
            "REPAIR" => Ok(Self::Repair),
            other => Err(D1Error::new(format!("unknown migration class: {other}"))),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Expand => "EXPAND",
            Self::Backfill => "BACKFILL",
            Self::Contract => "CONTRACT",
            Self::Repair => "REPAIR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RolloutOrder {
    MigrateBeforeCode,
    CodeBeforeMigrate,
    Either,
    SeparateContractRelease,
}

impl RolloutOrder {
    pub(super) fn parse(value: &str) -> Result<Self, D1Error> {
        match value {
            "MIGRATE_BEFORE_CODE" => Ok(Self::MigrateBeforeCode),
            "CODE_BEFORE_MIGRATE" => Ok(Self::CodeBeforeMigrate),
            "EITHER" => Ok(Self::Either),
            "SEPARATE_CONTRACT_RELEASE" => Ok(Self::SeparateContractRelease),
            other => Err(D1Error::new(format!("unknown rollout order: {other}"))),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
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
    pub(super) fn new(message: impl Into<String>) -> Self {
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
pub(super) struct MigrationContract {
    pub(super) migration_file: String,
    pub(super) migration_class: MigrationClass,
    pub(super) rollout_order: RolloutOrder,
    pub(super) fail_forward_required: bool,
    pub(super) destructive: bool,
    pub(super) code_rollback_allowed: bool,
    pub(super) contract_preconditions: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ComponentAuthority {
    pub(super) component_id: String,
    pub(super) historical_len: usize,
    pub(super) ordered_history: Vec<String>,
    pub(super) post_epoch: Vec<MigrationContract>,
    pub(super) current_repository_revision: String,
    pub(super) history_digest: String,
}

#[derive(Debug, Clone)]
pub(super) struct ReleaseSchemaContract {
    pub(super) database_component: String,
    pub(super) target_schema_revision: String,
    pub(super) supported_schema_min: String,
    pub(super) supported_schema_max: String,
    pub(super) migration_history_digest: String,
    pub(super) compatibility_policy_digest: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Preconditions {
    pub(super) completed: HashSet<String>,
}

#[derive(Debug, Clone)]
pub(super) struct Evaluation {
    pub(super) ledger_state: LedgerState,
    pub(super) decision: Decision,
    pub(super) remote_revision: Option<String>,
    pub(super) target_revision: String,
    pub(super) planned_migrations: Vec<String>,
    pub(super) planned_contracts: Vec<Value>,
    pub(super) reason_codes: Vec<String>,
    pub(super) rollback_context_complete: bool,
    pub(super) allowed: bool,
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
