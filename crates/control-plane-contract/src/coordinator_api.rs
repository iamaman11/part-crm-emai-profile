use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {},
        "components": {
            "schemas": {
                "CoordinatorReleaseDispositionDto": string_enum(&COORDINATOR_RELEASE_DISPOSITIONS),
                "CoordinatorStatusDto": string_enum(&COORDINATOR_STATUSES),
                "CoordinatorOutcomeDto": string_enum(&COORDINATOR_OUTCOMES),
                "CoordinatorCommandDto": {
                    "oneOf": [
                        command_variant(
                            "issue_launch_intent",
                            [
                                ("launch_intent_id", string_schema()),
                                ("device_id", string_schema()),
                                ("expires_in_ms", json!({"type": "integer", "minimum": 1000, "maximum": 300000})),
                            ],
                        ),
                        command_variant(
                            "claim",
                            [
                                ("launch_intent_id", string_schema()),
                                ("device_id", string_schema()),
                                ("session_id", string_schema()),
                            ],
                        ),
                        command_variant(
                            "heartbeat",
                            [
                                ("session_id", string_schema()),
                                ("epoch", non_negative_integer_schema()),
                                ("fencing_token", string_schema()),
                            ],
                        ),
                        command_variant(
                            "release",
                            [
                                ("session_id", string_schema()),
                                ("epoch", non_negative_integer_schema()),
                                ("fencing_token", string_schema()),
                                ("disposition", schema_ref("CoordinatorReleaseDispositionDto")),
                            ],
                        ),
                        command_variant("begin_drain", []),
                        command_variant("mark_recovered", []),
                    ],
                    "discriminator": {"propertyName": "type"}
                },
                "CoordinatorCommandRequestDto": {
                    "type": "object",
                    "required": ["idempotency_key", "sequence", "expected_version", "command"],
                    "properties": {
                        "idempotency_key": string_schema(),
                        "sequence": {"type": "integer", "minimum": 1},
                        "expected_version": positive_version_schema(),
                        "command": schema_ref("CoordinatorCommandDto")
                    }
                },
                "CoordinatorResponseDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "outcome",
                        "version",
                        "sequence",
                        "replayed",
                        "fencing_token",
                        "epoch",
                        "projection"
                    ],
                    "properties": {
                        "outcome": schema_ref("CoordinatorOutcomeDto"),
                        "version": positive_version_schema(),
                        "sequence": non_negative_integer_schema(),
                        "replayed": {"type": "boolean"},
                        "fencing_token": nullable_string_schema(),
                        "epoch": nullable_non_negative_integer_schema(),
                        "projection": schema_ref("CoordinatorProjectionDto")
                    }
                },
                "CoordinatorProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "tenant_id",
                        "profile_id",
                        "status",
                        "version",
                        "sequence",
                        "next_epoch",
                        "active_session_id",
                        "active_device_id",
                        "active_epoch",
                        "idle_expires_at_ms",
                        "hard_expires_at_ms",
                        "drain_deadline_ms",
                        "pending_launch_intent_id",
                        "pending_intent_expires_at_ms"
                    ],
                    "properties": {
                        "tenant_id": string_schema(),
                        "profile_id": string_schema(),
                        "status": schema_ref("CoordinatorStatusDto"),
                        "version": positive_version_schema(),
                        "sequence": non_negative_integer_schema(),
                        "next_epoch": non_negative_integer_schema(),
                        "active_session_id": nullable_string_schema(),
                        "active_device_id": nullable_string_schema(),
                        "active_epoch": nullable_non_negative_integer_schema(),
                        "idle_expires_at_ms": nullable_non_negative_integer_schema(),
                        "hard_expires_at_ms": nullable_non_negative_integer_schema(),
                        "drain_deadline_ms": nullable_non_negative_integer_schema(),
                        "pending_launch_intent_id": nullable_string_schema(),
                        "pending_intent_expires_at_ms": nullable_non_negative_integer_schema()
                    }
                }
            }
        }
    })
}

fn command_variant<const N: usize>(kind: &str, fields: [(&str, Value); N]) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert("type".to_owned(), json!({"type": "string", "enum": [kind]}));
    let mut required = vec![Value::String("type".to_owned())];
    for (name, schema) in fields {
        properties.insert(name.to_owned(), schema);
        required.push(Value::String(name.to_owned()));
    }
    json!({
        "type": "object",
        "required": required,
        "properties": properties
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn string_schema() -> Value {
    json!({"type": "string"})
}

fn nullable_string_schema() -> Value {
    json!({"type": "string", "nullable": true})
}

fn positive_version_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn non_negative_integer_schema() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn nullable_non_negative_integer_schema() -> Value {
    json!({"type": "integer", "minimum": 0, "nullable": true})
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinatorCommandRequestDto, CoordinatorOutcomeDto, CoordinatorProjectionDto,
        CoordinatorResponseDto, CoordinatorStatusDto, openapi_fragment,
    };
    use serde_json::{Value, json};

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

    #[test]
    fn fragment_is_schema_only_and_documents_tagged_union_and_deferred_bounds() {
        let document = openapi_fragment();
        assert_eq!(document["paths"], json!({}));
        assert_eq!(
            document["components"]["schemas"]["CoordinatorCommandDto"]["discriminator"]["propertyName"],
            "type"
        );
        assert_eq!(
            document["components"]["schemas"]["CoordinatorCommandDto"]["oneOf"][0]["properties"]["expires_in_ms"]
                ["minimum"],
            1000
        );
        assert!(
            document["components"]["schemas"]["CoordinatorCommandRequestDto"]
                .get("additionalProperties")
                .is_none(),
            "request schema must remain unknown-field tolerant"
        );
    }
}
