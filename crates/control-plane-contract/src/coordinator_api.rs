use serde::{Deserialize, Serialize};

pub const COORDINATOR_RELEASE_DISPOSITIONS: [&str; 3] = ["clean", "dirty", "uncertain"];
pub const COORDINATOR_STATUSES: [&str; 5] = ["idle", "active", "draining", "dirty", "uncertain"];
pub const COORDINATOR_OUTCOMES: [&str; 10] = [
    "snapshot",
    "launch_intent_issued",
    "lease_claimed",
    "heartbeat_accepted",
    "released",
    "drain_started",
    "timed_out",
    "launch_intent_expired",
    "recovered",
    "no_change",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorReleaseDispositionDto {
    Clean,
    Dirty,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorStatusDto {
    Idle,
    Active,
    Draining,
    Dirty,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorOutcomeDto {
    Snapshot,
    LaunchIntentIssued,
    LeaseClaimed,
    HeartbeatAccepted,
    Released,
    DrainStarted,
    TimedOut,
    LaunchIntentExpired,
    Recovered,
    NoChange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoordinatorCommandRequestDto {
    pub idempotency_key: String,
    pub sequence: u64,
    pub expected_version: u64,
    pub command: CoordinatorCommandDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoordinatorCommandDto {
    IssueLaunchIntent {
        launch_intent_id: String,
        device_id: String,
        expires_in_ms: u64,
    },
    Claim {
        launch_intent_id: String,
        device_id: String,
        session_id: String,
    },
    Heartbeat {
        session_id: String,
        epoch: u64,
        fencing_token: String,
    },
    Release {
        session_id: String,
        epoch: u64,
        fencing_token: String,
        disposition: CoordinatorReleaseDispositionDto,
    },
    BeginDrain,
    MarkRecovered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoordinatorResponseDto {
    pub outcome: CoordinatorOutcomeDto,
    pub version: u64,
    pub sequence: u64,
    pub replayed: bool,
    pub fencing_token: Option<String>,
    pub epoch: Option<u64>,
    pub projection: CoordinatorProjectionDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoordinatorProjectionDto {
    pub tenant_id: String,
    pub profile_id: String,
    pub status: CoordinatorStatusDto,
    pub version: u64,
    pub sequence: u64,
    pub next_epoch: u64,
    pub active_session_id: Option<String>,
    pub active_device_id: Option<String>,
    pub active_epoch: Option<u64>,
    pub idle_expires_at_ms: Option<u64>,
    pub hard_expires_at_ms: Option<u64>,
    pub drain_deadline_ms: Option<u64>,
    pub pending_launch_intent_id: Option<String>,
    pub pending_intent_expires_at_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinatorCommandRequestDto, CoordinatorOutcomeDto, CoordinatorProjectionDto,
        CoordinatorResponseDto, CoordinatorStatusDto,
    };
    use serde_json::Value;

    #[test]
    fn coordinator_requests_preserve_unknown_field_tolerance() {
        let request = r#"{
            "idempotency_key":"idem_01JTEST",
            "sequence":1,
            "expected_version":1,
            "top_level_extension":"accepted",
            "command":{"type":"begin_drain","command_extension":"accepted"}
        }"#;
        assert!(serde_json::from_str::<CoordinatorCommandRequestDto>(request).is_ok());
    }

    #[test]
    fn coordinator_dto_keeps_application_validation_deferred() {
        let request = r#"{
            "idempotency_key":"not-yet-validated",
            "sequence":0,
            "expected_version":0,
            "command":{
                "type":"issue_launch_intent",
                "launch_intent_id":"not-yet-validated",
                "device_id":"not-yet-validated",
                "expires_in_ms":999
            }
        }"#;
        assert!(serde_json::from_str::<CoordinatorCommandRequestDto>(request).is_ok());
    }

    #[test]
    fn canonical_status_and_outcome_values_match_public_wire()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = serde_json::to_value(CoordinatorResponseDto {
            outcome: CoordinatorOutcomeDto::LaunchIntentIssued,
            version: 2,
            sequence: 3,
            replayed: false,
            fencing_token: None,
            epoch: None,
            projection: CoordinatorProjectionDto {
                tenant_id: "tenant_01JTEST".to_owned(),
                profile_id: "profile_01JTEST".to_owned(),
                status: CoordinatorStatusDto::Draining,
                version: 2,
                sequence: 3,
                next_epoch: 4,
                active_session_id: None,
                active_device_id: None,
                active_epoch: None,
                idle_expires_at_ms: None,
                hard_expires_at_ms: None,
                drain_deadline_ms: None,
                pending_launch_intent_id: None,
                pending_intent_expires_at_ms: None,
            },
        })?;
        assert_eq!(response["outcome"], "launch_intent_issued");
        assert_eq!(response["projection"]["status"], "draining");
        assert!(response.get("fencing_token").is_some_and(Value::is_null));
        Ok(())
    }
}
