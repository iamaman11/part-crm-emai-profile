use std::path::PathBuf;

const RETIRED_SEMANTIC_INPUTS: [&str; 3] = [
    "architecture/architecture-fitness-policy.json",
    "architecture/inventory.json",
    "scripts/_ar3_application_architecture.py",
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
