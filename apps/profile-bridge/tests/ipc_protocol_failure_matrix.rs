#![forbid(unsafe_code)]

use bridge_domain::{
    BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort, CamouhostProtocolError,
};
use profile_bridge::FakeCamouhost;
use profile_platform_primitives::SessionId;

#[test]
fn oversized_ipc_frame_fails_closed() {
    let oversized = "x".repeat(513);
    assert_eq!(
        CamouhostMessage::parse(&oversized),
        Err(CamouhostProtocolError::MalformedFrame)
    );
}

#[test]
fn launch_and_close_replay_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let session_id = SessionId::parse("session_01JAR10REPLAY")?;
    let mut runtime = FakeCamouhost::default();

    assert_eq!(
        runtime.exchange(&CamouhostMessage::Hello {
            version: CAMOUHOST_IPC_VERSION,
        })?,
        CamouhostMessage::HelloAck {
            version: CAMOUHOST_IPC_VERSION,
        }
    );

    let launch = CamouhostMessage::Launch {
        session_id: session_id.clone(),
    };
    assert_eq!(
        runtime.exchange(&launch)?,
        CamouhostMessage::Ready {
            session_id: session_id.clone(),
        }
    );
    assert_eq!(
        runtime.exchange(&launch),
        Err(BridgePortError::InvalidResponse)
    );

    let close = CamouhostMessage::Close {
        session_id: session_id.clone(),
    };
    assert_eq!(
        runtime.exchange(&close)?,
        CamouhostMessage::Closed {
            session_id,
            clean: true,
        }
    );
    assert_eq!(
        runtime.exchange(&close),
        Err(BridgePortError::InvalidResponse)
    );
    Ok(())
}
