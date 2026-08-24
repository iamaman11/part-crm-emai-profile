use std::fs;
use std::path::PathBuf;

const RETIRED_SEMANTIC_INPUTS: [&str; 3] = [
    "architecture/architecture-fitness-policy.json",
    "architecture/inventory.json",
    "scripts/_ar3_application_architecture.py",
];

const DOCTOR_FORBIDDEN_SEMANTIC_MARKERS: [&str; 6] = [
    "AUTHORITIES",
    "INTERNAL_NATIVE_IMPLEMENTATION_CONTRACT",
    "canonical_json_document",
    "serde_json::Value",
    "architecture/inventory.json",
    "architecture/operator-contract.json",
];

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn retired_semantic_inputs_stay_absent() {
    let root = repository_root();
    for relative in RETIRED_SEMANTIC_INPUTS {
        assert!(
            !root.join(relative).exists(),
            "retired/manual architecture semantic input must stay absent: {relative}"
        );
    }
}

#[test]
fn doctor_semantic_composition_stays_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(repository_root().join("tools/opsctl/src/doctor.rs"))?;
    let production = source
        .split_once("#[cfg(test)]")
        .map_or(source.as_str(), |(production, _)| production);

    for forbidden in DOCTOR_FORBIDDEN_SEMANTIC_MARKERS {
        assert!(
            !production.contains(forbidden),
            "doctor production source restored retired/generic semantic authority marker: {forbidden}"
        );
    }
    Ok(())
}
