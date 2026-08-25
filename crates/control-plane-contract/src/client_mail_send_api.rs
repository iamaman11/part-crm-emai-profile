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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientMailSendReceiptDto {
    pub intent_id: String,
    pub state: ClientMailSendStateDto,
    pub attempt_count: u8,
    pub replayed: bool,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/send": {
                "post": post_operation()
            }
        },
        "components": {
            "schemas": {
                "ClientMailSendOperationDto": {
                    "type": "string",
                    "enum": ["NEW", "REPLY", "REPLY_ALL", "FORWARD"]
                },
                "ClientMailSendRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "mailboxBindingId",
                        "operation",
                        "sourceProviderReference",
                        "to",
                        "cc",
                        "bcc",
                        "subject",
                        "textBody",
                        "htmlBody"
                    ],
                    "properties": {
                        "mailboxBindingId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "operation": schema_ref("ClientMailSendOperationDto"),
                        "sourceProviderReference": nullable_string(1, 512),
                        "to": address_array(),
                        "cc": address_array(),
                        "bcc": address_array(),
                        "subject": nullable_string(0, 998),
                        "textBody": nullable_string(0, 1048576),
                        "htmlBody": nullable_string(0, 1048576)
                    }
                },
                "ClientMailSendStateDto": {
                    "type": "string",
                    "enum": [
                        "PENDING",
                        "DISPATCHING",
                        "RETRYABLE",
                        "SENT",
                        "AMBIGUOUS",
                        "REJECTED"
                    ]
                },
                "ClientMailSendReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["intentId", "state", "attemptCount", "replayed"],
                    "properties": {
                        "intentId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "state": schema_ref("ClientMailSendStateDto"),
                        "attemptCount": {"type": "integer", "minimum": 0, "maximum": 255},
                        "replayed": {"type": "boolean"}
                    }
                }
            }
        }
    })
}

fn post_operation() -> Value {
    json!({
        "operationId": "sendClientMail",
        "security": [{"cloudflareAccessJwt": []}],
        "parameters": [
            path_parameter("tenantId"),
            path_parameter("clientId"),
            component_parameter("CorrelationHeader"),
            component_parameter("IdempotencyHeader")
        ],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema_ref("ClientMailSendRequestDto")
                }
            }
        },
        "responses": {
            "200": {
                "description": "Retry-safe Client Mail send receipt",
                "content": {
                    "application/json": {
                        "schema": schema_ref("ClientMailSendReceiptDto")
                    }
                }
            },
            "400": problem_response("Invalid request"),
            "404": problem_response("Not found"),
            "409": problem_response("Idempotency conflict"),
            "500": problem_response("Internal failure"),
            "503": problem_response("Dependency unavailable")
        }
    })
}

fn path_parameter(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": {"type": "string", "minLength": 8, "maxLength": 96}
    })
}

fn component_parameter(name: &str) -> Value {
    json!({"$ref": format!("#/components/parameters/{name}")})
}

fn nullable_string(min_length: u64, max_length: u64) -> Value {
    json!({
        "oneOf": [
            {"type": "string", "minLength": min_length, "maxLength": max_length},
            {"type": "null"}
        ]
    })
}

fn address_array() -> Value {
    json!({
        "type": "array",
        "maxItems": 100,
        "items": {"type": "string", "minLength": 3, "maxLength": 320}
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn problem_response(description: &str) -> Value {
    json!({
        "description": description,
        "content": {"application/problem+json": {"schema": schema_ref("Problem")}}
    })
}

#[cfg(test)]
mod tests {
    use super::{ClientMailSendOperationDto, ClientMailSendRequestDto, openapi_fragment};
    use serde_json::Value;

    #[test]
    fn request_contract_rejects_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
        let request = ClientMailSendRequestDto {
            mailbox_binding_id: "binding_01JMAILSEND".to_owned(),
            operation: ClientMailSendOperationDto::New,
            source_provider_reference: None,
            to: vec!["client@example.test".to_owned()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: Some("Subject".to_owned()),
            text_body: Some("Body".to_owned()),
            html_body: None,
        };
        let mut value = serde_json::to_value(request)?;
        value["unexpectedField"] = Value::Bool(true);
        assert!(serde_json::from_value::<ClientMailSendRequestDto>(value).is_err());
        Ok(())
    }

    #[test]
    fn send_contract_has_governed_idempotent_browser_semantics()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = openapi_fragment();
        let operation =
            &document["paths"]["/api/v1/tenants/{tenantId}/clients/{clientId}/mail/send"]["post"];
        let Some(parameters) = operation["parameters"].as_array() else {
            return Err(std::io::Error::other("operation parameters missing").into());
        };
        assert_eq!(parameters.len(), 4);
        assert_eq!(operation["security"][0]["cloudflareAccessJwt"], json!([]));
        assert_eq!(
            parameters[3]["$ref"],
            "#/components/parameters/IdempotencyHeader"
        );
        assert!(operation["requestBody"].is_object());
        assert_eq!(
            operation["responses"]["409"]["content"]["application/problem+json"]["schema"]["$ref"],
            "#/components/schemas/Problem"
        );
        Ok(())
    }
}
