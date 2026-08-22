use super::model::{ComponentAuthority, D1Error, ReleaseSchemaContract};
use super::status::revision_index;

pub(super) fn validate_release_contract(
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
    if contract.compatibility_policy_digest != authority.policy_digest {
        return Err(D1Error::new(
            "release compatibility_policy_digest differs from canonical typed D1 policy",
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

pub(super) fn runtime_supports_remote(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
    remote_count: usize,
) -> Result<bool, D1Error> {
    if remote_count == 0 {
        return Ok(false);
    }
    runtime_supports_index(authority, contract, remote_count - 1)
}

pub(super) fn runtime_supports_index(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
    index: usize,
) -> Result<bool, D1Error> {
    let minimum = revision_index(&authority.ordered_history, &contract.supported_schema_min)?;
    let maximum = revision_index(&authority.ordered_history, &contract.supported_schema_max)?;
    Ok(index >= minimum && index <= maximum)
}
