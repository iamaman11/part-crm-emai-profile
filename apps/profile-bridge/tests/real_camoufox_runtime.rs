#![forbid(unsafe_code)]
#![cfg(windows)] // This integration test exercises the packaged Windows Firefox runtime.

use profile_bridge::local_profile::MaterializationRoot;
use profile_bridge::runtime_bundle::ApprovedRuntimeBundle;
use profile_platform_primitives::DeviceId;
#[cfg(not(windows))]
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ENABLE_ENV: &str = "AR10_REAL_CAMOUFOX";
const PYTHON_ENV: &str = "AR10_PYTHON";
const RUNTIME_ROOT_ENV: &str = "AR10_RUNTIME_ROOT";
const HEADLESS_ENV: &str = "AR10_REAL_CAMOUFOX_HEADLESS";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const NATIVE_COLD_LAUNCH_ATTEMPTS: usize = 3;

#[test]
fn published_windows_runtime_is_consumed_by_shipping_selector()
-> Result<(), Box<dyn std::error::Error>> {
    use profile_bridge::operator_flow::RuntimeBundleSelectionPort;
    use profile_bridge::runtime_bundle::FilesystemRuntimeBundleSelection;
    let Some(root) = env::var_os("PACKAGED_WINDOWS_RUNTIME_ROOT") else {
        return Ok(());
    };
    let mut selector = FilesystemRuntimeBundleSelection::open(PathBuf::from(root))?;
    let bundle = selector.select_bundle()?;
    assert!(bundle.inventory().entries().len() >= 5);
    Ok(())
}

// This test is *only* enabled by the existing official-runtime Windows CI job.
// Read exactly the shipping compressed artifact with the production Rust reader,
// rather than interpreting the package with a second Python/TAR implementation.
// Other CI/normal developer tests do not need the multi-GB runtime.
#[cfg(windows)]
#[test]
fn shipping_windows_runtime_archive_is_readable_by_production_delivery_reader()
-> Result<(), Box<dyn std::error::Error>> {
    use profile_bridge::windows_delivery_archive::WindowsDeliveryArchiveReader;
    use profile_bridge::windows_delivery_staging::{
        DeliveryArchiveEntryKind, DeliveryArchiveReader, DeliveryComponentKind,
    };
    use std::collections::BTreeMap;

    let Some(archive) = env::var_os("AR10_PACKAGED_RUNTIME_ARCHIVE") else {
        return Ok(());
    };
    let manifest_path = env::var("AR10_PACKAGED_RUNTIME_MANIFEST")?;
    let archive = PathBuf::from(archive);
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let files = manifest
        .get("files")
        .and_then(|v| v.get("files"))
        .and_then(serde_json::Value::as_array)
        .ok_or("packaged runtime manifest file list is missing")?;

    let mut expected: BTreeMap<String, (u64, String)> = BTreeMap::new();
    for file in files {
        let path = file
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("manifest file path missing")?;
        let size = file
            .get("size_bytes")
            .and_then(serde_json::Value::as_u64)
            .ok_or("manifest file size missing")?;
        let hash = file
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or("manifest file hash missing")?;
        if expected
            .insert(path.to_owned(), (size, hash.to_owned()))
            .is_some()
        {
            return Err("duplicate packaged runtime manifest path".into());
        }
    }
    if expected.len() < 4_000 {
        return Err("the real official Windows runtime was not selected".into());
    }
    if expected
        .insert(
            "runtime-manifest.json".to_owned(),
            (manifest_bytes.len() as u64, sha256_hex(&manifest_bytes)),
        )
        .is_some()
    {
        return Err("runtime manifest collides with an existing runtime file".into());
    }

    let mut reader = WindowsDeliveryArchiveReader::new();
    let entries = reader.entries(DeliveryComponentKind::RuntimeBundle, &archive)?;
    if entries.len() != expected.len() {
        return Err(format!(
            "shipping archive inventory mismatch: expected {}, observed {}",
            expected.len(),
            entries.len()
        )
        .into());
    }
    let mut last: Option<&str> = None;
    for entry in &entries {
        let path = entry.relative_path();
        if last.is_some_and(|previous| previous >= path) {
            return Err("shipping runtime entries are not unique and ordered".into());
        }
        last = Some(path);
        if entry.kind() != DeliveryArchiveEntryKind::RegularFile {
            return Err("shipping runtime archive contains a link or special file".into());
        }
        let (size, sha) = expected
            .remove(path)
            .ok_or("shipping runtime archive has an unexpected path")?;
        if entry.size_bytes() != size || entry.sha256() != sha {
            return Err(format!("shipping runtime content digest mismatch: {path}").into());
        }
    }
    if !expected.is_empty() {
        return Err("shipping runtime archive omitted expected paths".into());
    }
    for (path, bytes) in [("runtime-manifest.json", manifest_bytes)] {
        let index = entries
            .iter()
            .position(|e| e.relative_path() == path)
            .ok_or("archive manifest missing")?;
        let mut streamed = Vec::new();
        reader.copy_regular_file(
            DeliveryComponentKind::RuntimeBundle,
            &archive,
            index,
            &mut streamed,
        )?;
        if streamed != bytes {
            return Err("shipping reader returned corrupted manifest bytes".into());
        }
    }
    eprintln!(
        "REAL_SHIPPING_ARCHIVE_READER=PASS;FILES={};COMPRESSED_BYTES={}",
        entries.len(),
        fs::metadata(&archive)?.len()
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn close_native_camoufox_window() -> Result<(), Box<dyn std::error::Error>> {
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class NativeCamoufoxWindow {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool IsWindow(IntPtr hWnd);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
}
'@
$deadline = [DateTime]::UtcNow.AddSeconds(30)
$posted = $false
do {
    $browserProcesses = @(Get-Process -Name 'camoufox' -ErrorAction SilentlyContinue)
    $browserWindows = @()
    foreach ($process in $browserProcesses) {
        $process.Refresh()
        $handle = $process.MainWindowHandle
        if ($handle -ne [IntPtr]::Zero -and [NativeCamoufoxWindow]::IsWindow($handle)) {
            $browserWindows += [pscustomobject]@{ Process = $process; Handle = $handle }
        }
    }
    if ($browserWindows.Count -gt 1) {
        throw "expected one native Camoufox window, found $($browserWindows.Count)"
    }
    if ($browserWindows.Count -eq 1) {
        $handle = $browserWindows[0].Handle
        if ([NativeCamoufoxWindow]::PostMessage(
            $handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero
        )) {
            $posted = $true
            break
        }
        $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        if ($errorCode -ne 1400) {
            throw "WM_CLOSE failed: $errorCode"
        }
        # The HWND was destroyed between IsWindow and PostMessage. Re-read
        # the process/window pair instead of treating a stale handle as a
        # browser lifecycle failure.
    }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
if ($posted) {
    Write-Output 'NATIVE_CAMOUFOX_WM_CLOSE=POSTED'
} elseif (@(Get-Process -Name 'camoufox' -ErrorAction SilentlyContinue).Count -eq 0) {
    # The browser closed naturally during the observation race. The caller
    # still joins the Bridge thread and requires its clean-close receipt.
    Write-Output 'NATIVE_CAMOUFOX_WM_CLOSE=ALREADY_CLOSED'
} else {
    throw 'native Camoufox window remained unavailable for WM_CLOSE'
}
"#;
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !(stdout.contains("NATIVE_CAMOUFOX_WM_CLOSE=POSTED")
            || stdout.contains("NATIVE_CAMOUFOX_WM_CLOSE=ALREADY_CLOSED"))
    {
        return Err(format!(
            "native Camoufox WM_CLOSE failed: stdout={stdout:?}; stderr={stderr:?}"
        )
        .into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn digest_file(path: &Path) -> Result<(u64, String), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("runtime evidence file is not regular".into());
    }
    let bytes = fs::read(path)?;
    Ok((metadata.len(), sha256_hex(&bytes)))
}

#[cfg(not(windows))]
fn inventory_digest(entries: &[(String, u64, String)]) -> String {
    let mut canonical = String::from("[");
    for (index, (path, length, sha256)) in entries.iter().enumerate() {
        if index > 0 {
            canonical.push(',');
        }
        canonical.push_str(&format!(
            "{{\"length\":{length},\"path\":\"{path}\",\"sha256\":\"{sha256}\"}}"
        ));
    }
    canonical.push_str("]\n");
    sha256_hex(canonical.as_bytes())
}

fn approved_runtime(
    runtime_root: &Path,
) -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        use profile_bridge::operator_flow::RuntimeBundleSelectionPort;
        use profile_bridge::runtime_bundle::FilesystemRuntimeBundleSelection;
        let mut selector = FilesystemRuntimeBundleSelection::open(runtime_root)?;
        Ok(selector.select_bundle()?)
    }
    #[cfg(not(windows))]
    {
        let real_path = runtime_root.join("camouhost/real.py");
        let lock_path = runtime_root.join("camouhost/runtime-lock.json");
        let (real_length, real_sha256) = digest_file(&real_path)?;
        let (lock_length, lock_sha256) = digest_file(&lock_path)?;
        let entries = [
            (
                "camouhost/real.py".to_owned(),
                real_length,
                real_sha256.clone(),
            ),
            (
                "camouhost/runtime-lock.json".to_owned(),
                lock_length,
                lock_sha256.clone(),
            ),
        ];
        let inventory_sha256 = inventory_digest(&entries);
        let manifest = RuntimeManifest::new(
            "2.0.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/real.py")?,
            Sha256Digest::parse(inventory_sha256.clone())?,
        )?;
        let inventory = RuntimeInventory::new([
            InventoryEntry::new(
                BundleRelativePath::parse("camouhost/real.py")?,
                real_length,
                Sha256Digest::parse(real_sha256)?,
            ),
            InventoryEntry::new(
                BundleRelativePath::parse("camouhost/runtime-lock.json")?,
                lock_length,
                Sha256Digest::parse(lock_sha256)?,
            ),
        ])?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &Sha256Digest::parse(inventory_sha256)?,
        )?)
    }
}

fn root_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(env::temp_dir().join(format!(
        "profile-bridge-ar10-real-runtime-{}-{nonce}",
        std::process::id()
    )))
}

// This official Windows gate runs the same native local profile owner as
// shipping_composition::run_initial. It intentionally has no cloud stub,
// managed Camouhost IPC, predecessor, synthetic generation or per-page observer.
#[cfg(windows)]
fn run_native_cold_launch_cycle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
    let python = PathBuf::from(env::var(PYTHON_ENV)?).canonicalize()?;
    let runtime_root = PathBuf::from(env::var(RUNTIME_ROOT_ENV)?).canonicalize()?;
    if python != runtime_root.join("python/python.exe").canonicalize()? {
        return Err("CI selected a different Python from the verified shipping runtime".into());
    }
    if env::var(HEADLESS_ENV).as_deref() != Ok("false") {
        return Err("native user Camoufox must run headful on Windows".into());
    }
    let started = Instant::now();
    let bundle = approved_runtime(&runtime_root)?;
    eprintln!(
        "AR10_COLD_CYCLE_PHASE=runtime_selection;elapsed_ms={}",
        started.elapsed().as_millis()
    );

    let root_path = root_path()?;
    let root = MaterializationRoot::open_or_create(&root_path)?;
    let intent = profile_bridge::generation_initialization::InitialProfileOpen::parse(
        "profilebridge://open/v2/tenant_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/actor_01JAR10REAL/profile_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/1/DRAFT",
    )?;
    let device = DeviceId::parse("device_01JAR10REAL")?;
    let mut stable_current = None;
    let mut stable_config = None;
    for iteration in 0..2 {
        let target = root_path.clone();
        let native_runtime = runtime_root.clone();
        let launch = intent.clone();
        let device = device.clone();
        let opened = thread::spawn(move || -> Result<(), String> {
            let owner =
                MaterializationRoot::open_or_create(target).map_err(|error| error.to_string())?;
            profile_bridge::local_browser_open::open(&owner, &launch, &device, &native_runtime)
                .map_err(|error| error.to_string())
        });

        // Observe only native filesystem/process readiness; no page/DOM
        // inspection, synthetic Playwright navigator, or managed browser IPC.
        let deadline = Instant::now() + Duration::from_secs(90);
        let folder = loop {
            if opened.is_finished() {
                let result = opened.join().map_err(|_| "native Open thread panicked")?;
                return Err(format!("native Camoufox exited before user Close: {result:?}").into());
            }
            if let Ok(folder) = root.open_profile_current(&intent.tenant_id, &intent.profile_id) {
                if folder.path().join("camoufox-config.json").is_file()
                    && fs::read(folder.path().join(".local-unsynced"))
                        .ok()
                        .as_deref()
                        == Some(&b"local-unsynced-v2"[..])
                {
                    break folder;
                }
            }
            if Instant::now() >= deadline {
                return Err("native current/identity/dirty marker not ready".into());
            }
            thread::sleep(Duration::from_millis(100));
        };
        // Reusing exactly this one folder is the only valid local Open path.
        match &stable_current {
            Some(path) if path != folder.path() => {
                return Err("cold reopening selected a different local folder".into());
            }
            None => stable_current = Some(folder.path().to_path_buf()),
            _ => {}
        }

        // The current native SDK must create a real browser window that
        // closes via a normal user WM_CLOSE, not kill/Playwright automation.
        let begin_close = Instant::now();
        close_native_camoufox_window()?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = opened
                .join()
                .map_err(|_| "native Open thread panicked".to_owned())
                .and_then(|value| value);
            let _ = sender.send(result);
        });
        let close = receiver
            .recv_timeout(Duration::from_secs(60))
            .map_err(|_| "native browser did not finish after WM_CLOSE")?;
        close.map_err(|error| format!("native browser Close failed: {error}"))?;
        eprintln!(
            "AR10_COLD_CYCLE_PHASE=native_open_close;ordinal={};elapsed_ms={}",
            iteration + 1,
            begin_close.elapsed().as_millis()
        );

        let confirmed = root.open_profile_current(&intent.tenant_id, &intent.profile_id)?;
        if confirmed.path() != folder.path() {
            return Err("native Close changed the local CURRENT pathname".into());
        }
        if fs::read(confirmed.path().join(".local-unsynced"))? != b"local-unsynced-v2" {
            return Err("local Close lost the pending Sync marker".into());
        }
        let config = fs::read(confirmed.path().join("camoufox-config.json"))?;
        if config.is_empty() {
            return Err("native Camoufox identity was empty".into());
        }
        if let Some(original) = &stable_config {
            if original != &config {
                return Err("reopening unexpectedly regenerated Camoufox identity".into());
            }
            if fs::read(confirmed.path().join("native-local-state-fixture.bin"))?
                != b"persistent native browser folder bytes"
            {
                return Err("native reopen lost working folder state".into());
            }
        } else {
            fs::write(
                confirmed.path().join("native-local-state-fixture.bin"),
                b"persistent native browser folder bytes",
            )?;
            stable_config = Some(config);
        }
    }

    // No hidden cloud Save is implied by local browser Close.
    let folder = root.open_profile_current(&intent.tenant_id, &intent.profile_id)?;
    if folder.base_generation_id().is_some() {
        return Err("native Open unexpectedly claimed a cloud generation".into());
    }
    fs::remove_dir_all(root_path)?;
    Ok(bundle)
}

#[cfg(windows)]
#[test]
fn bridge_preflight_launches_real_camoufox_through_official_playwright_without_page_inspection()
-> Result<(), Box<dyn std::error::Error>> {
    if env::var(ENABLE_ENV).as_deref() != Ok("1") {
        return Ok(());
    }

    let mut last_bundle = None;
    for attempt in 0..NATIVE_COLD_LAUNCH_ATTEMPTS {
        let started = Instant::now();
        last_bundle = Some(run_native_cold_launch_cycle()?);
        eprintln!(
            "AR10_COLD_CYCLE_COMPLETED=PASS;ordinal={};elapsed_ms={}",
            attempt + 1,
            started.elapsed().as_millis()
        );
    }
    // Each subsequent cold cycle revalidates the packaged official runtime.
    // The original persistent CURRENT has also been reopened twice per cycle.
    let last_bundle = last_bundle.ok_or("cold-launch attempt count must be nonzero")?;
    assert_eq!(
        approved_runtime(&PathBuf::from(env::var(RUNTIME_ROOT_ENV)?))?,
        last_bundle
    );
    Ok(())
}
