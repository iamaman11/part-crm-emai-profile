use super::contract::{ClientMailSendOperationDto, ClientMailSendRequestDto};
use application_ports::outbound_mail::{
    MailAddress, MailBody, MailRecipients, MailSubject, OutboundMailIntent, OutboundMailOperation,
    OutboundMailSourceReference, ProviderMessageReference,
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
    use super::super::contract::{ClientMailSendOperationDto, ClientMailSendRequestDto};
    use profile_platform_primitives::ClientId;

    #[test]
    fn reply_rejects_browser_supplied_recipients() -> Result<(), Box<dyn std::error::Error>> {
        let client_id = ClientId::parse("client_01JMAILSEND")?;
        let request = ClientMailSendRequestDto {
            mailbox_binding_id: "binding_01JMAILSEND".to_owned(),
            operation: ClientMailSendOperationDto::Reply,
            source_provider_reference: Some("message-1".to_owned()),
            to: vec!["cross-client@example.test".to_owned()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: None,
            text_body: Some("reply".to_owned()),
            html_body: None,
        };
        assert!(build_intent(&client_id, &request).is_err());
        Ok(())
    }
}
