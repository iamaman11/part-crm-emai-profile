use super::model::{ComponentAuthority, D1Error, Evaluation, ReleaseSchemaContract};
use super::status::revision_index;

/// A runtime fact admitted against a proved canonical history, never a deployment target.
pub(super) struct RuntimeSchemaContext {
    schema: ReleaseSchemaContract,
}

impl RuntimeSchemaContext {
    pub(super) fn new(
        history: &ComponentAuthority,
        schema: ReleaseSchemaContract,
    ) -> Result<Self, D1Error> {
        validate_release_contract(history, &schema)?;
        Ok(Self { schema })
    }

    pub(super) fn schema(&self) -> &ReleaseSchemaContract {
        &self.schema
    }
}

pub(super) struct RuntimeCompatibility {
    pub(super) compatible: bool,
    pub(super) reason_codes: Vec<String>,
    pub(super) required_steps: Vec<String>,
}

/// Migration eligibility is not evidence that the candidate can run on the observed schema.
/// Keep the migration verdict intact for prepare/executor callers and evaluate runtime admission
/// against the same native schema window used by D1 planning.
pub(super) fn runtime_compatibility(
    authority: &ComponentAuthority,
    contract: &ReleaseSchemaContract,
    remote_count: usize,
    evaluation: &Evaluation,
) -> Result<RuntimeCompatibility, D1Error> {
    if !evaluation.allowed {
        return Ok(RuntimeCompatibility {
            compatible: false,
            reason_codes: evaluation.reason_codes.clone(),
            required_steps: Vec::new(),
        });
    }
    if runtime_supports_remote(authority, contract, remote_count)? {
        return Ok(RuntimeCompatibility {
            compatible: true,
            reason_codes: Vec::new(),
            required_steps: Vec::new(),
        });
    }
    Ok(RuntimeCompatibility {
        compatible: false,
        reason_codes: vec!["RUNTIME_SCHEMA_MIGRATION_REQUIRED".to_owned()],
        required_steps: vec![format!(
            "Prepare {} D1 convergence to {} through its existing migration owner, preserving the planned migration contracts and separate authorization; then rerun read-only promotion preflight.",
            authority.component_id, contract.target_schema_revision
        )],
    })
}

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
