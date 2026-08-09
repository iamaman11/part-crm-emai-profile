#!/usr/bin/env python3
from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"{path}: expected exactly one {label} marker, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


# Pure browser execution policy: only browser-owned lock artifacts belong here.
domain = Path("crates/browser-execution-domain/src/lib.rs")
replace_once(
    domain,
    '''pub struct BrowserWriterObservation {
    bridge_lock_present: bool,
    parent_lock_present: bool,
    browser_lock_present: bool,
    supervised_writer_active: bool,
}

impl BrowserWriterObservation {
    #[must_use]
    pub const fn new(
        bridge_lock_present: bool,
        parent_lock_present: bool,
        browser_lock_present: bool,
        supervised_writer_active: bool,
    ) -> Self {
        Self {
            bridge_lock_present,
            parent_lock_present,
            browser_lock_present,
            supervised_writer_active,
        }
    }

    #[must_use]
    pub const fn classify(self) -> BrowserWriterDecision {
        if self.supervised_writer_active {
            return BrowserWriterDecision::ProfileBusy;
        }
        if self.bridge_lock_present || self.parent_lock_present || self.browser_lock_present {
            return BrowserWriterDecision::RecoveryRequired;
        }
        BrowserWriterDecision::Ready
    }
}''',
    '''pub struct BrowserWriterObservation {
    parent_lock_present: bool,
    browser_lock_present: bool,
    supervised_writer_active: bool,
}

impl BrowserWriterObservation {
    #[must_use]
    pub const fn new(
        parent_lock_present: bool,
        browser_lock_present: bool,
        supervised_writer_active: bool,
    ) -> Self {
        Self {
            parent_lock_present,
            browser_lock_present,
            supervised_writer_active,
        }
    }

    #[must_use]
    pub const fn classify(self) -> BrowserWriterDecision {
        if self.supervised_writer_active {
            return BrowserWriterDecision::ProfileBusy;
        }
        if self.parent_lock_present || self.browser_lock_present {
            return BrowserWriterDecision::RecoveryRequired;
        }
        BrowserWriterDecision::Ready
    }
}''',
    "BrowserWriterObservation block",
)
text = domain.read_text(encoding="utf-8")
replacements = {
    "BrowserWriterObservation::new(false, false, false, false)": "BrowserWriterObservation::new(false, false, false)",
    "BrowserWriterObservation::new(false, true, false, false)": "BrowserWriterObservation::new(true, false, false)",
    "BrowserWriterObservation::new(true, false, false, false)": "BrowserWriterObservation::new(false, true, false)",
    "BrowserWriterObservation::new(true, true, true, true)": "BrowserWriterObservation::new(true, true, true)",
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"{domain}: expected one writer test marker {old!r}")
    text = text.replace(old, new, 1)
domain.write_text(text, encoding="utf-8")

# Existing local lock remains the single local ownership mechanism. Add exact proof.
filesystem = Path("apps/profile-bridge/src/local_profile/filesystem.rs")
replace_once(
    filesystem,
    '''        let lock_path = workspace.path().join(BRIDGE_LOCK_FILE);
        let ownership = format!(
            "profile-platform-bridge-lock-v1\\n{}\\n{epoch}\\n",
            device_id.as_str()
        );''',
    '''        let lock_path = workspace.path().join(BRIDGE_LOCK_FILE);
        let ownership = render_bridge_lock_ownership(device_id, epoch);''',
    "lock ownership rendering",
)
replace_once(
    filesystem,
    '''    pub fn release(self) -> Result<(), LocalProfileError> {
        if read_control_text(&self.lock_path)? != self.ownership {
            return Err(LocalProfileError::LockOwnershipMismatch);
        }
        fs::remove_file(self.lock_path)?;
        Ok(())
    }
}''',
    '''    pub fn verify_owned(
        &self,
        workspace: &GenerationWorkspace,
        expected_device_id: &DeviceId,
        expected_epoch: u64,
    ) -> Result<(), LocalProfileError> {
        if expected_epoch == 0 {
            return Err(LocalProfileError::InvalidPolicy);
        }
        let expected_lock_path = workspace.path().join(BRIDGE_LOCK_FILE);
        let expected_ownership = render_bridge_lock_ownership(expected_device_id, expected_epoch);
        if self.lock_path != expected_lock_path || self.ownership != expected_ownership {
            return Err(LocalProfileError::LockOwnershipMismatch);
        }
        if read_control_text(&self.lock_path)? != self.ownership {
            return Err(LocalProfileError::LockOwnershipMismatch);
        }
        Ok(())
    }

    pub fn release(self) -> Result<(), LocalProfileError> {
        if read_control_text(&self.lock_path)? != self.ownership {
            return Err(LocalProfileError::LockOwnershipMismatch);
        }
        fs::remove_file(self.lock_path)?;
        Ok(())
    }
}

fn render_bridge_lock_ownership(device_id: &DeviceId, epoch: u64) -> String {
    format!(
        "profile-platform-bridge-lock-v1\\n{}\\n{epoch}\\n",
        device_id.as_str()
    )
}''',
    "verify_owned insertion",
)

# Profile Bridge launch guard requires and re-verifies exact local ownership.
guard = Path("apps/profile-bridge/src/browser_execution.rs")
replace_once(
    guard,
    'use crate::local_profile::{GenerationWorkspace, LocalProfileError};',
    'use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, LocalProfileError};',
    "BridgeWorkspaceLock import",
)
replace_once(
    guard,
    'use profile_platform_primitives::{GenerationId, ProfileId, TenantId};',
    'use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};',
    "DeviceId import",
)
replace_once(
    guard,
    'const BRIDGE_LOCK_FILE: &str = ".profile-platform.lock";\n',
    '',
    "obsolete bridge lock constant",
)
replace_once(
    guard,
    '''pub fn evaluate_browser_launch(
    workspace: &GenerationWorkspace,
    expected: &MaterializationBinding,
    network_policy: &NetworkIdentityPolicy,
    network_observation: &NetworkIdentityObservation,
    supervised_writer_active: bool,
) -> Result<(), BrowserLaunchBlocker> {
    let actual = load_materialization_binding(''',
    '''pub fn evaluate_browser_launch(
    workspace: &GenerationWorkspace,
    bridge_lock: &BridgeWorkspaceLock,
    expected_device_id: &DeviceId,
    expected_workspace_epoch: u64,
    expected: &MaterializationBinding,
    network_policy: &NetworkIdentityPolicy,
    network_observation: &NetworkIdentityObservation,
    supervised_writer_active: bool,
) -> Result<(), BrowserLaunchBlocker> {
    verify_bridge_lock(
        bridge_lock,
        workspace,
        expected_device_id,
        expected_workspace_epoch,
    )?;
    let actual = load_materialization_binding(''',
    "launch signature and initial lease proof",
)
replace_once(
    guard,
    '''    let writer = BrowserWriterObservation::new(
        path_present(&workspace.path().join(BRIDGE_LOCK_FILE))?,
        path_present(&workspace.path().join(FIREFOX_PARENT_LOCK_FILE))?,
        path_present(&workspace.path().join(FIREFOX_LOCK_FILE))?,
        supervised_writer_active,
    );''',
    '''    let writer = BrowserWriterObservation::new(
        path_present(&workspace.path().join(FIREFOX_PARENT_LOCK_FILE))?,
        path_present(&workspace.path().join(FIREFOX_LOCK_FILE))?,
        supervised_writer_active,
    );''',
    "browser writer observation",
)
replace_once(
    guard,
    '''    match network_policy.evaluate(network_observation) {
        NetworkIdentityDecision::Accepted => Ok(()),
        NetworkIdentityDecision::RetryableRouteChurn => {
            Err(BrowserLaunchBlocker::RetryableNetworkRouteChurn)
        }
        NetworkIdentityDecision::OperatorRemediationRequired => {
            Err(BrowserLaunchBlocker::NetworkPolicyMismatch)
        }
    }
}''',
    '''    match network_policy.evaluate(network_observation) {
        NetworkIdentityDecision::Accepted => {
            verify_bridge_lock(
                bridge_lock,
                workspace,
                expected_device_id,
                expected_workspace_epoch,
            )?;
            Ok(())
        }
        NetworkIdentityDecision::RetryableRouteChurn => {
            Err(BrowserLaunchBlocker::RetryableNetworkRouteChurn)
        }
        NetworkIdentityDecision::OperatorRemediationRequired => {
            Err(BrowserLaunchBlocker::NetworkPolicyMismatch)
        }
    }
}

fn verify_bridge_lock(
    bridge_lock: &BridgeWorkspaceLock,
    workspace: &GenerationWorkspace,
    expected_device_id: &DeviceId,
    expected_workspace_epoch: u64,
) -> Result<(), BrowserLaunchBlocker> {
    bridge_lock
        .verify_owned(workspace, expected_device_id, expected_workspace_epoch)
        .map_err(|_| BrowserLaunchBlocker::RecoveryRequired)
}''',
    "second lease proof and helper",
)
replace_once(
    guard,
    '    use crate::local_profile::MaterializationRoot;',
    '    use crate::local_profile::{BridgeWorkspaceLock, MaterializationRoot};',
    "test lock import",
)
replace_once(
    guard,
    '    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};',
    '    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};',
    "test DeviceId import",
)

text = guard.read_text(encoding="utf-8")
# Add a valid local lock in each of the three existing launch tests.
workspace_markers = [
    ('let workspace = root.create_generation(&tenant, &profile, &generation)?;\n        fs::write(workspace.path().join("prefs.js"), b"accepted")?;',
     'let workspace = root.create_generation(&tenant, &profile, &generation)?;\n        let device = DeviceId::parse("device_01JLAUNCH")?;\n        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;\n        fs::write(workspace.path().join("prefs.js"), b"accepted")?;'),
    ('let workspace = root.create_generation(&tenant, &profile, &generation)?;\n        let binding = MaterializationBinding::new(',
     'let workspace = root.create_generation(&tenant, &profile, &generation)?;\n        let device = DeviceId::parse("device_01JLOCK")?;\n        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;\n        let binding = MaterializationBinding::new('),
]
# First marker occurs once; second occurs twice, so handle second by specific tenant context below.
old, new = workspace_markers[0]
if text.count(old) != 1:
    raise SystemExit("freshness test workspace marker changed")
text = text.replace(old, new, 1)

lock_old = '''        let generation = GenerationId::parse("generation_01JLOCK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let binding = MaterializationBinding::new('''
lock_new = '''        let generation = GenerationId::parse("generation_01JLOCK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let device = DeviceId::parse("device_01JLOCK")?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new('''
if text.count(lock_old) != 1:
    raise SystemExit("browser-lock test workspace marker changed")
text = text.replace(lock_old, lock_new, 1)

network_old = '''        let generation = GenerationId::parse("generation_01JNETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let binding = MaterializationBinding::new('''
network_new = '''        let generation = GenerationId::parse("generation_01JNETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let device = DeviceId::parse("device_01JNETWORK")?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new('''
if text.count(network_old) != 1:
    raise SystemExit("network test workspace marker changed")
text = text.replace(network_old, network_new, 1)

# Every existing launch call now carries exact local ownership arguments.
call_old = '''            &workspace,
            &binding,'''
call_new = '''            &workspace,
            &bridge_lock,
            &device,
            1,
            &binding,'''
count = text.count(call_old)
if count != 4:
    raise SystemExit(f"expected four existing evaluate calls, found {count}")
text = text.replace(call_old, call_new)

# Release valid local lock before deleting each test root.
release_old = '        fs::remove_dir_all(root_path)?;\n        Ok(())'
release_new = '        bridge_lock.release()?;\n        fs::remove_dir_all(root_path)?;\n        Ok(())'
count = text.count(release_old)
if count != 3:
    raise SystemExit(f"expected three existing cleanup markers, found {count}")
text = text.replace(release_old, release_new)

# Add stale-epoch proof before the end of the test module.
closing = '''    #[test]
    fn network_route_churn_is_retryable_before_launch()'''
if text.count(closing) != 1:
    raise SystemExit("network test anchor changed")
# Append test by replacing final module close only once.
body, end = text.rsplit("\n}\n", 1)
stale_test = r'''

    #[test]
    fn stale_workspace_epoch_fails_closed_without_releasing_or_deleting_lock()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("stale-epoch")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JSTALE")?;
        let profile = ProfileId::parse("profile_01JSTALE")?;
        let generation = GenerationId::parse("generation_01JSTALE")?;
        let device = DeviceId::parse("device_01JSTALE")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.inventory()?.inventory_digest(),
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &bridge_lock,
                &device,
                2,
                &binding,
                &policy("route-a")?,
                &observation("route-a")?,
                false,
            ),
            Err(BrowserLaunchBlocker::RecoveryRequired)
        );
        assert!(workspace.path().join(".profile-platform.lock").exists());
        bridge_lock.release()?;
        fs::remove_dir_all(root_path)?;
        Ok(())
    }
'''
guard.write_text(body + stale_test + "\n}\n" + end, encoding="utf-8")
