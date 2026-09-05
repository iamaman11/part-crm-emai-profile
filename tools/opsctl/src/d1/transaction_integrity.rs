use super::model::D1Error;
use super::transaction::{
    ProviderObservationInput, TransactionKind, TransactionPhase, TransactionProjection,
};
use crate::canonical::{canonical_json, sha256_hex};
use std::collections::BTreeSet;

const TRANSACTION_SCHEMA_VERSION: u64 = 1;
const RELEASE_SET_PREFIX: &str = "release-set-v3-sha256-";

pub fn revalidate_transaction_projection(
    transaction: &TransactionProjection,
) -> Result<(), D1Error> {
    if transaction.schema_version != TRANSACTION_SCHEMA_VERSION
        || transaction.transaction_plan.schema_version != TRANSACTION_SCHEMA_VERSION
        || transaction.provider_observation.schema_version != TRANSACTION_SCHEMA_VERSION
    {
        return Err(D1Error::new(
            "prepared transaction revalidation requires schema_version 1",
        ));
    }
    if transaction.status != "TRANSACTION_PREPARED" || transaction.mode != "read-only" {
        return Err(D1Error::new(
            "prepared transaction revalidation requires read-only TRANSACTION_PREPARED status",
        ));
    }
    if transaction.authorization_consumed
        || transaction.mutation_executed
        || transaction.provider_mutation_executed
    {
        return Err(D1Error::new(
            "prepared transaction revalidation requires an unconsumed, non-mutating projection",
        ));
    }

    let plan = &transaction.transaction_plan;
    let observation = &transaction.provider_observation;
    validate_sha256(&transaction.transaction_id, "transaction_id")?;
    validate_target(
        &plan.target.environment,
        &plan.target.account_id,
        &plan.target.database_name,
        &plan.target.database_id,
        "transaction plan target",
    )?;
    validate_target(
        &observation.target.environment,
        &observation.target.account_id,
        &observation.target.database_name,
        &observation.target.database_id,
        "provider observation target",
    )?;
    if plan.target != observation.target {
        return Err(D1Error::new(
            "transaction plan target must equal the sealed provider observation target",
        ));
    }
    if plan.transaction_kind != TransactionKind::D1Migration {
        return Err(D1Error::new(
            "prepared transaction kind must be D1_MIGRATION",
        ));
    }
    if plan.phase != TransactionPhase::Ordinary {
        return Err(D1Error::new(
            "prepared PREPARE_READY transaction phase must remain ORDINARY",
        ));
    }

    validate_git_object_id(&plan.source_sha, "transaction plan source_sha")?;
    validate_git_object_id(&plan.tree_sha, "transaction plan tree_sha")?;
    validate_release_candidate_id(&plan.release_candidate_id)?;
    validate_sha256(
        &plan.repository_identity_sha256,
        "transaction plan repository_identity_sha256",
    )?;
    validate_sha256(
        &plan.planner_policy_digest,
        "transaction plan planner_policy_digest",
    )?;
    validate_sha256(
        &plan.migration_lineage_digest,
        "transaction plan migration_lineage_digest",
    )?;
    validate_sha256(
        &plan.predecessor_ledger_sha256,
        "transaction plan predecessor_ledger_sha256",
    )?;
    validate_sha256(
        &plan.observation_digest,
        "transaction plan observation_digest",
    )?;
    validate_sha256(
        &observation.observation_digest,
        "provider observation observation_digest",
    )?;
    validate_sha256(
        &observation.remote_ledger_sha256,
        "provider observation remote_ledger_sha256",
    )?;
    if plan.observation_digest != observation.observation_digest
        || plan.observed_at_unix_seconds != observation.observed_at_unix_seconds
    {
        return Err(D1Error::new(
            "transaction plan observation identity must equal the sealed provider observation",
        ));
    }
    if plan.predecessor_ledger_sha256 != observation.remote_ledger_sha256 {
        return Err(D1Error::new(
            "transaction predecessor ledger must equal the sealed provider observation ledger",
        ));
    }
    if plan.observed_at_unix_seconds <= 0 || plan.freshness_max_age_seconds == 0 {
        return Err(D1Error::new(
            "transaction observation time must be positive and freshness window non-zero",
        ));
    }

    validate_non_empty(
        &observation.observation_source,
        "provider observation source",
    )?;
    validate_unique_strings(&observation.remote_migrations, "provider remote_migrations")?;
    validate_unique_strings(
        &observation.wrangler_pending_migrations,
        "provider wrangler_pending_migrations",
    )?;
    let observation_input = ProviderObservationInput {
        schema_version: observation.schema_version,
        target: observation.target.clone(),
        observed_at_unix_seconds: observation.observed_at_unix_seconds,
        observation_source: observation.observation_source.clone(),
        remote_ledger_sha256: observation.remote_ledger_sha256.clone(),
        remote_migrations: observation.remote_migrations.clone(),
        wrangler_pending_migrations: observation.wrangler_pending_migrations.clone(),
        deployment_identity: observation.deployment_identity.clone(),
        time_travel_bookmark_capable: observation.time_travel_bookmark_capable,
    };
    let canonical_observation =
        canonical_json(&serde_json::to_value(&observation_input).map_err(|error| {
            D1Error::new(format!("cannot serialize provider observation: {error}"))
        })?)
        .map_err(D1Error::new)?;
    if sha256_hex(canonical_observation.as_bytes()) != observation.observation_digest {
        return Err(D1Error::new(
            "provider observation digest does not match its canonical sealed facts",
        ));
    }

    if plan.release_manifest_digests.is_empty() {
        return Err(D1Error::new(
            "transaction plan release_manifest_digests must not be empty",
        ));
    }
    for (name, digest) in &plan.release_manifest_digests {
        validate_non_empty(name, "release manifest digest name")?;
        validate_sha256(digest, "release manifest digest")?;
    }
    let mut migration_names = BTreeSet::new();
    for migration in &plan.planned_migrations {
        validate_non_empty(&migration.migration_file, "planned migration filename")?;
        validate_sha256(
            &migration.content_sha256,
            "planned migration content digest",
        )?;
        if !migration_names.insert(&migration.migration_file) {
            return Err(D1Error::new(
                "transaction plan planned_migrations must not contain duplicate filenames",
            ));
        }
    }
    let planned_names = plan
        .planned_migrations
        .iter()
        .map(|migration| migration.migration_file.clone())
        .collect::<Vec<_>>();
    if observation.wrangler_pending_migrations != planned_names {
        return Err(D1Error::new(
            "sealed Wrangler pending migrations must exactly equal prepared transaction planned_migrations",
        ));
    }
    validate_unique_strings(
        &plan.precondition_evidence_refs,
        "transaction precondition_evidence_refs",
    )?;
    validate_non_empty(&plan.schema_target, "transaction schema_target")?;
    validate_non_empty(
        &plan.supported_schema_min,
        "transaction supported_schema_min",
    )?;
    validate_non_empty(
        &plan.supported_schema_max,
        "transaction supported_schema_max",
    )?;
    if !plan.expected_post_state.is_object() {
        return Err(D1Error::new(
            "transaction expected_post_state must be a JSON object",
        ));
    }
    validate_effect_scope(
        &plan.allowed_provider_effects,
        "transaction allowed_provider_effects",
    )?;
    if plan.allowed_provider_effects.is_empty() {
        return Err(D1Error::new(
            "transaction allowed_provider_effects must not be empty",
        ));
    }
    validate_effect_scope(
        &plan.forbidden_provider_effects,
        "transaction forbidden_provider_effects",
    )?;
    let forbidden = plan
        .forbidden_provider_effects
        .iter()
        .collect::<BTreeSet<_>>();
    if plan
        .allowed_provider_effects
        .iter()
        .any(|effect| forbidden.contains(effect))
    {
        return Err(D1Error::new(
            "transaction allowed and forbidden provider effects must be disjoint",
        ));
    }

    let canonical_plan = canonical_json(&serde_json::to_value(plan).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize migration transaction plan for revalidation: {error}"
        ))
    })?)
    .map_err(D1Error::new)?;
    if sha256_hex(canonical_plan.as_bytes()) != transaction.transaction_id {
        return Err(D1Error::new(
            "transaction_id must equal sha256(canonical MigrationTransactionPlan)",
        ));
    }
    Ok(())
}

fn validate_effect_scope(effects: &[String], label: &str) -> Result<(), D1Error> {
    validate_unique_strings(effects, label)
}

fn validate_unique_strings(values: &[String], label: &str) -> Result<(), D1Error> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_non_empty(value, label)?;
        if !unique.insert(value) {
            return Err(D1Error::new(format!("{label} must not contain duplicates")));
        }
    }
    Ok(())
}

fn validate_target(
    environment: &str,
    account_id: &str,
    database_name: &str,
    database_id: &str,
    label: &str,
) -> Result<(), D1Error> {
    validate_non_empty(environment, &format!("{label}.environment"))?;
    validate_non_empty(account_id, &format!("{label}.account_id"))?;
    validate_non_empty(database_name, &format!("{label}.database_name"))?;
    validate_non_empty(database_id, &format!("{label}.database_id"))?;
    Ok(())
}

fn validate_release_candidate_id(value: &str) -> Result<(), D1Error> {
    let digest = value.strip_prefix(RELEASE_SET_PREFIX).ok_or_else(|| {
        D1Error::new(format!(
            "transaction release_candidate_id must start with {RELEASE_SET_PREFIX}"
        ))
    })?;
    validate_sha256(digest, "transaction release_candidate_id digest")
}

fn validate_git_object_id(value: &str, label: &str) -> Result<(), D1Error> {
    if !matches!(value.len(), 40 | 64) || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be a 40- or 64-character lowercase Git object id"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(D1Error::new(format!(
            "{label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!("{label} must not be empty")));
    }
    Ok(())
}
