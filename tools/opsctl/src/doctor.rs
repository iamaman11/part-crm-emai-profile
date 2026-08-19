use crate::OpsctlError;
use crate::repository::canonical_json_document;
use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    for relative in [
        "docs/status.json",
        "architecture/inventory.json",
        "architecture/python-estate-ar6.json",
        "architecture/credential-authority.json",
        "architecture/credential-lifecycle.json",
        "architecture/profile-security.json",
        "architecture/operator-contract.json",
        "scripts/generate-architecture-inventory.py",
        "scripts/python-estate-ar6.py",
    ] {
        if !root.join(relative).is_file() {
            return Err(OpsctlError::new(
                "doctor",
                format!("required canonical authority is missing: {relative}"),
            ));
        }
    }

    for relative in [
        "architecture/credential-authority.json",
        "architecture/credential-lifecycle.json",
        "architecture/profile-security.json",
        "architecture/operator-contract.json",
    ] {
        let _ = canonical_json_document(root, relative, "doctor")?;
    }

    run_python_check(
        root,
        "scripts/generate-architecture-inventory.py",
        &["--check"],
    )?;
    run_python_check(root, "scripts/python-estate-ar6.py", &["--check"])?;
    Ok("{\"schema_version\":1,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"read-only\",\"mutation_executed\":false,\"authorities\":[\"architecture/inventory.json\",\"architecture/python-estate-ar6.json\",\"architecture/credential-authority.json\",\"architecture/credential-lifecycle.json\",\"architecture/profile-security.json\",\"architecture/operator-contract.json\",\"docs/status.json\"]}\n".to_owned())
}

fn run_python_check(root: &Path, script: &str, arguments: &[&str]) -> Result<(), OpsctlError> {
    let python = env::var_os("OPSCTL_PYTHON").unwrap_or_else(|| OsString::from("python"));
    let output = Command::new(&python)
        .arg(root.join(script))
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| OpsctlError::new("doctor", format!("cannot execute {script}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(OpsctlError::new(
        "doctor",
        format!("canonical validator failed: {script}: {detail}"),
    ))
}
