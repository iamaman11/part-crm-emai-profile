from pathlib import Path

path = Path("apps/profile-bridge/src/shipping_control_plane.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    text = text.replace(old, new)


replace_once(
    """#[derive(Clone, Debug, Eq, PartialEq)]
struct CoordinatorCursor {
""",
    """#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlPlaneLeaseTiming {
    idle_expires_at_ms: u64,
    hard_expires_at_ms: u64,
}

impl ControlPlaneLeaseTiming {
    #[must_use]
    pub const fn idle_expires_at_ms(self) -> u64 {
        self.idle_expires_at_ms
    }

    #[must_use]
    pub const fn hard_expires_at_ms(self) -> u64 {
        self.hard_expires_at_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoordinatorCursor {
""",
    "timing value",
)
replace_once(
    """    fencing_token: FencingToken,
    version: u64,
    sequence: u64,
}
""",
    """    fencing_token: FencingToken,
    version: u64,
    sequence: u64,
    timing: ControlPlaneLeaseTiming,
}
""",
    "cursor timing",
)
replace_once(
    """    pub const fn new(transport: T) -> Self {
        Self {
            transport,
            cursor: None,
        }
    }
}
""",
    """    pub const fn new(transport: T) -> Self {
        Self {
            transport,
            cursor: None,
        }
    }

    pub fn runtime_timing(&self) -> Result<ControlPlaneLeaseTiming, ShippingControlPlaneError> {
        self.cursor
            .as_ref()
            .map(|cursor| cursor.timing)
            .ok_or(ShippingControlPlaneError::InvalidResponse)
    }
}
""",
    "runtime timing getter",
)
replace_once(
    """        let epoch = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::LeaseClaimed,
            actor.tenant_scope().tenant_id(),
            profile_id,
            device_id,
            &session_id,
            sequence,
        )?;
""",
    """        let (epoch, timing) = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::LeaseClaimed,
            actor.tenant_scope().tenant_id(),
            profile_id,
            device_id,
            &session_id,
            sequence,
        )?;
""",
    "claim timing",
)
replace_once(
    """            fencing_token,
            version: response.version,
            sequence: response.sequence,
        });
""",
    """            fencing_token,
            version: response.version,
            sequence: response.sequence,
            timing,
        });
""",
    "store claim timing",
)
replace_once(
    """        let epoch = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::HeartbeatAccepted,
            &cursor.tenant_id,
            &cursor.profile_id,
            &cursor.device_id,
            &cursor.session_id,
            sequence,
        )?;
""",
    """        let (epoch, timing) = validate_active_projection(
            &response,
            CoordinatorOutcomeDto::HeartbeatAccepted,
            &cursor.tenant_id,
            &cursor.profile_id,
            &cursor.device_id,
            &cursor.session_id,
            sequence,
        )?;
""",
    "heartbeat timing",
)
replace_once(
    """        current.version = response.version;
        current.sequence = response.sequence;
        Ok(())
""",
    """        current.version = response.version;
        current.sequence = response.sequence;
        current.timing = timing;
        Ok(())
""",
    "update heartbeat timing",
)
replace_once(
    """    expected_sequence: u64,
) -> Result<u64, ShippingControlPlaneError> {
""",
    """    expected_sequence: u64,
) -> Result<(u64, ControlPlaneLeaseTiming), ShippingControlPlaneError> {
""",
    "active projection return type",
)
replace_once(
    """    if epoch == 0 {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    Ok(epoch)
}
""",
    """    if epoch == 0 {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    let timing = lease_timing(
        response.projection.idle_expires_at_ms,
        response.projection.hard_expires_at_ms,
    )?;
    Ok((epoch, timing))
}

fn lease_timing(
    idle_expires_at_ms: Option<u64>,
    hard_expires_at_ms: Option<u64>,
) -> Result<ControlPlaneLeaseTiming, ShippingControlPlaneError> {
    let idle_expires_at_ms =
        idle_expires_at_ms.ok_or(ShippingControlPlaneError::InvalidResponse)?;
    let hard_expires_at_ms =
        hard_expires_at_ms.ok_or(ShippingControlPlaneError::InvalidResponse)?;
    if idle_expires_at_ms == 0
        || hard_expires_at_ms == 0
        || idle_expires_at_ms > hard_expires_at_ms
    {
        return Err(ShippingControlPlaneError::InvalidResponse);
    }
    Ok(ControlPlaneLeaseTiming {
        idle_expires_at_ms,
        hard_expires_at_ms,
    })
}
""",
    "lease timing validation",
)
replace_once(
    """        ControlPlaneEnrollment, MachineHttpMethod, MachineHttpPort, MachineHttpResponse,
        ShippingControlPlaneError,
""",
    """        ControlPlaneEnrollment, ControlPlaneLeaseTiming, MachineHttpMethod, MachineHttpPort,
        MachineHttpResponse, ShippingControlPlaneError, lease_timing,
""",
    "test imports",
)
replace_once(
    """    #[test]
    fn enrollment_uses_canonical_projection_and_rechecks_local_device()
""",
    """    #[test]
    fn active_lease_timing_is_server_owned_and_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            lease_timing(Some(30_000), Some(900_000))?,
            ControlPlaneLeaseTiming {
                idle_expires_at_ms: 30_000,
                hard_expires_at_ms: 900_000,
            }
        );
        assert_eq!(
            lease_timing(None, Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(30_000), None),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(0), Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        assert_eq!(
            lease_timing(Some(900_001), Some(900_000)),
            Err(ShippingControlPlaneError::InvalidResponse)
        );
        Ok(())
    }

    #[test]
    fn enrollment_uses_canonical_projection_and_rechecks_local_device()
""",
    "timing tests",
)

path.write_text(text, encoding="utf-8")
