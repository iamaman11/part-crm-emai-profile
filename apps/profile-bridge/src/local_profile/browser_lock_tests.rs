use super::test_support::{cleanup, ids, test_root};
use super::{BridgeWorkspaceLock, LocalProfileError, MaterializationRoot};
use profile_platform_primitives::{DeviceId, GenerationId};
use std::fs;

#[test]
fn bridge_lock_is_exclusive_and_preserves_browser_lock_files()
-> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("locks")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8B")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    fs::write(workspace.path().join(".parentlock"), b"browser-owned")?;
    fs::write(workspace.path().join("lock"), b"browser-owned")?;
    let device_id = DeviceId::parse("device_01JSTEP8")?;
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device_id, 1)?;
    assert!(matches!(
        BridgeWorkspaceLock::acquire(&workspace, &device_id, 2),
        Err(LocalProfileError::LockBusy)
    ));
    lock.release()?;
    assert!(workspace.path().join(".parentlock").exists());
    assert!(workspace.path().join("lock").exists());
    let second = BridgeWorkspaceLock::acquire(&workspace, &device_id, 2)?;
    second.release()?;
    cleanup(root_path)?;
    Ok(())
}

#[test]
fn inventory_is_deterministic_and_includes_browser_owned_locks()
-> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("inventory")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8C")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    fs::create_dir(workspace.path().join("storage"))?;
    fs::write(workspace.path().join("z.txt"), b"z")?;
    fs::write(workspace.path().join("storage/a.bin"), b"alpha")?;
    fs::write(workspace.path().join(".parentlock"), b"present")?;
    let first = workspace.inventory()?;
    let second = workspace.inventory()?;
    assert_eq!(first, second);
    assert_eq!(first.entries().len(), 3);
    assert!(
        first
            .entries()
            .iter()
            .any(|entry| entry.relative_path() == ".parentlock")
    );
    cleanup(root_path)?;
    Ok(())
}
