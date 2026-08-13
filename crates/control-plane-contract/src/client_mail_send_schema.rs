use crate::client_mail_send_schema_common::schema_ref;
use crate::client_mail_send_schema_request::request_schema;
use serde_json::{Value, json};

pub fn openapi_fragment() -> Value {
    json!({"components": {"schemas": {
        "ClientMailSendOperationDto": {"type": "string", "enum": ["NEW", "REPLY", "REPLY_ALL", "FORWARD"]},
        "ClientMailSendRequestDto": request_schema(),
        "ClientMailSendStateDto": {"type": "string", "enum": ["PENDING", "DISPATCHING", "RETRYABLE", "SENT", "AMBIGUOUS", "REJECTED"]},
        "ClientMailSendReceiptDto": receipt_schema()
    }}})
}

fn receipt_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["intentId", "state", "attemptCount", "replayed"],
        "properties": {
            "intentId": {"type": "string"},
            "state": schema_ref("ClientMailSendStateDto"),
            "attemptCount": {"type": "integer"},
            "replayed": {"type": "boolean"}
        }
    })
}
