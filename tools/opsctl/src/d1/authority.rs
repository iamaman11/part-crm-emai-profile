use super::model::{
    ComponentAuthority, D1Error, MigrationClass, MigrationContract, Preconditions,
    ReleaseSchemaContract, RolloutOrder,
};
use super::util::{
    ensure_unique, read_json, required_bool, required_string, required_string_array,
    required_value_string, resolve_input,
};
use serde_json::Value;
use std::path::Path;

pub(super) fn load_component_authority(
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

pub(super) fn load_wrangler_ledger(path: &Path) -> Result<Vec<String>, D1Error> {
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
            if last_id.is_some_and(|previous| id <= previous) {
                return Err(D1Error::new(
                    "D1 ledger ids must be strictly increasing in query order",
                ));
            }
            last_id = Some(id);
        }
        names.push(name);
    }
    Ok(names)
}

pub(super) fn load_release_contract(
    path: &Path,
    component: &str,
) -> Result<ReleaseSchemaContract, D1Error> {
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

pub(super) fn load_preconditions(path: &Path, component: &str) -> Result<Preconditions, D1Error> {
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
