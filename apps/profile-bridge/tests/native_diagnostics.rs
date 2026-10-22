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
