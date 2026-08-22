use super::model::{D1Error, Preconditions, ReleaseSchemaContract};
use super::util::{read_json, required_string, required_string_array};
use serde_json::Value;
use std::path::Path;

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
