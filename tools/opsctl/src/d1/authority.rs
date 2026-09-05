use super::model::{D1Error, GateResult, Preconditions, ReleaseSchemaContract};
use super::util::{read_json, required_string};
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
    const REMEDIATION: &str = "Regenerate the preconditions input from the typed caller contract with exact component and completed fields, then rerun prepare before requesting authorization.";

    let document = read_json(path, "D1 contract preconditions")?;
    let object = document.as_object().ok_or_else(|| {
        D1Error::blocked(GateResult::blocked(
            "INPUT_VALIDATION",
            "d1.preconditions.schema",
            "D1_PRECONDITIONS_NOT_OBJECT",
            "D1 contract preconditions must be a JSON object",
            Some("{\"component\":<string>,\"completed\":[<string>...]}".to_owned()),
            Some("non-object JSON value".to_owned()),
            REMEDIATION,
        ))
    })?;

    let precondition_component =
        object
            .get("component")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                D1Error::blocked(GateResult::blocked(
                    "INPUT_VALIDATION",
                    "d1.preconditions.schema",
                    "D1_PRECONDITIONS_COMPONENT_INVALID",
                    "D1 contract preconditions require a string component field",
                    Some(format!("component={component:?}")),
                    Some(if object.contains_key("component") {
                        "component present but not a string".to_owned()
                    } else {
                        "component field absent".to_owned()
                    }),
                    REMEDIATION,
                ))
            })?;
    if precondition_component != component {
        return Err(D1Error::blocked(GateResult::blocked(
            "INPUT_VALIDATION",
            "d1.preconditions.component",
            "D1_PRECONDITIONS_COMPONENT_MISMATCH",
            "D1 contract precondition component does not match the requested component",
            Some(component.to_owned()),
            Some(precondition_component.to_owned()),
            REMEDIATION,
        )));
    }

    let completed_values = object
        .get("completed")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            D1Error::blocked(GateResult::blocked(
                "INPUT_VALIDATION",
                "d1.preconditions.schema",
                "D1_PRECONDITIONS_COMPLETED_INVALID",
                "D1 contract preconditions require a completed string array",
                Some("completed=[<string>...]".to_owned()),
                Some(if object.contains_key("completed") {
                    "completed present but not an array".to_owned()
                } else {
                    "completed field absent".to_owned()
                }),
                REMEDIATION,
            ))
        })?;
    let mut completed = std::collections::HashSet::with_capacity(completed_values.len());
    for (index, value) in completed_values.iter().enumerate() {
        let item = value.as_str().ok_or_else(|| {
            D1Error::blocked(GateResult::blocked(
                "INPUT_VALIDATION",
                "d1.preconditions.schema",
                "D1_PRECONDITIONS_COMPLETED_INVALID",
                "D1 contract preconditions completed entries must all be strings",
                Some("completed=[<string>...]".to_owned()),
                Some(format!("completed[{index}] is not a string")),
                REMEDIATION,
            ))
        })?;
        completed.insert(item.to_owned());
    }

    Ok(Preconditions { completed })
}

#[cfg(test)]
mod tests {
    use super::load_preconditions;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("opsctl-d1-{name}-{}.json", std::process::id()))
    }

    #[test]
    fn historical_empty_preconditions_are_self_explaining() -> Result<(), std::io::Error> {
        let path = fixture_path("empty-preconditions");
        std::fs::write(&path, "{}")?;
        let result = load_preconditions(&path, "catalog");
        std::fs::remove_file(&path)?;
        let error = match result {
            Err(error) => error,
            Ok(_) => {
                return Err(std::io::Error::other(
                    "empty preconditions must fail closed",
                ));
            }
        };
        let gate = error.gate_result_json();
        assert_eq!(gate["status"], "BLOCKED");
        assert_eq!(gate["phase"], "INPUT_VALIDATION");
        assert_eq!(gate["gate_id"], "d1.preconditions.schema");
        assert_eq!(gate["reason_code"], "D1_PRECONDITIONS_COMPONENT_INVALID");
        assert_eq!(gate["observed"], "component field absent");
        assert!(
            gate["remediation"].as_str().is_some_and(
                |value| value.contains("rerun prepare before requesting authorization")
            )
        );
        assert_eq!(gate["transaction_id"], serde_json::Value::Null);
        Ok(())
    }

    #[test]
    fn missing_completed_field_has_distinct_durable_reason() -> Result<(), std::io::Error> {
        let path = fixture_path("missing-completed");
        std::fs::write(&path, r#"{"component":"catalog"}"#)?;
        let result = load_preconditions(&path, "catalog");
        std::fs::remove_file(&path)?;
        let error = match result {
            Err(error) => error,
            Ok(_) => return Err(std::io::Error::other("missing completed must fail closed")),
        };
        let gate = error.gate_result_json();
        assert_eq!(gate["status"], "BLOCKED");
        assert_eq!(gate["reason_code"], "D1_PRECONDITIONS_COMPLETED_INVALID");
        assert_eq!(gate["observed"], "completed field absent");
        Ok(())
    }
}
