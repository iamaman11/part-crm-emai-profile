use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn an_empty_store_without_a_verified_staged_candidate_is_not_installed() -> TestResult {
    let directory = TestDirectory::create("first-install-unprepared")?;
    let releases = directory.0.join(RELEASES_DIRECTORY);
    fs::create_dir(&releases)?;
    let staging = DeliveryStagingRoot::open_or_create(releases)?;
    let state_root = directory.0.join(STATE_DIRECTORY);
    let before = DeliveryStateStore::initialize(&state_root)?;
    let materialization = MaterializationRoot::open_or_create(directory.0.join("profiles"))?;
    let guard = DeliveryActivationGuard::acquire(&materialization)?;
    let coordinator = DeliveryHandoffCoordinator::new(staging, &state_root);
    assert_eq!(
        coordinator.complete_first_installation(&guard, directory.0.join("absent.exe")),
        Err(DeliveryHandoffError::NoStagedTarget)
    );
    let after = DeliveryStateStore::open(&state_root)?;
    assert_eq!(after.state(), before.state());
    assert!(after.handoff().is_none());
    Ok(())
}

#[test]
fn first_installation_records_actual_target_arrival_with_health_still_pending() -> TestResult {
    let fixture = first_install_prepared_fixture("first-install-arrival")?;
    let coordinator = DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
    let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
    let scheduled = Cell::new(false);
    // Update admission still requires a real healthy predecessor; first install does not fake one.
    assert_eq!(
        coordinator.start_activation_with(&guard, 42, &fixture.first_executable, |_| {
            scheduled.set(true);
            Ok(())
        }),
        Err(DeliveryHandoffError::NoActiveSource)
    );
    assert!(!scheduled.get());

    coordinator.complete_first_installation(&guard, &fixture.first_executable)?;
    let reopened = DeliveryStateStore::open(&fixture.state_root)?;
    assert_eq!(reopened.state().active(), Some(&fixture.first));
    assert!(reopened.state().staged().is_none());
    assert!(reopened.state().last_known_good().is_none());
    assert!(!reopened.state().active_health_confirmed());
    assert_eq!(reopened.state().activation_generation(), 1);
    assert_eq!(
        reopened.state().last_activation().map(|v| v.outcome),
        Some(DeliveryActivationOutcome::PendingHealth)
    );
    let handoff = reopened.handoff().ok_or("missing first-install arrival")?;
    assert_eq!(handoff.outcome(), DeliveryHandoffOutcome::Arrived);
    assert_eq!(handoff.target(), &fixture.first);
    assert!(!coordinator.restart_recovery_pending()?);
    Ok(())
}

#[test]
fn exact_pending_and_healthy_install_retries_append_no_journal_snapshot() -> TestResult {
    let fixture = first_install_prepared_fixture("first-install-retry")?;
    let coordinator = DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
    let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
    coordinator.complete_first_installation(&guard, &fixture.first_executable)?;
    for healthy in [false, true] {
        if healthy {
            let mut store = DeliveryStateStore::open(&fixture.state_root)?;
            let mut next = store.state().clone();
            next.start_health_attempt(&fixture.first, 1)?;
            next.confirm_health()?;
            store.persist_preserving_handoff(&next)?;
        }
        let before = DeliveryStateStore::open(&fixture.state_root)?;
        let count = fs::read_dir(&fixture.state_root)?.count();
        coordinator.complete_first_installation(&guard, &fixture.first_executable)?;
        let after = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(after.state(), before.state());
        assert_eq!(after.handoff(), before.handoff());
        assert_eq!(fs::read_dir(&fixture.state_root)?.count(), count);
    }
    Ok(())
}

#[test]
fn first_installation_cannot_reset_an_interrupted_runtime_health_attempt() -> TestResult {
    let fixture = first_install_prepared_fixture("first-install-interrupted-health")?;
    let coordinator = DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
    let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
    coordinator.complete_first_installation(&guard, &fixture.first_executable)?;
    let mut store = DeliveryStateStore::open(&fixture.state_root)?;
    let mut next = store.state().clone();
    next.start_health_attempt(&fixture.first, 1)?;
    store.persist_preserving_handoff(&next)?;
    assert_eq!(
        coordinator.complete_first_installation(&guard, &fixture.first_executable),
        Err(DeliveryHandoffError::FirstInstallationConflict)
    );
    assert_eq!(
        DeliveryStateStore::open(&fixture.state_root)?.state(),
        &next
    );
    Ok(())
}

#[test]
fn a_different_or_tampered_executable_never_publishes_first_activation() -> TestResult {
    for tampered in [false, true] {
        let fixture =
            first_install_prepared_fixture(if tampered { "tamper" } else { "wrong-process" })?;
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        let before = DeliveryStateStore::open(&fixture.state_root)?;
        let current = if tampered {
            fs::write(&fixture.first_executable, b"tampered-bridge")?;
            fixture.first_executable.clone()
        } else {
            let path = fixture._directory.0.join("unrelated-profile-bridge.exe");
            fs::write(&path, b"unrelated-bridge")?;
            path
        };
        assert!(
            coordinator
                .complete_first_installation(&guard, &current)
                .is_err()
        );
        let after = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(after.state(), before.state());
        assert_eq!(after.handoff(), before.handoff());
    }
    Ok(())
}

#[test]
fn first_installation_cannot_replace_update_or_recovery_history() -> TestResult {
    for started in [false, true] {
        let fixture = fixture(
            if started {
                "install-over-pending"
            } else {
                "install-over-active"
            },
            started,
        )?;
        let coordinator =
            DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
        let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
        let before = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(
            coordinator.complete_first_installation(&guard, &fixture.second_executable),
            Err(DeliveryHandoffError::FirstInstallationConflict)
        );
        let after = DeliveryStateStore::open(&fixture.state_root)?;
        assert_eq!(after.state(), before.state());
        assert_eq!(after.handoff(), before.handoff());
    }
    Ok(())
}

#[test]
fn first_installation_cannot_complete_an_unfinished_process_transfer() -> TestResult {
    let fixture = first_install_started_fixture("first-install-pending-transfer")?;
    let coordinator = DeliveryHandoffCoordinator::new(fixture.staging.clone(), &fixture.state_root);
    let guard = DeliveryActivationGuard::acquire(&fixture.materialization)?;
    let before = DeliveryStateStore::open(&fixture.state_root)?;
    assert_eq!(
        coordinator.complete_first_installation(&guard, &fixture.first_executable),
        Err(DeliveryHandoffError::FirstInstallationConflict)
    );
    let after = DeliveryStateStore::open(&fixture.state_root)?;
    assert_eq!(after.state(), before.state());
    assert_eq!(after.handoff(), before.handoff());
    Ok(())
}

#[test]
fn installed_workspace_root_is_shared_with_the_activation_guard() -> TestResult {
    let fixture = first_install_prepared_fixture("installed-profile-root")?;
    let coordinator =
        DeliveryHandoffCoordinator::from_current_executable(&fixture.first_executable)?;
    let root = fs::canonicalize(coordinator.installed_profile_root()?)?;
    assert_eq!(
        root,
        fs::canonicalize(fixture._directory.0.join("profiles"))?
    );
    let invalid =
        DeliveryHandoffCoordinator::new(fixture.staging, fixture.state_root.join("elsewhere"));
    assert_eq!(
        invalid.installed_profile_root(),
        Err(DeliveryHandoffError::InvalidInstalledLayout)
    );
    Ok(())
}
