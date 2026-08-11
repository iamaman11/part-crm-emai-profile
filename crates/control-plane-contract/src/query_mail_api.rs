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

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/search": {
                "post": post_operation("searchClientMail", "ClientMailSearchInput", "MailMessageSearchPageDto")
            },
            "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/message": {
                "post": post_operation("getClientMailMessage", "MailboxMessageReferenceDto", "MailMessageBodyDto")
            }
        },
        "components": {
            "schemas": {
                "ClientMailSearchInput": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["mailboxBindingId", "term", "cursor", "limit"],
                    "properties": {
                        "mailboxBindingId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "term": {"type": "string", "nullable": true, "minLength": 1, "maxLength": 200},
                        "cursor": {"type": "string", "nullable": true, "minLength": 1, "maxLength": 512},
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
                        "reference": schema_ref("MailboxMessageReferenceDto"),
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
                            "items": schema_ref("MailMessageSummaryDto")
                        },
                        "nextCursor": {"type": "string", "nullable": true, "maxLength": 512}
                    }
                },
                "MailMessageBodyDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["summary", "textBody", "htmlBody"],
                    "properties": {
                        "summary": schema_ref("MailMessageSummaryDto"),
                        "textBody": {"type": "string", "nullable": true, "maxLength": 1048576},
                        "htmlBody": {"type": "string", "nullable": true, "maxLength": 1048576}
                    }
                }
            }
        }
    })
}

fn post_operation(operation_id: &str, request_schema: &str, response_schema: &str) -> Value {
    json!({
        "operationId": operation_id,
        "parameters": [
            {
                "name": "tenantId",
                "in": "path",
                "required": true,
                "schema": {"type": "string", "minLength": 8, "maxLength": 96}
            },
            {
                "name": "clientId",
                "in": "path",
                "required": true,
                "schema": {"type": "string", "minLength": 8, "maxLength": 96}
            }
        ],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema_ref(request_schema)
                }
            }
        },
        "responses": {
            "200": {
                "description": "Authorized transient Client Mail query response",
                "content": {
                    "application/json": {
                        "schema": schema_ref(response_schema)
                    }
                }
            },
            "400": problem_response(),
            "404": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": {"type": "object"}}}
    })
}

#[cfg(test)]
mod tests {
    use super::{ClientMailSearchInput, openapi_fragment};

    #[test]
    fn client_mail_contract_uses_post_bodies_not_sensitive_query_parameters() {
        let document = openapi_fragment();
        for path in [
            "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/search",
            "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/message",
        ] {
            let operation = &document["paths"][path]["post"];
            assert!(operation.is_object());
            assert!(operation["requestBody"].is_object());
            assert!(operation["parameters"].as_array().is_some_and(|parameters| {
                parameters.iter().all(|parameter| parameter["in"] == "path")
            }));
        }
    }

    #[test]
    fn nullable_transient_fields_remain_explicit_on_wire() -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::to_value(ClientMailSearchInput {
            mailbox_binding_id: "binding_01JMAILQUERY".to_owned(),
            term: None,
            cursor: None,
            limit: 25,
        })?;
        assert!(value.get("term").is_some());
        assert!(value.get("cursor").is_some());
        Ok(())
    }
}
