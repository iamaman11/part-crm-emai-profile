const SOURCE: &str = include_str!("../src/dirty_generation_commit.rs");

fn policy_errors(source: &str) -> Vec<String> {
    let production = source.split("#[cfg(test)]").next().unwrap_or(source);
    let mut errors = Vec::new();

    for required in [
        "validate_execution(scope, proof, prepared)",
        "publish_prepared_dirty_generation(scope, prepared, upload, verifier)",
        "DirtyGenerationCommitRequest::from_published(proof, &published)",
        ".commit_dirty_generation(scope, &request)",
        "metadata.base_generation_id() != Some(proof.generation_id())",
        "lease.status() != LeaseStatus::Active",
    ] {
        if !production.contains(required) {
            errors.push(format!("missing dirty-generation invariant: {required}"));
        }
    }

    let function = production
        .split("pub async fn publish_and_commit_dirty_generation")
        .nth(1)
        .unwrap_or_default();
    let publish = function.find("publish_prepared_dirty_generation");
    let commit = function.find(".commit_dirty_generation");
    if publish.is_none() || commit.is_none() || publish >= commit {
        errors.push("metadata commit must happen only after immutable publish/exact verify".to_owned());
    }

    for forbidden in [
        "mark_synced(",
        "workspace_lock.release",
        "coordinator.release",
        "std::fs::remove",
        "reqwest",
        "ureq",
        "R2GenerationObjects",
        "R2_PROFILES_BINDING",
    ] {
        if function.contains(forbidden) {
            errors.push(format!(
                "dirty-generation commit flow must not mutate ownership or bind provider transport: {forbidden}"
            ));
        }
    }

    let request = production
        .split("pub struct DirtyGenerationCommitRequest {")
        .nth(1)
        .and_then(|value| value.split("\n}\n\nimpl DirtyGenerationCommitRequest").next())
        .unwrap_or_default();
    for forbidden in [
        "tenant_id",
        "device_id",
        "observed_at",
        "executed_at",
        "expected_job_version",
        "expected_profile_version",
        "coordinator_version",
        "coordinator_sequence",
    ] {
        if request.contains(forbidden) {
            errors.push(format!(
                "Bridge request carries server-derived authority field: {forbidden}"
            ));
        }
    }

    errors
}

#[test]
fn production_dirty_generation_flow_is_publish_before_commit_and_provider_free() {
    let errors = policy_errors(SOURCE);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn commit_before_publish_negative_fixture_is_rejected() {
    let production = SOURCE.split("#[cfg(test)]").next().unwrap_or(SOURCE);
    let mut fixture = production.to_owned();
    let marker = "let published = publish_prepared_dirty_generation(scope, prepared, upload, verifier)";
    fixture = fixture.replacen(
        marker,
        "let _unsafe_commit_before_publish = commit.commit_dirty_generation(scope, &request);\n    let published = publish_prepared_dirty_generation(scope, prepared, upload, verifier)",
        1,
    );
    let errors = policy_errors(&fixture);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("metadata commit must happen only after")),
        "negative fixture unexpectedly passed: {errors:?}"
    );
}
