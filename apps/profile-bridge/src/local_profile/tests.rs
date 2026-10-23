use super::{
    BridgeWorkspaceLock, ForgottenWindowAction, ForgottenWindowPolicy, LocalGenerationRecord,
    LocalGenerationState, LocalProfileError, MaterializationRoot, QuotaPolicy, RecoveryClone,
    SupportBundleSummary,
};
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId, UnixMillis};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_root(label: &str) -> Result<PathBuf, LocalProfileError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LocalProfileError::ClockRegression)?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "part-crm-step8-{label}-{}-{nonce}",
        std::process::id()
    )))
}

fn ids() -> Result<(TenantId, ProfileId), Box<dyn std::error::Error>> {
    Ok((
        TenantId::parse("tenant_01JSTEP8")?,
        ProfileId::parse("profile_01JSTEP8")?,
    ))
}

#[test]
fn materialization_root_builds_opaque_generation_paths() -> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("paths")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8A")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    assert!(workspace.path().starts_with(root.path()));
    let relative = workspace.path().strip_prefix(root.path())?;
    assert_eq!(relative.components().count(), 3);
    assert_eq!(relative.to_string_lossy().len(), 130);
    assert!(!workspace.path().to_string_lossy().contains('@'));
    fs::remove_dir_all(root_path)?;
    Ok(())
}

#[test]
fn compact_workspace_binds_every_id_and_preserves_existing_user_directories()
-> Result<(), Box<dyn std::error::Error>> {
    let path = test_root("compact")?;
    let root = MaterializationRoot::open_or_create(&path)?;
    let tenant = TenantId::parse(format!("tenant_{}", "a".repeat(64)))?;
    let profile = ProfileId::parse(format!("profile_{}", "b".repeat(32)))?;
    let generation = GenerationId::parse(format!("generation_{}", "c".repeat(64)))?;
    let workspace = root.create_generation(&tenant, &profile, &generation)?;
    assert_eq!(
        workspace
            .path()
            .strip_prefix(root.path())?
            .to_string_lossy()
            .len(),
        130
    );
    assert_eq!(
        root.open_generation(&tenant, &profile, &generation)?,
        workspace
    );
    for (other_tenant, other_profile, other_generation) in [
        (
            TenantId::parse("tenant_different")?,
            profile.clone(),
            generation.clone(),
        ),
        (
            tenant.clone(),
            ProfileId::parse("profile_different")?,
            generation.clone(),
        ),
        (
            tenant.clone(),
            profile.clone(),
            GenerationId::parse("generation_different")?,
        ),
    ] {
        let other = root.create_generation(&other_tenant, &other_profile, &other_generation)?;
        assert_ne!(other.path(), workspace.path());
        if other_tenant == tenant && other_profile == profile {
            assert_eq!(other.path().parent(), workspace.path().parent());
        }
    }
    let legacy_id = GenerationId::parse("generation_existing_user_data")?;
    let legacy = root
        .path()
        .join(tenant.as_str())
        .join(profile.as_str())
        .join(legacy_id.as_str());
    fs::create_dir_all(&legacy)?;
    fs::write(
        legacy.join(".profile-generation"),
        "profile-platform-generation-v1\n",
    )?;
    fs::write(legacy.join("unsaved.bin"), b"preserve")?;
    assert_eq!(
        root.open_generation(&tenant, &profile, &legacy_id)?.path(),
        legacy
    );
    assert_eq!(
        root.create_generation(&tenant, &profile, &legacy_id),
        Err(LocalProfileError::TargetAlreadyExists)
    );
    assert_eq!(fs::read(legacy.join("unsaved.bin"))?, b"preserve");
    fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn bridge_lock_rejects_ownership_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("lock-tamper")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8M")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    let device_id = DeviceId::parse("device_01JSTEP8")?;
    let lock = BridgeWorkspaceLock::acquire(&workspace, &device_id, 1)?;
    let lock_path = workspace.path().join(".profile-platform.lock");
    fs::write(&lock_path, b"tampered-lock")?;
    assert_eq!(
        lock.release(),
        Err(LocalProfileError::LockOwnershipMismatch)
    );
    assert!(lock_path.exists());
    fs::remove_file(lock_path)?;
    fs::remove_dir_all(root_path)?;
    Ok(())
}

#[test]
fn recovery_is_clone_only_and_detects_clone_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("recovery")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let source_id = GenerationId::parse("generation_01JSTEP8D")?;
    let recovery_id = GenerationId::parse("generation_01JSTEP8E")?;
    let source = root.create_generation(&tenant_id, &profile_id, &source_id)?;
    fs::create_dir(source.path().join("storage"))?;
    fs::write(source.path().join("storage/state.bin"), b"accepted-state")?;
    let source_before = source.inventory()?;
    let recovery = RecoveryClone::create(&source, &root, &tenant_id, &profile_id, &recovery_id)?;
    assert_eq!(recovery.verify_clone_only()?, source_before);
    fs::write(
        recovery.workspace().path().join("storage/state.bin"),
        b"mutated-clone",
    )?;
    assert_eq!(
        recovery.verify_clone_only(),
        Err(LocalProfileError::CloneChanged)
    );
    assert_eq!(source.inventory()?, source_before);
    fs::remove_dir_all(root_path)?;
    Ok(())
}

#[test]
fn recovery_preserves_browser_state_but_not_ephemeral_firefox_locks()
-> Result<(), Box<dyn std::error::Error>> {
    let root_path = test_root("browser-state-recovery")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let source_id = GenerationId::parse("generation_01JSTEP8P")?;
    let recovery_id = GenerationId::parse("generation_01JSTEP8Q")?;
    let source = root.create_generation(&tenant_id, &profile_id, &source_id)?;
    let user_data = source.path().join("user_data");
    fs::create_dir(&user_data)?;
    fs::write(
        user_data.join("cookies.sqlite"),
        b"persistent-browser-state",
    )?;
    fs::write(user_data.join("lock"), b"ephemeral-firefox-legacy-lock")?;
    fs::write(
        user_data.join(".parentlock"),
        b"ephemeral-firefox-posix-lock-marker",
    )?;
    fs::write(
        user_data.join("parent.lock"),
        b"ephemeral-firefox-windows-lock-marker",
    )?;

    let materialization_before = source.materialization_inventory_digest()?;
    let source_inventory = source.inventory()?;
    assert!(
        source_inventory
            .entries()
            .iter()
            .any(|entry| entry.relative_path() == "user_data/cookies.sqlite")
    );
    for ephemeral in [
        "user_data/lock",
        "user_data/.parentlock",
        "user_data/parent.lock",
    ] {
        assert!(
            source_inventory
                .entries()
                .iter()
                .all(|entry| entry.relative_path() != ephemeral)
        );
    }

    let recovery = RecoveryClone::create(&source, &root, &tenant_id, &profile_id, &recovery_id)?;
    assert_eq!(
        fs::read(recovery.workspace().path().join("user_data/cookies.sqlite"))?,
        b"persistent-browser-state"
    );
    for ephemeral in ["lock", ".parentlock", "parent.lock"] {
        assert!(
            !recovery
                .workspace()
                .path()
                .join("user_data")
                .join(ephemeral)
                .exists()
        );
    }
    assert_eq!(
        recovery.workspace().materialization_inventory_digest()?,
        materialization_before
    );
    fs::remove_dir_all(root_path)?;
    Ok(())
}

#[test]
fn forgotten_window_policy_progresses_warn_drain_force_close()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = ForgottenWindowPolicy::new(100, 200, 500)?;
    let mut generation = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8F")?,
        10,
        UnixMillis::new(1_000),
    );
    generation.set_locked(true)?;
    generation.begin_use(UnixMillis::new(1_000))?;
    assert_eq!(
        policy.evaluate(&generation, UnixMillis::new(1_099))?,
        ForgottenWindowAction::None
    );
    assert_eq!(
        policy.evaluate(&generation, UnixMillis::new(1_100))?,
        ForgottenWindowAction::Warn
    );
    assert_eq!(
        policy.evaluate(&generation, UnixMillis::new(1_200))?,
        ForgottenWindowAction::Drain
    );
    assert_eq!(
        policy.evaluate(&generation, UnixMillis::new(1_500))?,
        ForgottenWindowAction::ForceClose
    );
    Ok(())
}

#[test]
fn quota_never_selects_dirty_recovery_in_use_or_locked_generation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut dirty = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8G")?,
        80,
        UnixMillis::new(10),
    );
    dirty.set_locked(true)?;
    dirty.begin_use(UnixMillis::new(11))?;
    dirty.graceful_close(UnixMillis::new(12))?;
    dirty.set_locked(false)?;

    let mut recovery = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8H")?,
        70,
        UnixMillis::new(20),
    );
    recovery.set_locked(true)?;
    recovery.begin_use(UnixMillis::new(21))?;
    recovery.observe_crash(UnixMillis::new(22))?;

    let mut in_use = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8O")?,
        40,
        UnixMillis::new(30),
    );
    in_use.set_locked(true)?;
    in_use.begin_use(UnixMillis::new(31))?;

    let mut eligible = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8I")?,
        60,
        UnixMillis::new(1),
    );
    eligible.set_locked(true)?;
    eligible.begin_use(UnixMillis::new(2))?;
    eligible.graceful_close(UnixMillis::new(3))?;
    eligible.set_locked(false)?;
    eligible.mark_synced(UnixMillis::new(4))?;

    let mut locked_synced = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8J")?,
        50,
        UnixMillis::new(5),
    );
    locked_synced.set_locked(true)?;
    locked_synced.begin_use(UnixMillis::new(6))?;
    locked_synced.graceful_close(UnixMillis::new(7))?;
    locked_synced.mark_synced(UnixMillis::new(8))?;

    let records = [dirty, recovery, in_use, eligible.clone(), locked_synced];
    let plan = QuotaPolicy::new(240)?.plan(&records)?;
    assert_eq!(plan.total_bytes(), 300);
    assert_eq!(plan.bytes_to_reclaim(), 60);
    assert_eq!(plan.reclaimable_bytes(), 60);
    assert!(plan.is_satisfied());
    assert_eq!(plan.candidates(), [eligible.generation_id().clone()]);
    assert_eq!(records[0].state(), LocalGenerationState::DirtyLocal);
    assert_eq!(records[1].state(), LocalGenerationState::RecoveryRequired);
    assert_eq!(records[2].state(), LocalGenerationState::InUse);
    Ok(())
}

#[test]
fn support_summary_contains_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
    let record = LocalGenerationRecord::new(
        GenerationId::parse("generation_01JSTEP8K")?,
        42,
        UnixMillis::new(1),
    );
    let rendered = SupportBundleSummary::from_records(&[record], 2)?.render_metadata_only();
    assert!(rendered.contains("total_generations=1"));
    assert!(rendered.contains("total_bytes=42"));
    assert!(rendered.contains("inventory_failures=2"));
    assert!(!rendered.contains("generation_01JSTEP8K"));
    assert!(!rendered.contains("user@example.com"));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains('\\'));
    Ok(())
}

#[cfg(unix)]
#[test]
fn generation_marker_rejects_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let root_path = test_root("marker-symlink")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8N")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    let marker = workspace.path().join(".profile-generation");
    let external = root.path().join("external-marker");
    fs::write(&external, b"profile-platform-generation-v1\n")?;
    fs::remove_file(&marker)?;
    symlink(&external, &marker)?;
    assert_eq!(
        root.open_generation(&tenant_id, &profile_id, &generation_id),
        Err(LocalProfileError::SymbolicLinkRejected)
    );
    fs::remove_dir_all(root_path)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn inventory_rejects_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let root_path = test_root("symlink")?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let (tenant_id, profile_id) = ids()?;
    let generation_id = GenerationId::parse("generation_01JSTEP8L")?;
    let workspace = root.create_generation(&tenant_id, &profile_id, &generation_id)?;
    fs::write(workspace.path().join("target.txt"), b"target")?;
    symlink("target.txt", workspace.path().join("link.txt"))?;
    assert_eq!(
        workspace.inventory(),
        Err(LocalProfileError::SymbolicLinkRejected)
    );
    fs::remove_dir_all(root_path)?;
    Ok(())
}
