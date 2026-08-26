use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::{ClientMailSendOperationDto, ClientMailSendRequestDto};
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
}
