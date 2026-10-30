//! Real shipping executable, captured like a test/installer caller: failures must terminate without
//! a modal desktop dependency and must not print handoff capabilities. No backend/key effects.

use std::process::{Command, Output, Stdio};

fn captured(arguments: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_profile-bridge"));
    command.env_clear().args(arguments).stdin(Stdio::null());
    command.output()
}

#[test]
fn captured_invalid_handoff_exits_with_safe_actionable_diagnosis()
-> Result<(), Box<dyn std::error::Error>> {
    let capability = "DO_NOT_PRINT_THIS_HANDOFF_CAPABILITY";
    let argument = format!("profilebridge://claim/{capability}?invalid=true");
    let output = captured(&[&argument])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("BRIDGE_INVALID_HANDOFF"));
    assert!(stderr.contains("Return to Profile Operations"));
    assert!(!stderr.contains(capability));
    assert!(!stderr.contains("profilebridge://"));
    Ok(())
}

#[test]
fn standalone_start_returns_to_the_application_without_cli_configuration_instructions()
-> Result<(), Box<dyn std::error::Error>> {
    let output = captured(&[])?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("BRIDGE_OPEN_FROM_APPLICATION"));
    assert!(stderr.contains("starts from the application"));
    assert!(!stderr.contains("PROFILE_BRIDGE_"));
    Ok(())
}

#[test]
fn preparation_captures_its_typed_owner_failure_without_disclosing_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let private_path =
        std::env::temp_dir().join(format!("DO_NOT_LEAK_INSTALL_PATH-{}", std::process::id()));
    assert!(!private_path.exists());
    let path = private_path.to_str().ok_or("non UTF8 temporary path")?;
    let output = captured(&["--delivery-prepare", path, path])?;
    assert_eq!(output.status.code(), Some(2));
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(outcome["schema_version"], 1);
    assert_eq!(outcome["result"], "FAILED");
    assert_eq!(outcome["activation_performed"], false);
    assert_eq!(outcome["health_verified"], false);
    assert!(outcome["owner_verdict"].as_str().is_some());
    assert!(!String::from_utf8(output.stdout)?.contains("DO_NOT_LEAK"));
    assert!(!String::from_utf8(output.stderr)?.contains("DO_NOT_LEAK"));
    assert!(!private_path.exists());
    Ok(())
}

#[cfg(windows)]
#[test]
fn fresh_machine_pairing_fails_before_key_or_network_effects_with_a_setup_diagnosis()
-> Result<(), Box<dyn std::error::Error>> {
    let output = captured(&["profilebridge://pair/start/tenant_01JDIAG/device_01JDIAG"])?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("BRIDGE_SETUP_INCOMPLETE"));
    assert!(stderr.contains("not ready to open profiles"));
    assert!(!stderr.contains("tenant_01JDIAG"));
    assert!(!stderr.contains("device_01JDIAG"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn installer_entry_rejects_an_unpackaged_executable_before_installation_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let output = captured(&["--delivery-install-staged", "https://control.example.com"])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("BRIDGE_INSTALLATION_FILES_INVALID"));
    assert!(!stderr.contains("PROFILE_BRIDGE_"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn failed_native_activation_does_not_publish_connection_settings()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    let root = std::env::temp_dir().join(format!("bridge-install-no-state-{}", std::process::id()));
    fs::create_dir(&root)?;
    let executable = root.join("releases/test-candidate/profile-bridge/profile-bridge.exe");
    fs::create_dir_all(executable.parent().ok_or("missing executable parent")?)?;
    fs::copy(env!("CARGO_BIN_EXE_profile-bridge"), &executable)?;
    let output = Command::new(executable)
        .env_clear()
        .args(["--delivery-install-staged", "https://control.example.com"])
        .stdin(Stdio::null())
        .output()?;
    let settings_created = root.join("connection.json").exists();
    let binding_created = root.join("device-application-binding.json").exists();
    let canonical_root = fs::canonicalize(&root)?;
    if canonical_root.parent() != Some(fs::canonicalize(std::env::temp_dir())?.as_path()) {
        return Err("test cleanup target escaped temporary root".into());
    }
    fs::remove_dir_all(canonical_root)?;
    assert_eq!(output.status.code(), Some(2));
    assert!(!settings_created);
    assert!(!binding_created);
    assert!(String::from_utf8(output.stderr)?.contains("BRIDGE_INSTALLATION_INCOMPLETE"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn s1_shipping_executable_is_read_only_for_missing_original_profile_root()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    let parent = std::env::temp_dir().join(format!(
        "s1-shipping-no-effects-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let original = parent.join("installed");
    let bundle = parent.join("candidate-inputs");
    fs::create_dir_all(&original)?;
    fs::create_dir(&bundle)?;
    let sentinel = original.join("do-not-change-original.txt");
    fs::write(&sentinel, b"original-only-copy")?;
    let original_path = original.to_str().ok_or("invalid original path")?;
    let bundle_path = bundle.to_str().ok_or("invalid bundle path")?;

    let output = captured(&["--s1-observe", bundle_path, original_path])?;
    assert_eq!(output.status.code(), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["verdict"], "UNKNOWN");
    assert!(report["reason_codes"]
        .as_array()
        .is_some_and(|reasons| reasons.iter().any(|r| r == "PROFILE_ROOT_UNVERIFIED")));
    assert!(String::from_utf8(output.stderr)?.contains("BRIDGE_S1_NOT_READY"));
    assert!(!original.join("profiles").exists());
    assert!(!original.join("state").exists());
    assert!(!original.join("releases").exists());
    assert_eq!(fs::read(&sentinel)?, b"original-only-copy");

    let copy = parent.join("not-existing-isolated-copy");
    let copy_path = copy.to_str().ok_or("invalid copy path")?;
    let output = captured(&["--s1-observe", bundle_path, original_path, copy_path])?;
    assert_eq!(output.status.code(), Some(3));
    assert!(!copy.exists());
    assert_eq!(fs::read(&sentinel)?, b"original-only-copy");
    let bytes = fs::read_dir(&original)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(bytes.len(), 1, "read-only CLI created unexpected original data");

    fs::remove_dir_all(parent)?;
    Ok(())
}
