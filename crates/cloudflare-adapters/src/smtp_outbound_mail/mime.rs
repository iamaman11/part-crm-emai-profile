mod encoding;
#[cfg(test)]
mod tests;

use application_ports::outbound_mail::{MailBody, MailRecipients, MailSubject};
use encoding::{
    push_address_header, push_alternative_part, push_header, push_reference_header,
    push_single_body, push_subject_header, validate_header_value,
};
use sha2::{Digest, Sha256};

const MAX_RENDERED_MESSAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_REFERENCE_HEADER_BYTES: usize = 8 * 1024;

pub(super) struct RenderContext<'a> {
    pub sender: &'a str,
    pub recipients: &'a MailRecipients,
    pub subject: Option<&'a MailSubject>,
    pub fallback_subject: Option<&'a str>,
    pub in_reply_to: Option<&'a str>,
    pub references: Option<&'a str>,
}

pub(super) struct RenderedMessage {
    pub bytes: Vec<u8>,
    pub envelope_recipients: Vec<String>,
}

pub(super) fn render_mime(
    context: &RenderContext<'_>,
    body: &MailBody,
) -> Result<RenderedMessage, ()> {
    validate_address(context.sender)?;
    let envelope_recipients = envelope_recipients(context.recipients)?;
    for address in context
        .recipients
        .to()
        .iter()
        .chain(context.recipients.cc())
    {
        validate_address(address.as_str())?;
    }
    let subject = context
        .subject
        .map(MailSubject::as_str)
        .or(context.fallback_subject)
        .unwrap_or("");
    validate_header_value(subject)?;
    if let Some(value) = context.in_reply_to {
        validate_reference_header(value)?;
    }
    if let Some(value) = context.references {
        validate_reference_header(value)?;
    }

    let mut output = String::new();
    push_header(&mut output, "From", context.sender);
    push_address_header(&mut output, "To", context.recipients.to());
    push_address_header(&mut output, "Cc", context.recipients.cc());
    push_subject_header(&mut output, subject)?;
    if let Some(value) = context.in_reply_to {
        push_reference_header(&mut output, "In-Reply-To", value)?;
    }
    if let Some(value) = context.references {
        push_reference_header(&mut output, "References", value)?;
    }
    output.push_str("MIME-Version: 1.0\r\n");

    match (body.text(), body.html()) {
        (Some(text), Some(html)) if !text.is_empty() && !html.is_empty() => {
            let boundary = multipart_boundary(context, body);
            output.push_str("Content-Type: multipart/alternative; boundary=\"");
            output.push_str(&boundary);
            output.push_str("\"\r\n\r\n");
            push_alternative_part(&mut output, &boundary, "text/plain", text);
            push_alternative_part(&mut output, &boundary, "text/html", html);
            output.push_str("--");
            output.push_str(&boundary);
            output.push_str("--\r\n");
        }
        (Some(text), _) if !text.is_empty() => {
            push_single_body(&mut output, "text/plain", text);
        }
        (_, Some(html)) if !html.is_empty() => {
            push_single_body(&mut output, "text/html", html);
        }
        _ => return Err(()),
    }
    if output.len() > MAX_RENDERED_MESSAGE_BYTES {
        return Err(());
    }
    Ok(RenderedMessage {
        bytes: output.into_bytes(),
        envelope_recipients,
    })
}

fn envelope_recipients(recipients: &MailRecipients) -> Result<Vec<String>, ()> {
    let mut output = Vec::new();
    for address in recipients
        .to()
        .iter()
        .chain(recipients.cc())
        .chain(recipients.bcc())
    {
        validate_address(address.as_str())?;
        if !output
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(address.as_str()))
        {
            output.push(address.as_str().to_owned());
        }
    }
    if output.is_empty() {
        return Err(());
    }
    Ok(output)
}

fn validate_address(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || !value.contains('@')
    {
        return Err(());
    }
    Ok(())
}

fn validate_reference_header(value: &str) -> Result<(), ()> {
    if value.len() > MAX_REFERENCE_HEADER_BYTES {
        return Err(());
    }
    validate_header_value(value)
}

fn multipart_boundary(context: &RenderContext<'_>, body: &MailBody) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context.sender.as_bytes());
    for address in context
        .recipients
        .to()
        .iter()
        .chain(context.recipients.cc())
        .chain(context.recipients.bcc())
    {
        hasher.update([0]);
        hasher.update(address.as_str().as_bytes());
    }
    hasher.update([0]);
    hasher.update(
        context
            .subject
            .map(MailSubject::as_str)
            .or(context.fallback_subject)
            .unwrap_or("")
            .as_bytes(),
    );
    if let Some(text) = body.text() {
        hasher.update([0]);
        hasher.update(text.as_bytes());
    }
    if let Some(html) = body.html() {
        hasher.update([0]);
        hasher.update(html.as_bytes());
    }
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        use core::fmt::Write as _;
        let _ = write!(&mut suffix, "{byte:02x}");
    }
    format!("profile-alt-{suffix}")
}
