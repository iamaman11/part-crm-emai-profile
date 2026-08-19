#![forbid(unsafe_code)]

use bridge_domain::{BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort};
use profile_bridge::{ProcessAction, ProcessControlPort};
use profile_bridge::runtime_bundle::{ApprovedRuntimeBundle, RuntimeLaunchError, RuntimeSessionOrchestrator};
use profile_platform_primitives::SessionId;
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
    Sha256Digest,
};
use std::collections::VecDeque;

fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
    Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
}

fn approved_bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
    let calculated = digest('a')?;
    let entrypoint = BundleRelativePath::parse("camouhost/real.py")?;
    let manifest = RuntimeManifest::new(
        "2.0.0",
        "3.12",
        RuntimePlatform::WindowsX86_64,
        entrypoint.clone(),
        calculated.clone(),
    )?;
    let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])?;
    Ok(ApprovedRuntimeBundle::validate(manifest, inventory, &calculated)?)
}

#[derive(Default)]
struct TrackingProcess {
    actions: Vec<ProcessAction>,
    spawn_error: Option<BridgePortError>,
    graceful_error: Option<BridgePortError>,
    confirm_error: Option<BridgePortError>,
    force_error: Option<BridgePortError>,
}

impl ProcessControlPort for TrackingProcess {
    fn spawn(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions.push(ProcessAction::Spawn(session_id.clone()));
        self.spawn_error.map_or(Ok(()), Err)
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .push(ProcessAction::GracefulClose(session_id.clone()));
        self.graceful_error.map_or(Ok(()), Err)
    }

    fn confirm_stopped(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .push(ProcessAction::ConfirmStopped(session_id.clone()));
        self.confirm_error.map_or(Ok(()), Err)
    }

    fn force_terminate(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        self.actions
            .push(ProcessAction::ForceTerminate(session_id.clone()));
        self.force_error.map_or(Ok(()), Err)
    }
}

struct ScriptedCamouhost {
    responses: VecDeque<Result<CamouhostMessage, BridgePortError>>,
}

impl ScriptedCamouhost {
    fn new(
        responses: impl IntoIterator<Item = Result<CamouhostMessage, BridgePortError>>,
    ) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

impl CamouhostPort for ScriptedCamouhost {
    fn exchange(
        &mut self,
        _message: &CamouhostMessage,
    ) -> Result<CamouhostMessage, BridgePortError> {
        self.responses
            .pop_front()
            .unwrap_or(Err(BridgePortError::InvalidResponse))
    }
}

fn hello_ack() -> CamouhostMessage {
    CamouhostMessage::HelloAck {
        version: CAMOUHOST_IPC_VERSION,
    }
}

fn ready(session_id: &SessionId) -> CamouhostMessage {
    CamouhostMessage::Ready {
        session_id: session_id.clone(),
    }
}

fn closed(session_id: &SessionId, clean: bool) -> CamouhostMessage {
    CamouhostMessage::Closed {
        session_id: session_id.clone(),
        clean,
    }
}

#[test]
fn hello_transport_loss_forces_process_rollback() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = approved_bundle()?;
    let session = SessionId::parse("session_01JAR10HELLOLOSS")?;
    let mut process = TrackingProcess::default();
    let mut camouhost = ScriptedCamouhost::new([Err(BridgePortError::Unavailable)]);

    assert_eq!(
        RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost),
        Err(RuntimeLaunchError::Camouhost(BridgePortError::Unavailable))
    );
    assert_eq!(
        process.actions,
        [
            ProcessAction::Spawn(session.clone()),
            ProcessAction::ForceTerminate(session),
        ]
    );
    Ok(())
}

#[test]
fn transport_loss_between_hello_and_ready_forces_process_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let bundle = approved_bundle()?;
    let session = SessionId::parse("session_01JAR10READYLOSS")?;
    let mut process = TrackingProcess::default();
    let mut camouhost = ScriptedCamouhost::new([
        Ok(hello_ack()),
        Err(BridgePortError::InvalidResponse),
    ]);

    assert_eq!(
        RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost),
        Err(RuntimeLaunchError::Camouhost(BridgePortError::InvalidResponse))
    );
    assert_eq!(
        process.actions,
        [
            ProcessAction::Spawn(session.clone()),
            ProcessAction::ForceTerminate(session),
        ]
    );
    Ok(())
}

#[test]
fn wrong_session_ready_is_rejected_and_terminated() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = approved_bundle()?;
    let session = SessionId::parse("session_01JAR10RIGHTREADY")?;
    let wrong = SessionId::parse("session_01JAR10WRONGREADY")?;
    let mut process = TrackingProcess::default();
    let mut camouhost = ScriptedCamouhost::new([Ok(hello_ack()), Ok(ready(&wrong))]);

    assert_eq!(
        RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost),
        Err(RuntimeLaunchError::Camouhost(BridgePortError::InvalidResponse))
    );
    assert_eq!(
        process.actions,
        [
            ProcessAction::Spawn(session.clone()),
            ProcessAction::ForceTerminate(session),
        ]
    );
    Ok(())
}

#[test]
fn wrong_or_unclean_closed_response_never_confirms_clean_stop()
-> Result<(), Box<dyn std::error::Error>> {
    for use_wrong_session in [false, true] {
        let bundle = approved_bundle()?;
        let session = SessionId::parse("session_01JAR10CLOSECHECK")?;
        let wrong = SessionId::parse("session_01JAR10WRONGCLOSE")?;
        let close_session = if use_wrong_session { &wrong } else { &session };
        let mut process = TrackingProcess::default();
        let mut camouhost = ScriptedCamouhost::new([
            Ok(hello_ack()),
            Ok(ready(&session)),
            Ok(closed(close_session, false)),
        ]);

        RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
        assert_eq!(
            RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost),
            Err(RuntimeLaunchError::Camouhost(BridgePortError::InvalidResponse))
        );
        assert_eq!(
            process.actions,
            [
                ProcessAction::Spawn(session.clone()),
                ProcessAction::GracefulClose(session.clone()),
                ProcessAction::ForceTerminate(session.clone()),
            ]
        );
    }
    Ok(())
}

#[test]
fn clean_closed_but_process_not_stopped_forces_termination()
-> Result<(), Box<dyn std::error::Error>> {
    let bundle = approved_bundle()?;
    let session = SessionId::parse("session_01JAR10NOTSTOPPED")?;
    let mut process = TrackingProcess {
        confirm_error: Some(BridgePortError::Unavailable),
        ..TrackingProcess::default()
    };
    let mut camouhost = ScriptedCamouhost::new([
        Ok(hello_ack()),
        Ok(ready(&session)),
        Ok(closed(&session, true)),
    ]);

    RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost)?;
    assert_eq!(
        RuntimeSessionOrchestrator::close(&bundle, &session, &mut process, &mut camouhost),
        Err(RuntimeLaunchError::Process(BridgePortError::Unavailable))
    );
    assert_eq!(
        process.actions,
        [
            ProcessAction::Spawn(session.clone()),
            ProcessAction::GracefulClose(session.clone()),
            ProcessAction::ConfirmStopped(session.clone()),
            ProcessAction::ForceTerminate(session),
        ]
    );
    Ok(())
}

#[test]
fn rollback_failure_is_preserved_as_distinct_error() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = approved_bundle()?;
    let session = SessionId::parse("session_01JAR10ROLLBACKFAIL")?;
    let mut process = TrackingProcess {
        force_error: Some(BridgePortError::Unavailable),
        ..TrackingProcess::default()
    };
    let mut camouhost = ScriptedCamouhost::new([Ok(CamouhostMessage::Hello {
        version: CAMOUHOST_IPC_VERSION,
    })]);

    assert_eq!(
        RuntimeSessionOrchestrator::launch(&bundle, &session, &mut process, &mut camouhost),
        Err(RuntimeLaunchError::Rollback {
            source: BridgePortError::InvalidResponse,
            rollback: BridgePortError::Unavailable,
        })
    );
    assert_eq!(
        process.actions,
        [
            ProcessAction::Spawn(session.clone()),
            ProcessAction::ForceTerminate(session),
        ]
    );
    Ok(())
}
