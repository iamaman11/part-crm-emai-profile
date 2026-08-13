use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMailSendOperationDto {
    New,
    Reply,
    ReplyAll,
    Forward,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientMailSendRequestDto {
    pub mailbox_binding_id: String,
    pub operation: ClientMailSendOperationDto,
    pub source_provider_reference: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: Option<String>,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMailSendStateDto {
    Pending,
    Dispatching,
    Retryable,
    Sent,
    Ambiguous,
    Rejected,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientMailSendReceiptDto {
    pub intent_id: String,
    pub state: ClientMailSendStateDto,
    pub attempt_count: u8,
    pub replayed: bool,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({"components": {"schemas": {}}})
}
