use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientMailSearchInput {
    pub mailbox_binding_id: String,
    pub term: Option<String>,
    pub cursor: Option<String>,
    pub limit: u16,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxMessageReferenceDto {
    pub mailbox_binding_id: String,
    pub provider_reference: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailMessageSummaryDto {
    pub reference: MailboxMessageReferenceDto,
    pub subject: Option<String>,
    pub sender: Option<String>,
    pub received_at_ms: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailMessageSearchPageDto {
    pub messages: Vec<MailMessageSummaryDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailMessageBodyDto {
    pub summary: MailMessageSummaryDto,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
}

fn canonical_fragment() -> Value {
    json!({
        "paths": {},
        "components": {
            "schemas": {
                "ClientMailSearchInput": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["mailboxBindingId", "term", "cursor", "limit"],
                    "properties": {
                        "mailboxBindingId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "term": {"type": "string", "nullable": true, "maxLength": 200},
                        "cursor": {"type": "string", "nullable": true, "maxLength": 512},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                    }
                },
                "MailboxMessageReferenceDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["mailboxBindingId", "providerReference"],
                    "properties": {
                        "mailboxBindingId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "providerReference": {"type": "string", "minLength": 1, "maxLength": 512}
                    }
                },
                "MailMessageSummaryDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["reference", "subject", "sender", "receivedAtMs"],
                    "properties": {
                        "reference": {"$ref": "#/components/schemas/MailboxMessageReferenceDto"},
                        "subject": {"type": "string", "nullable": true},
                        "sender": {"type": "string", "nullable": true},
                        "receivedAtMs": {"type": "integer", "minimum": 0}
                    }
                },
                "MailMessageSearchPageDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["messages", "nextCursor"],
                    "properties": {
                        "messages": {
                            "type": "array",
                            "maxItems": 100,
                            "items": {"$ref": "#/components/schemas/MailMessageSummaryDto"}
                        },
                        "nextCursor": {"type": "string", "nullable": true, "maxLength": 512}
                    }
                },
                "MailMessageBodyDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["summary", "textBody", "htmlBody"],
                    "properties": {
                        "summary": {"$ref": "#/components/schemas/MailMessageSummaryDto"},
                        "textBody": {"type": "string", "nullable": true, "maxLength": 1048576},
                        "htmlBody": {"type": "string", "nullable": true, "maxLength": 1048576}
                    }
                }
            }
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&canonical_fragment())?);
    Ok(())
}
