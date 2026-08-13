use application_ports::outbound_mail::{
    MailAddress, MailBody, MailRecipients, MailSubject, OutboundMailIntent, OutboundMailOperation,
    OutboundMailSourceReference, ProviderMessageReference,
};
use control_plane_contract::client_mail_send_api::{
    ClientMailSendOperationDto, ClientMailSendRequestDto,
};
use profile_platform_primitives::{ClientId, MailboxBindingId};
use std::collections::HashSet;

pub(super) fn build_intent(
    client_id: &ClientId,
    request: &ClientMailSendRequestDto,
) -> Result<OutboundMailIntent, ()> {
    let binding_id = MailboxBindingId::parse(request.mailbox_binding_id.clone()).map_err(|_| ())?;
    let body = MailBody::new(request.text_body.clone(), request.html_body.clone()).map_err(|_| ())?;
    let subject = request
        .subject
        .clone()
        .map(MailSubject::parse)
        .transpose()
        .map_err(|_| ())?;
    let operation = match request.operation {
        ClientMailSendOperationDto::New => {
            if request.source_provider_reference.is_some() {
                return Err(());
            }
            OutboundMailOperation::New {
                recipients: explicit_recipients(request)?,
            }
        }
        ClientMailSendOperationDto::Reply => {
            reject_explicit_recipients(request)?;
            OutboundMailOperation::Reply {
                source: source_reference(&binding_id, request)?,
            }
        }
        ClientMailSendOperationDto::ReplyAll => {
            reject_explicit_recipients(request)?;
            OutboundMailOperation::ReplyAll {
                source: source_reference(&binding_id, request)?,
            }
        }
        ClientMailSendOperationDto::Forward => OutboundMailOperation::Forward {
            source: source_reference(&binding_id, request)?,
            recipients: explicit_recipients(request)?,
        },
    };
    Ok(OutboundMailIntent::new(
        client_id.clone(),
        binding_id,
        operation,
        subject,
        body,
    ))
}

fn reject_explicit_recipients(request: &ClientMailSendRequestDto) -> Result<(), ()> {
    if request.to.is_empty() && request.cc.is_empty() && request.bcc.is_empty() {
        Ok(())
    } else {
        Err(())
    }
}

fn source_reference(
    binding_id: &MailboxBindingId,
    request: &ClientMailSendRequestDto,
) -> Result<OutboundMailSourceReference, ()> {
    let provider_reference = request
        .source_provider_reference
        .clone()
        .ok_or(())
        .and_then(|value| ProviderMessageReference::parse(value).map_err(|_| ()))?;
    Ok(OutboundMailSourceReference::new(
        binding_id.clone(),
        provider_reference,
    ))
}

fn explicit_recipients(request: &ClientMailSendRequestDto) -> Result<MailRecipients, ()> {
    let mut seen = HashSet::new();
    let to = addresses(&request.to, &mut seen)?;
    let cc = addresses(&request.cc, &mut seen)?;
    let bcc = addresses(&request.bcc, &mut seen)?;
    MailRecipients::new(to, cc, bcc).map_err(|_| ())
}

fn addresses(values: &[String], seen: &mut HashSet<String>) -> Result<Vec<MailAddress>, ()> {
    let mut addresses = Vec::with_capacity(values.len());
    for value in values {
        let address = MailAddress::parse(value.clone()).map_err(|_| ())?;
        if seen.insert(address.as_str().to_ascii_lowercase()) {
            addresses.push(address);
        }
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::build_intent;
    use control_plane_contract::client_mail_send_api::{
        ClientMailSendOperationDto, ClientMailSendRequestDto,
    };
    use profile_platform_primitives::ClientId;

    fn request(operation: ClientMailSendOperationDto) -> ClientMailSendRequestDto {
        ClientMailSendRequestDto {
            mailbox_binding_id: "binding_01JMAILSEND".to_owned(),
            operation,
            source_provider_reference: None,
            to: vec!["client@example.test".to_owned()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: Some("Subject".to_owned()),
            text_body: Some("Body".to_owned()),
            html_body: None,
        }
    }

    #[test]
    fn new_and_forward_require_explicit_recipient_intent() -> Result<(), Box<dyn std::error::Error>> {
        let client_id = ClientId::parse("client_01JMAILSEND")?;
        assert!(build_intent(&client_id, &request(ClientMailSendOperationDto::New)).is_ok());

        let mut forward = request(ClientMailSendOperationDto::Forward);
        forward.source_provider_reference = Some("message-1".to_owned());
        assert!(build_intent(&client_id, &forward).is_ok());
        Ok(())
    }

    #[test]
    fn reply_modes_reject_browser_supplied_recipients() -> Result<(), Box<dyn std::error::Error>> {
        let client_id = ClientId::parse("client_01JMAILSEND")?;
        for operation in [
            ClientMailSendOperationDto::Reply,
            ClientMailSendOperationDto::ReplyAll,
        ] {
            let mut reply = request(operation);
            reply.source_provider_reference = Some("message-1".to_owned());
            assert!(build_intent(&client_id, &reply).is_err());
        }
        Ok(())
    }

    #[test]
    fn explicit_recipients_are_validated_and_deduplicated_across_fields(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client_id = ClientId::parse("client_01JMAILSEND")?;
        let mut compose = request(ClientMailSendOperationDto::New);
        compose.to = vec!["person@example.test".to_owned()];
        compose.cc = vec!["PERSON@example.test".to_owned()];
        assert!(build_intent(&client_id, &compose).is_ok());
        compose.bcc = vec!["bad address@example.test".to_owned()];
        assert!(build_intent(&client_id, &compose).is_err());
        Ok(())
    }
}
