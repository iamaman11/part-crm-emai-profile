use super::{finalize_json, request};
use crate::canonical::{parse_strict_json, sha256_hex};
use crate::release::capability_policy_manifest::{
    CAPABILITY_POLICY_ARTIFACT_KIND, CAPABILITY_POLICY_MANIFEST_PATH, render_bytes,
};
use serde_json::{Value, json};
use std::path::PathBuf;

const GIT: &str = "1111111111111111111111111111111111111111";
const WINDOWS_SBOM_PATH: &str = "windows/windows-sbom-v1.json";
const WINDOWS_PROVENANCE_PATH: &str = "windows/windows-provenance-v1.json";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn digest(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn component(
    component_id: &str,
    release_id: &str,
    path: &str,
    digest_character: char,
    size: u64,
) -> Value {
    json!({
        "component_id": component_id,
        "release_id": release_id,
        "source_commit_sha": GIT,
        "artifact_path": path,
        "artifact_sha256": digest(digest_character),
        "artifact_size_bytes": size,
        "component_manifest_sha256": digest('e')
    })
}

fn evidence(path: &str, digest_character: char, size: u64, kind: &str) -> Value {
    json!({
        "path": path,
        "sha256": digest(digest_character),
        "size_bytes": size,
        "kind": kind
    })
}

fn request_value() -> Value {
    json!({
        "schema_version": 1,
        "kind": "RELEASE_FINALIZE_REQUEST",
        "source": {
            "repository": "iamaman11/part-crm-emai-profile",
            "commit_sha": GIT,
            "accepted_main": true,
            "accepted_main_evidence_sha256": digest('a')
        },
        "components": {
            "control_plane": component(
                "control_plane",
                "control-v1",
                "components/control-plane.tar",
                '1',
                11
            ),
            "frontend": component(
                "frontend",
                "control-v1",
                "components/control-plane.tar",
                '1',
                11
            ),
            "secret_resolver": component(
                "secret_resolver",
                "resolver-v1",
                "components/secret-resolver.tar",
                '2',
                12
            ),
            "runtime_bundle": component(
                "runtime_bundle",
                "runtime-v1",
                "components/runtime-bundle.tar",
                '3',
                13
            ),
            "profile_bridge": component(
                "profile_bridge",
                "bridge-v1",
                "components/profile-bridge.zip",
                '4',
                14
            )
        },
        "evidence_artifacts": {
            "windows_sbom": evidence(
                WINDOWS_SBOM_PATH,
                '5',
                15,
                "windows-delivery-sbom"
            ),
            "windows_provenance": evidence(
                WINDOWS_PROVENANCE_PATH,
                '6',
                16,
                "windows-delivery-provenance"
            )
        },
        "protocols": {
            "profile_bridge_protocol_version": 1,
            "resolver_protocol": "mailbox-secret-resolver-v1"
        }
    })
}

fn request_json() -> Result<String, String> {
    serde_json::to_string(&request_value()).map_err(|error| error.to_string())
}

#[test]
fn finalizes_through_pure_v3_core_and_v3_renderer() -> Result<(), String> {
    let output = finalize_json(&root(), &request_json()?).map_err(|error| error.to_string())?;
    let value = parse_strict_json(&output)?;
    assert_eq!(value.get("schema_version").and_then(Value::as_u64), Some(3));
    assert!(
        value
            .get("release_set_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("release-set-v3-sha256-"))
    );
    assert_eq!(value["source"]["commit_sha"].as_str(), Some(GIT));
    assert_eq!(
        value["protocols"]["profile_bridge_protocol_version"].as_u64(),
        Some(1)
    );
    let artifacts = value["artifact_inventory"]
        .as_array()
        .ok_or_else(|| "artifact inventory missing".to_owned())?;
    assert_eq!(artifacts.len(), 7);
    let expected_policy = render_bytes().map_err(|error| error.to_string())?;
    let expected_policy_sha = sha256_hex(&expected_policy);
    let expected_policy_size =
        u64::try_from(expected_policy.len()).map_err(|error| error.to_string())?;
    let policy_artifact = artifacts
        .iter()
        .find(|artifact| artifact["path"].as_str() == Some(CAPABILITY_POLICY_MANIFEST_PATH))
        .ok_or_else(|| "new Release Set writer omitted capability policy manifest".to_owned())?;
    assert_eq!(
        policy_artifact["kind"].as_str(),
        Some(CAPABILITY_POLICY_ARTIFACT_KIND)
    );
    assert_eq!(
        policy_artifact["sha256"].as_str(),
        Some(expected_policy_sha.as_str())
    );
    assert_eq!(
        policy_artifact["size_bytes"].as_u64(),
        Some(expected_policy_size)
    );
    let sbom = artifacts
        .iter()
        .find(|artifact| artifact["path"].as_str() == Some(WINDOWS_SBOM_PATH))
        .ok_or_else(|| "Release Set omitted Windows SBOM".to_owned())?;
    assert_eq!(sbom["sha256"].as_str(), Some(digest('5').as_str()));
    assert_eq!(sbom["kind"].as_str(), Some("windows-delivery-sbom"));
    let provenance = artifacts
        .iter()
        .find(|artifact| artifact["path"].as_str() == Some(WINDOWS_PROVENANCE_PATH))
        .ok_or_else(|| "Release Set omitted Windows provenance".to_owned())?;
    assert_eq!(provenance["sha256"].as_str(), Some(digest('6').as_str()));
    assert_eq!(
        provenance["kind"].as_str(),
        Some("windows-delivery-provenance")
    );
    assert!(value.get("display_version").is_none());
    Ok(())
}

#[test]
fn strict_transport_rejects_duplicates_unknown_fields_and_wrong_identity() -> Result<(), String> {
    let duplicate = request_json()?.replacen("{", "{\"schema_version\":1,", 1);
    assert!(request::parse(&duplicate).is_err());

    let mut unknown = request_value();
    unknown["unexpected"] = Value::Bool(true);
    let unknown = serde_json::to_string(&unknown).map_err(|error| error.to_string())?;
    assert!(request::parse(&unknown).is_err());

    let mut wrong_kind = request_value();
    wrong_kind["kind"] = Value::String("OTHER".to_owned());
    let wrong_kind = serde_json::to_string(&wrong_kind).map_err(|error| error.to_string())?;
    assert!(request::parse(&wrong_kind).is_err());
    Ok(())
}

#[test]
fn profile_bridge_release_requires_exact_windows_evidence_pair() -> Result<(), String> {
    let mut missing = request_value();
    missing["evidence_artifacts"]
        .as_object_mut()
        .ok_or_else(|| "evidence map missing".to_owned())?
        .remove("windows_provenance");
    let input = serde_json::to_string(&missing).map_err(|error| error.to_string())?;
    let error = finalize_json(&root(), &input)
        .err()
        .ok_or_else(|| "missing Windows provenance unexpectedly finalized".to_owned())?;
    assert!(
        error
            .to_string()
            .contains("exactly Windows SBOM and provenance")
    );

    let mut wrong_path = request_value();
    wrong_path["evidence_artifacts"]["windows_sbom"]["path"] =
        Value::String("other.json".to_owned());
    let input = serde_json::to_string(&wrong_path).map_err(|error| error.to_string())?;
    let error = finalize_json(&root(), &input)
        .err()
        .ok_or_else(|| "wrong Windows SBOM path unexpectedly finalized".to_owned())?;
    assert!(error.to_string().contains("identity mismatch"));
    Ok(())
}

#[test]
fn independent_component_identity_reaches_pure_core_fail_closed() -> Result<(), String> {
    let mut wrong_source = request_value();
    wrong_source["components"]["runtime_bundle"]["source_commit_sha"] =
        Value::String("2222222222222222222222222222222222222222".to_owned());
    let input = serde_json::to_string(&wrong_source).map_err(|error| error.to_string())?;
    let error = finalize_json(&root(), &input)
        .err()
        .ok_or_else(|| "component source mismatch unexpectedly finalized".to_owned())?;
    assert!(error.to_string().contains("SOURCE_IDENTITY_MISMATCH"));

    let mut wrong_component = request_value();
    wrong_component["components"]["frontend"]["component_id"] =
        Value::String("control_plane".to_owned());
    let input = serde_json::to_string(&wrong_component).map_err(|error| error.to_string())?;
    let error = finalize_json(&root(), &input)
        .err()
        .ok_or_else(|| "component key mismatch unexpectedly finalized".to_owned())?;
    assert!(
        error
            .to_string()
            .contains("component identity key mismatch")
    );
    Ok(())
}

#[test]
fn conflicting_shared_artifact_observations_fail_in_pure_core() -> Result<(), String> {
    let mut request = request_value();
    request["components"]["frontend"]["artifact_sha256"] = Value::String(digest('9'));
    let input = serde_json::to_string(&request).map_err(|error| error.to_string())?;
    let error = finalize_json(&root(), &input)
        .err()
        .ok_or_else(|| "conflicting artifact observations unexpectedly finalized".to_owned())?;
    assert!(error.to_string().contains("ARTIFACT_INVENTORY_MISMATCH"));
    Ok(())
}

#[test]
fn transport_is_typed_before_composition() -> Result<(), String> {
    let request = request::parse(&request_json()?).map_err(|error| error.to_string())?;
    assert_eq!(request.schema_version, 1);
    assert_eq!(request.components.len(), 5);
    assert_eq!(request.evidence_artifacts.len(), 2);
    assert_eq!(request.components["frontend"].component_id, "frontend");
    assert_eq!(request.components["runtime_bundle"].source_commit_sha, GIT);
    assert_eq!(
        request.evidence_artifacts["windows_sbom"].path,
        WINDOWS_SBOM_PATH
    );
    Ok(())
}
