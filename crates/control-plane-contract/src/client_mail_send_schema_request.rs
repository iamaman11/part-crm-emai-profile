use crate::client_mail_send_schema_common::{address_array, schema_ref};
use serde_json::{Value, json};

pub(crate) fn request_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "mailboxBindingId", "operation", "sourceProviderReference",
            "to", "cc", "bcc", "subject", "textBody", "htmlBody"
        ],
        "properties": {
            "mailboxBindingId": {"type": "string", "minLength": 8, "maxLength": 96},
            "operation": schema_ref("ClientMailSendOperationDto"),
            "sourceProviderReference": {"type": "string", "nullable": true, "minLength": 1, "maxLength": 512},
            "to": address_array(),
            "cc": address_array(),
            "bcc": address_array(),
            "subject": {"type": "string", "nullable": true, "maxLength": 998},
            "textBody": {"type": "string", "nullable": true, "maxLength": 1048576},
            "htmlBody": {"type": "string", "nullable": true, "maxLength": 1048576}
        }
    })
}
