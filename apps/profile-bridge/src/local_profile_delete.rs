//! Exact, machine-confirmed terminal Profile cleanup in the existing local file owner.
//! A browser URI is a request, never authorization to erase local native data.
use crate::local_profile::{DeliveryActivationGuard, LocalProfileError, MaterializationRoot};
use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort};
use bridge_domain::BridgePortError;
use control_plane_contract::profile_generation_api::{
    BRIDGE_PROFILE_DELETE_READBACK_PATH_TEMPLATE, BRIDGE_PROFILE_LOCAL_COPY_READBACK_PATH_TEMPLATE,
    BridgeReadyLocalCopyReadback, ProfileProjectionDto, ProfileStatusDto,
};
use profile_platform_primitives::{ActorId, CorrelationId, GenerationId, ProfileId, TenantId};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub const TERMINAL_DELETE_URI_PREFIX: &str = "profilebridge://delete/v1/";
pub const READY_LOCAL_COPY_URI_PREFIX: &str = "profilebridge://local-copy/v1/";

fn parse_local_uri(
    uri: &str,
    prefix: &str,
) -> Result<(TenantId, ActorId, ProfileId, u64), BridgePortError> {
    let rest = uri
        .strip_prefix(prefix)
        .ok_or(BridgePortError::InvalidResponse)?;
    let parts: Vec<_> = rest.split('/').collect();
    if parts.len() != 4
        || parts[3].starts_with('0')
        || !parts[3].bytes().all(|c| c.is_ascii_digit())
    {
        return Err(BridgePortError::InvalidResponse);
    }
    let version = parts[3]
        .parse::<u64>()
        .map_err(|_| BridgePortError::InvalidResponse)?;
    if version == 0 || version >= i64::MAX as u64 {
        return Err(BridgePortError::InvalidResponse);
    }
    Ok((
        TenantId::parse(parts[0]).map_err(|_| BridgePortError::InvalidResponse)?,
        ActorId::parse(parts[1]).map_err(|_| BridgePortError::InvalidResponse)?,
        ProfileId::parse(parts[2]).map_err(|_| BridgePortError::InvalidResponse)?,
        version,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadyLocalCopyEviction {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub profile_id: ProfileId,
    pub expected_ready_version: u64,
}

impl ReadyLocalCopyEviction {
    pub fn parse(uri: &str) -> Result<Self, BridgePortError> {
        let (tenant_id, actor_id, profile_id, expected_ready_version) =
            parse_local_uri(uri, READY_LOCAL_COPY_URI_PREFIX)?;
        Ok(Self {
            tenant_id,
            actor_id,
            profile_id,
            expected_ready_version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalProfileDelete {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub profile_id: ProfileId,
    pub expected_deleted_version: u64,
}

impl TerminalProfileDelete {
    pub fn parse(uri: &str) -> Result<Self, BridgePortError> {
        let (tenant_id, actor_id, profile_id, version) =
            parse_local_uri(uri, TERMINAL_DELETE_URI_PREFIX)?;
        Ok(Self {
            tenant_id,
            actor_id,
            profile_id,
            expected_deleted_version: version,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalDeleteError {
    AuthorizationUnconfirmed,
    LocalBusy,
    LocalFilesystem,
    LocalNotClean,
    UserDeclined,
}

impl core::fmt::Display for TerminalDeleteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::AuthorizationUnconfirmed => {
                "Cloud DELETED was not proven by the authenticated device; local files preserved"
            }
            Self::LocalBusy => {
                "A native browser or Bridge writer may still be active; local files preserved"
            }
            Self::LocalFilesystem => {
                "Local profile cleanup could not be verified; inspect native local files"
            }
            Self::LocalNotClean => {
                "Local READY copy has uncommitted or recovery evidence; local files preserved"
            }
            Self::UserDeclined => "Local READY copy removal was cancelled; local files preserved",
        })
    }
}

impl std::error::Error for TerminalDeleteError {}

/// A machine-read VERIFIED current is only a prerequisite. The native owner
/// must independently refuse dirty, pending, stale, busy, or foreign local data.
pub fn confirm_ready_local_copy<T: MachineHttpPort>(
    intent: &ReadyLocalCopyEviction,
    transport: &mut T,
) -> Result<GenerationId, TerminalDeleteError> {
    let path = BRIDGE_PROFILE_LOCAL_COPY_READBACK_PATH_TEMPLATE
        .replace("{tenantId}", intent.tenant_id.as_str())
        .replace("{profileId}", intent.profile_id.as_str());
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?
        .as_millis();
    let correlation =
        CorrelationId::parse(format!("corr_local_ready_{}_{}", std::process::id(), epoch))
            .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    let response = transport
        .request(MachineHttpMethod::Get, &path, &correlation, None)
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    if response.status() != 200 || response.body().len() > 1_024 {
        return Err(TerminalDeleteError::AuthorizationUnconfirmed);
    }
    let confirmed: BridgeReadyLocalCopyReadback = serde_json::from_slice(response.body())
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    if confirmed.profile_id != intent.profile_id.as_str()
        || confirmed.profile_version != intent.expected_ready_version
    {
        return Err(TerminalDeleteError::AuthorizationUnconfirmed);
    }
    GenerationId::parse(confirmed.generation_id)
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)
}

pub fn cleanup_clean_ready_current(
    root: &MaterializationRoot,
    intent: &ReadyLocalCopyEviction,
    confirmed_generation: &GenerationId,
) -> Result<(), TerminalDeleteError> {
    // Excludes other Bridge Opens/Syncs, inherited Camouhost launch guards,
    // native Firefox handles, and active generation admission locks.
    let _guard =
        DeliveryActivationGuard::acquire(root).map_err(|_| TerminalDeleteError::LocalBusy)?;
    let workspace = match root.open_profile_current(&intent.tenant_id, &intent.profile_id) {
        Ok(value) => value,
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => return Ok(()),
        Err(_) => return Err(TerminalDeleteError::LocalFilesystem),
    };
    if workspace.generation_id() != confirmed_generation
        || workspace.base_generation_id() != Some(confirmed_generation)
        || workspace.profile_version() != intent.expected_ready_version
        || workspace.pending_sync().is_some()
    {
        return Err(TerminalDeleteError::LocalNotClean);
    }
    // Any dirty marker (including an unsafe symlink or unreadable file), and
    // any unresolved previous Bridge writer marker, are recovery evidence.
    // Even an empty marker cannot authorize local eviction.
    for name in [".local-unsynced", ".profile-platform.lock"] {
        match fs::symlink_metadata(workspace.path().join(name)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(TerminalDeleteError::LocalNotClean),
        }
    }
    workspace
        .metadata_total_bytes()
        .map_err(|_| TerminalDeleteError::LocalFilesystem)?;
    fs::remove_dir_all(workspace.path()).map_err(|_| TerminalDeleteError::LocalFilesystem)?;
    Ok(())
}

/// The same P-256 device-authenticated transport as normal Bridge Sync.
/// No human-provided URI, cloud response from a browser, or stale mutation
/// receipt can substitute for this read-only terminal D1 projection.
pub fn confirm_terminal_delete<T: MachineHttpPort>(
    intent: &TerminalProfileDelete,
    transport: &mut T,
) -> Result<(), TerminalDeleteError> {
    let path = BRIDGE_PROFILE_DELETE_READBACK_PATH_TEMPLATE
        .replace("{tenantId}", intent.tenant_id.as_str())
        .replace("{profileId}", intent.profile_id.as_str());
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?
        .as_millis();
    let correlation = CorrelationId::parse(format!(
        "corr_local_deleted_{}_{}",
        std::process::id(),
        epoch
    ))
    .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    let response = transport
        .request(MachineHttpMethod::Get, &path, &correlation, None)
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    if response.status() != 200 || response.body().len() > 4_096 {
        return Err(TerminalDeleteError::AuthorizationUnconfirmed);
    }
    let projection: ProfileProjectionDto = serde_json::from_slice(response.body())
        .map_err(|_| TerminalDeleteError::AuthorizationUnconfirmed)?;
    if projection.profile_id != intent.profile_id.as_str()
        || projection.status != ProfileStatusDto::Deleted
        || projection.version < intent.expected_deleted_version
    {
        return Err(TerminalDeleteError::AuthorizationUnconfirmed);
    }
    Ok(())
}

/// Existing MaterializationRoot remains the sole owner of native files.
/// The existing *exclusive* root OS admission fences all concurrent Bridge
/// Opens/Syncs and the native Firefox lock check before removing exact CURRENT.
/// This method never cleans historical or unrelated folders by prefix.
pub fn cleanup_confirmed_current(
    root: &MaterializationRoot,
    intent: &TerminalProfileDelete,
) -> Result<(), TerminalDeleteError> {
    let _guard =
        DeliveryActivationGuard::acquire(root).map_err(|_| TerminalDeleteError::LocalBusy)?;
    let workspace = match root.open_profile_current(&intent.tenant_id, &intent.profile_id) {
        Ok(value) => value,
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => return Ok(()),
        Err(_) => return Err(TerminalDeleteError::LocalFilesystem),
    };
    // The existing machine-authenticated caller has proved terminal DELETED.
    // Dirty bytes and an unfinished successor belong to the explicitly discarded
    // Profile. Do not require saving them before irrevocable deletion. Root-wide
    // OS admission above still rejects any live/unknown Bridge or Firefox writer.
    // Metadata traversal rejects links/reparse points and unexpected special
    // files without reading user cookies, IndexedDB or the huge browser cache.
    workspace
        .metadata_total_bytes()
        .map_err(|_| TerminalDeleteError::LocalFilesystem)?;
    fs::remove_dir_all(workspace.path()).map_err(|_| TerminalDeleteError::LocalFilesystem)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        READY_LOCAL_COPY_URI_PREFIX, ReadyLocalCopyEviction, TERMINAL_DELETE_URI_PREFIX,
        TerminalDeleteError, TerminalProfileDelete, cleanup_clean_ready_current,
        cleanup_confirmed_current, confirm_ready_local_copy, confirm_terminal_delete,
    };
    use crate::local_profile::{MaterializationRoot, PendingSyncDescriptor};
    use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, MachineHttpResponse};
    use profile_platform_primitives::{CorrelationId, GenerationId};
    use std::fs;

    struct Readback {
        status: u16,
        body: Vec<u8>,
    }
    impl MachineHttpPort for Readback {
        type Error = ();
        fn request(
            &mut self,
            method: MachineHttpMethod,
            path: &str,
            _cid: &CorrelationId,
            body: Option<&[u8]>,
        ) -> Result<MachineHttpResponse, Self::Error> {
            assert_eq!(method, MachineHttpMethod::Get);
            assert_eq!(
                path,
                "/bridge/v1/tenants/tenant_delete_test_01/profiles/profile_delete_test_01/delete-readback"
            );
            assert!(body.is_none());
            Ok(MachineHttpResponse::new(self.status, self.body.clone()))
        }
    }
    fn intent() -> Result<TerminalProfileDelete, bridge_domain::BridgePortError> {
        TerminalProfileDelete::parse(
            "profilebridge://delete/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4",
        )
    }
    fn response(status: &str, version: u64) -> Readback {
        Readback { status: 200, body: format!(
            r#"{{"profileId":"profile_delete_test_01","displayName":"Deleted test profile","status":"{status}","version":{version},"metadataVersion":1,"linkedClientId":null,"canOperate":false}}"#
        ).into_bytes() }
    }
    fn ready_intent() -> Result<ReadyLocalCopyEviction, bridge_domain::BridgePortError> {
        ReadyLocalCopyEviction::parse(
            "profilebridge://local-copy/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4",
        )
    }
    fn ready_response(profile: &str, version: u64, generation: &str) -> Readback {
        Readback { status: 200, body: format!(
            r#"{{"profileId":"{profile}","profileVersion":{version},"generationId":"{generation}"}}"#,
        ).into_bytes() }
    }

    #[test]
    fn ready_eviction_intent_cannot_be_terminal_delete_or_forged_version()
    -> Result<(), Box<dyn std::error::Error>> {
        let ready = ready_intent()?;
        assert_eq!(ready.expected_ready_version, 4);
        assert!(READY_LOCAL_COPY_URI_PREFIX.ends_with("/v1/"));
        for invalid in [
            "profilebridge://delete/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4",
            "profilebridge://local-copy/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/0",
            "profilebridge://local-copy/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/04",
            "profilebridge://local-copy/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4/extra",
        ] {
            assert!(ReadyLocalCopyEviction::parse(invalid).is_err());
        }
        Ok(())
    }

    #[test]
    fn ready_eviction_requires_exact_signed_ready_generation_readback()
    -> Result<(), Box<dyn std::error::Error>> {
        struct ReadyReadback(Readback);
        impl MachineHttpPort for ReadyReadback {
            type Error = ();
            fn request(
                &mut self,
                method: MachineHttpMethod,
                path: &str,
                _cid: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, Self::Error> {
                assert_eq!(method, MachineHttpMethod::Get);
                assert_eq!(
                    path,
                    "/bridge/v1/tenants/tenant_delete_test_01/profiles/profile_delete_test_01/local-copy-readback"
                );
                assert!(body.is_none());
                Ok(MachineHttpResponse::new(self.0.status, self.0.body.clone()))
            }
        }
        let ready = ready_intent()?;
        let good_generation = "generation_delete_test_ready";
        let confirmed = confirm_ready_local_copy(
            &ready,
            &mut ReadyReadback(ready_response("profile_delete_test_01", 4, good_generation)),
        )?;
        assert_eq!(confirmed.as_str(), good_generation);
        for refused in [
            ready_response("profile_delete_other_01", 4, good_generation),
            ready_response("profile_delete_test_01", 3, good_generation),
            ready_response("profile_delete_test_01", 5, good_generation),
            ready_response("profile_delete_test_01", 4, "invalid/generation"),
            Readback {
                status: 403,
                body: ready_response("profile_delete_test_01", 4, good_generation).body,
            },
            Readback {
                status: 200,
                body: b"{}".to_vec(),
            },
        ] {
            assert_eq!(
                confirm_ready_local_copy(&ready, &mut ReadyReadback(refused)),
                Err(TerminalDeleteError::AuthorizationUnconfirmed),
            );
        }
        Ok(())
    }

    #[test]
    fn ready_eviction_removes_only_exact_clean_current_and_retains_unsafe_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        use profile_platform_primitives::ProfileId;
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let ready = ready_intent()?;
        let generation = GenerationId::parse("generation_delete_test_ready")?;
        let other = ProfileId::parse("profile_ready_other_01")?;
        let initial =
            root.create_profile_current(&ready.tenant_id, &ready.profile_id, &generation, 4)?;
        let workspace = root.update_profile_current_sync_base(&initial, &generation, 4)?;
        let unrelated = root.create_profile_current(&ready.tenant_id, &other, &generation, 4)?;
        fs::create_dir(workspace.path().join("user_data"))?;
        fs::write(
            workspace.path().join("user_data/cookies.sqlite"),
            b"must survive refused requests",
        )?;
        fs::write(unrelated.path().join("keep.txt"), b"keep unrelated")?;
        let other_generation = GenerationId::parse("generation_ready_mismatch")?;
        assert_eq!(
            cleanup_clean_ready_current(&root, &ready, &other_generation),
            Err(TerminalDeleteError::LocalNotClean)
        );
        assert!(workspace.path().join("user_data/cookies.sqlite").exists());

        fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;
        assert_eq!(
            cleanup_clean_ready_current(&root, &ready, &generation),
            Err(TerminalDeleteError::LocalNotClean)
        );
        fs::remove_file(workspace.path().join(".local-unsynced"))?;

        let candidate = GenerationId::parse("generation_delete_pending_ready")?;
        let pending =
            PendingSyncDescriptor::new(candidate, [0x11; 32], [0x22; 32], 4096, [0x33; 32], 7)?;
        let marked = root.set_profile_current_pending_sync(&workspace, pending)?;
        assert_eq!(
            cleanup_clean_ready_current(&root, &ready, &generation),
            Err(TerminalDeleteError::LocalNotClean)
        );
        let workspace = root.clear_profile_current_pending_sync(&marked)?;
        fs::write(
            workspace.path().join(".profile-platform.lock"),
            b"recovery evidence",
        )?;
        assert_eq!(
            cleanup_clean_ready_current(&root, &ready, &generation),
            Err(TerminalDeleteError::LocalNotClean)
        );
        fs::remove_file(workspace.path().join(".profile-platform.lock"))?;

        cleanup_clean_ready_current(&root, &ready, &generation)?;
        assert!(!workspace.path().exists());
        assert_eq!(
            fs::read(unrelated.path().join("keep.txt"))?,
            b"keep unrelated"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn ready_eviction_refuses_live_native_firefox_even_when_markers_are_clean()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let ready = ready_intent()?;
        let generation = GenerationId::parse("generation_delete_test_ready")?;
        let initial =
            root.create_profile_current(&ready.tenant_id, &ready.profile_id, &generation, 4)?;
        let workspace = root.update_profile_current_sync_base(&initial, &generation, 4)?;
        fs::create_dir(workspace.path().join("user_data"))?;
        let native_lock = workspace.path().join("user_data/parent.lock");
        fs::write(&native_lock, b"native-lock")?;
        let handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&native_lock)?;
        assert_eq!(
            cleanup_clean_ready_current(&root, &ready, &generation),
            Err(TerminalDeleteError::LocalBusy)
        );
        assert!(workspace.path().exists());
        drop(handle);
        Ok(())
    }
    #[test]
    fn parsing_never_accepts_open_or_forged_extra_segments()
    -> Result<(), Box<dyn std::error::Error>> {
        let valid = intent()?;
        assert_eq!(valid.expected_deleted_version, 4);
        for invalid in [
            "profilebridge://delete/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/0",
            "profilebridge://delete/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/04",
            "profilebridge://delete/v1/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4/extra",
            "profilebridge://open/v2/tenant_delete_test_01/actor_delete_test_01/profile_delete_test_01/4/READY",
        ] {
            assert!(TerminalProfileDelete::parse(invalid).is_err());
        }
        assert!(TERMINAL_DELETE_URI_PREFIX.ends_with("/v1/"));
        Ok(())
    }

    #[test]
    fn only_exact_terminal_machine_response_authorizes_cleanup()
    -> Result<(), Box<dyn std::error::Error>> {
        let intent = intent()?;
        confirm_terminal_delete(&intent, &mut response("DELETED", 5))?;
        for mut denied in [
            response("READY", 4),
            response("DELETING", 5),
            response("DELETED", 3),
            Readback {
                status: 404,
                body: response("DELETED", 5).body,
            },
        ] {
            assert_eq!(
                confirm_terminal_delete(&intent, &mut denied),
                Err(TerminalDeleteError::AuthorizationUnconfirmed)
            );
        }
        Ok(())
    }

    #[test]
    fn terminal_delete_discards_dirty_after_verified_cloud_deleted()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent()?;
        let generation = GenerationId::parse("generation_delete_test_01")?;
        let workspace =
            root.create_profile_current(&intent.tenant_id, &intent.profile_id, &generation, 2)?;
        fs::create_dir(workspace.path().join("user_data"))?;
        fs::write(workspace.path().join("user_data/cookies.sqlite"), b"cookie")?;
        fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;

        for mut refused in [response("READY", 4), response("DELETING", 5)] {
            assert_eq!(
                confirm_terminal_delete(&intent, &mut refused),
                Err(TerminalDeleteError::AuthorizationUnconfirmed)
            );
            assert!(workspace.path().join("user_data/cookies.sqlite").exists());
        }
        confirm_terminal_delete(&intent, &mut response("DELETED", 5))?;
        cleanup_confirmed_current(&root, &intent)?;
        assert!(!workspace.path().exists());
        cleanup_confirmed_current(&root, &intent)?;
        Ok(())
    }

    #[test]
    fn terminal_delete_discards_pending_successor_but_not_another_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        use profile_platform_primitives::ProfileId;
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let intent = intent()?;
        let other = ProfileId::parse("profile_delete_other_pending")?;
        let generation = GenerationId::parse("generation_delete_test_pending")?;
        let workspace =
            root.create_profile_current(&intent.tenant_id, &intent.profile_id, &generation, 2)?;
        let unrelated = root.create_profile_current(&intent.tenant_id, &other, &generation, 2)?;
        fs::create_dir(workspace.path().join("user_data"))?;
        fs::write(
            workspace.path().join("user_data/cookies.sqlite"),
            b"discard",
        )?;
        fs::write(unrelated.path().join("keep.txt"), b"keep")?;
        let candidate = GenerationId::parse("generation_delete_pending_successor")?;
        let pending =
            PendingSyncDescriptor::new(candidate, [0x11; 32], [0x22; 32], 4096, [0x33; 32], 7)?;
        // An N+1 candidate is valid only after the local folder has a cloud
        // base. Test the real READY successor path rather than a DRAFT.
        let ready = root.update_profile_current_sync_base(&workspace, &generation, 2)?;
        let marked = root.set_profile_current_pending_sync(&ready, pending)?;
        assert!(marked.pending_sync().is_some());
        fs::write(marked.path().join(".local-unsynced"), b"local-unsynced-v2")?;
        confirm_terminal_delete(&intent, &mut response("DELETED", 5))?;
        cleanup_confirmed_current(&root, &intent)?;
        assert!(!workspace.path().exists());
        assert_eq!(fs::read(unrelated.path().join("keep.txt"))?, b"keep");
        Ok(())
    }

    #[test]
    fn local_cleanup_removes_only_the_selected_profile() -> Result<(), Box<dyn std::error::Error>> {
        use profile_platform_primitives::ProfileId;
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let target = intent()?;
        let other = ProfileId::parse("profile_delete_other_01")?;
        let generation = GenerationId::parse("generation_delete_other_01")?;
        let selected_workspace =
            root.create_profile_current(&target.tenant_id, &target.profile_id, &generation, 4)?;
        let other_workspace =
            root.create_profile_current(&target.tenant_id, &other, &generation, 4)?;
        fs::write(other_workspace.path().join("keep.txt"), b"unrelated")?;
        cleanup_confirmed_current(&root, &target)?;
        assert!(!selected_workspace.path().exists());
        assert_eq!(
            fs::read(other_workspace.path().join("keep.txt"))?,
            b"unrelated"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn local_cleanup_refuses_live_firefox_without_a_bridge_process()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir()?;
        let root = MaterializationRoot::open_or_create(directory.path())?;
        let target = intent()?;
        let generation = GenerationId::parse("generation_delete_firefox_01")?;
        let workspace =
            root.create_profile_current(&target.tenant_id, &target.profile_id, &generation, 4)?;
        let browser_data = workspace.path().join("user_data");
        fs::create_dir(&browser_data)?;
        let cookies = browser_data.join("cookies.sqlite");
        fs::write(&cookies, b"native-data-must-survive")?;
        fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;
        let parent_lock = browser_data.join("parent.lock");
        fs::write(&parent_lock, b"firefox-writer")?;
        let firefox_handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&parent_lock)?;

        assert_eq!(
            cleanup_confirmed_current(&root, &target),
            Err(TerminalDeleteError::LocalBusy)
        );
        assert_eq!(fs::read(&cookies)?, b"native-data-must-survive");
        drop(firefox_handle);

        // Stale parent.lock after normal native Close is not a live writer.
        cleanup_confirmed_current(&root, &target)?;
        assert!(!workspace.path().exists());
        Ok(())
    }
}
