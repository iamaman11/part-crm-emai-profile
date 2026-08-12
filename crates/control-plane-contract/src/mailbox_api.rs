use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAILBOX_PROVIDERS: [&str; 4] = [
    "GMAIL_API",
    "IMAP",
    "BROWSER_FALLBACK",
    "MICROSOFT_GRAPH",
];
pub const MAILBOX_BINDING_STATUSES: [&str; 4] = ["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"];
pub const MAILBOX_JOB_STATUSES: [&str; 8] = [
    "SCHEDULED",
    "QUEUED",
    "RUNNING",
    "RETRY_PENDING",
    "AUTH_REQUIRED",
    "SUSPENDED",
    "SUCCEEDED",
    "FAILED",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MailboxProviderDto {
    GmailApi,
    Imap,
    BrowserFallback,
    MicrosoftGraph,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MailboxBindingStatusDto {
    Active,
    AuthRequired,
    Suspended,
    Revoked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MailboxJobStatusDto {
    Scheduled,
    Queued,
    Running,
    RetryPending,
    AuthRequired,
    Suspended,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxBindingProjectionDto {
    pub binding_id: String,
    pub provider: MailboxProviderDto,
    pub status: MailboxBindingStatusDto,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMailboxBindingRequestDto {
    pub binding_id: String,
    pub provider: String,
    pub secret_handle: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeMailboxBindingRequestDto {
    pub expected_binding_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindBrowserMailboxExecutionRequestDto {
    pub profile_id: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserExecutionBindingReceiptDto {
    pub binding_id: String,
    pub profile_id: String,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxJobProjectionDto {
    pub job_id: String,
    pub status: MailboxJobStatusDto,
    pub attempt: u32,
    pub max_attempts: u32,
    pub next_run_at_ms: u64,
    pub provider_status: Option<String>,
    pub bounded_item_count: u32,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMailboxJobRequestDto {
    pub job_id: String,
    pub cursor: Option<String>,
    pub delay_ms: u64,
    pub max_attempts: u32,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunMailboxJobRequestDto {
    pub expected_job_version: u64,
    pub request_digest: String,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {},
        "components": {
            "schemas": {
                "MailboxProviderDto": string_enum(&MAILBOX_PROVIDERS),
                "MailboxBindingStatusDto": string_enum(&MAILBOX_BINDING_STATUSES),
                "MailboxJobStatusDto": string_enum(&MAILBOX_JOB_STATUSES),
                "MailboxBindingProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["bindingId", "provider", "status", "version"],
                    "properties": {
                        "bindingId": string_schema(),
                        "provider": schema_ref("MailboxProviderDto"),
                        "status": schema_ref("MailboxBindingStatusDto"),
                        "version": positive_version_schema()
                    }
                },
                "CreateMailboxBindingRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["bindingId", "provider", "secretHandle", "requestDigest"],
                    "properties": {
                        "bindingId": string_schema(),
                        "provider": schema_ref("MailboxProviderDto"),
                        "secretHandle": string_schema(),
                        "requestDigest": sha256_schema()
                    }
                },
                "RevokeMailboxBindingRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedBindingVersion", "requestDigest"],
                    "properties": {
                        "expectedBindingVersion": positive_version_schema(),
                        "requestDigest": sha256_schema()
                    }
                },
                "BindBrowserMailboxExecutionRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["profileId", "requestDigest"],
                    "properties": {
                        "profileId": string_schema(),
                        "requestDigest": sha256_schema()
                    }
                },
                "BrowserExecutionBindingReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["bindingId", "profileId", "replayed"],
                    "properties": {
                        "bindingId": string_schema(),
                        "profileId": string_schema(),
                        "replayed": {"type": "boolean"}
                    }
                },
                "MailboxJobProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["jobId", "status", "attempt", "maxAttempts", "nextRunAtMs", "providerStatus", "boundedItemCount", "version"],
                    "properties": {
                        "jobId": string_schema(),
                        "status": schema_ref("MailboxJobStatusDto"),
                        "attempt": {"type": "integer", "minimum": 0},
                        "maxAttempts": {"type": "integer", "minimum": 1, "maximum": 10},
                        "nextRunAtMs": {"type": "integer", "minimum": 0},
                        "providerStatus": {"type": "string", "nullable": true},
                        "boundedItemCount": {"type": "integer", "minimum": 0},
                        "version": positive_version_schema()
                    }
                },
                "CreateMailboxJobRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["jobId", "cursor", "delayMs", "maxAttempts", "requestDigest"],
                    "properties": {
                        "jobId": string_schema(),
                        "cursor": {"type": "string", "nullable": true, "maxLength": 512},
                        "delayMs": {"type": "integer", "minimum": 0, "maximum": 604800000},
                        "maxAttempts": {"type": "integer", "minimum": 1, "maximum": 10},
                        "requestDigest": sha256_schema()
                    }
                },
                "RunMailboxJobRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expectedJobVersion", "requestDigest"],
                    "properties": {
                        "expectedJobVersion": positive_version_schema(),
                        "requestDigest": sha256_schema()
                    }
                }
            }
        }
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

fn positive_version_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

#[cfg(test)]
mod tests {
    use super::{
        BindBrowserMailboxExecutionRequestDto, CreateMailboxBindingRequestDto,
        CreateMailboxJobRequestDto, MailboxBindingProjectionDto, MailboxBindingStatusDto,
        MailboxJobProjectionDto, MailboxJobStatusDto, MailboxProviderDto, openapi_fragment,
    };
    use serde_json::{Value, json};

    #[test]
    fn mailbox_requests_reject_unknown_and_sensitive_fields() {
        let digest = "a".repeat(64);
        let valid_binding = format!(
            r#"{{"bindingId":"mailbox_01JTEST","provider":"IMAP","secretHandle":"secret_01JTEST","requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxBindingRequestDto>(&valid_binding).is_ok());
        for forbidden in ["password", "messageBody"] {
            let invalid = format!(
                r#"{{"bindingId":"mailbox_01JTEST","provider":"IMAP","secretHandle":"secret_01JTEST","requestDigest":"{digest}","{forbidden}":"forbidden"}}"#
            );
            assert!(serde_json::from_str::<CreateMailboxBindingRequestDto>(&invalid).is_err());
        }

        let browser = format!(
            r#"{{"profileId":"profile_01JTEST","requestDigest":"{digest}","deviceId":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<BindBrowserMailboxExecutionRequestDto>(&browser).is_err());

        let job = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":0,"maxAttempts":3,"requestDigest":"{digest}","messageBody":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequestDto>(&job).is_err());
    }

    #[test]
    fn canonical_status_sets_match_runtime_wire_values() -> Result<(), Box<dyn std::error::Error>> {
        let binding = serde_json::to_value(MailboxBindingProjectionDto {
            binding_id: "mailbox_01JTEST".to_owned(),
            provider: MailboxProviderDto::BrowserFallback,
            status: MailboxBindingStatusDto::AuthRequired,
            version: 2,
        })?;
        assert_eq!(binding["provider"], "BROWSER_FALLBACK");
        assert_eq!(binding["status"], "AUTH_REQUIRED");

        let graph_provider = serde_json::to_value(MailboxProviderDto::MicrosoftGraph)?;
        assert_eq!(graph_provider, "MICROSOFT_GRAPH");

        let job = serde_json::to_value(MailboxJobProjectionDto {
            job_id: "mailjob_01JTEST".to_owned(),
            status: MailboxJobStatusDto::Scheduled,
            attempt: 0,
            max_attempts: 3,
            next_run_at_ms: 0,
            provider_status: None,
            bounded_item_count: 0,
            version: 1,
        })?;
        assert_eq!(job["status"], "SCHEDULED");
        assert!(job.get("providerStatus").is_some_and(Value::is_null));
        Ok(())
    }

    #[test]
    fn fragment_is_schema_only_and_exposes_complete_status_enums() {
        let document = openapi_fragment();
        assert_eq!(document["paths"], json!({}));
        assert_eq!(
            document["components"]["schemas"]["MailboxProviderDto"]["enum"],
            json!(["GMAIL_API", "IMAP", "BROWSER_FALLBACK", "MICROSOFT_GRAPH"])
        );
        assert_eq!(
            document["components"]["schemas"]["MailboxBindingStatusDto"]["enum"],
            json!(["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"])
        );
        assert_eq!(
            document["components"]["schemas"]["MailboxJobStatusDto"]["enum"],
            json!([
                "SCHEDULED",
                "QUEUED",
                "RUNNING",
                "RETRY_PENDING",
                "AUTH_REQUIRED",
                "SUSPENDED",
                "SUCCEEDED",
                "FAILED"
            ])
        );
    }
}
