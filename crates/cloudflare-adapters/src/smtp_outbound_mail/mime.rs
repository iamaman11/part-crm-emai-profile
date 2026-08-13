use application_ports::outbound_mail::{MailBody, MailRecipients, MailSubject};
use sha2::{Digest, Sha256};

const MAX_RENDERED_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

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
    let to = addresses(context.recipients.to())?;
    let cc = addresses(context.recipients.cc())?;
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
    if !to.is_empty() {
        push_header(&mut output, "To", &to.join(", "));
    }
    if !cc.is_empty() {
        push_header(&mut output, "Cc", &cc.join(", "));
    }
    push_header(&mut output, "Subject", subject);
    if let Some(value) = context.in_reply_to {
        push_header(&mut output, "In-Reply-To", value);
    }
    if let Some(value) = context.references {
        push_header(&mut output, "References", value);
    }
    output.push_str("MIME-Version: 1.0\r\n");

    match (body.text(), body.html()) {
        (Some(text), Some(html)) if !text.is_empty() && !html.is_empty() => {
            let boundary = multipart_boundary(context, body);
            output.push_str("Content-Type: multipart/alternative; boundary=\"");
            output.push_str(&boundary);
            output.push_str("\"\r\n\r\n");
            output.push_str("--");
            output.push_str(&boundary);
            output.push_str(
                "\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n",
            );
            push_normalized_body(&mut output, text);
            output.push_str("\r\n--");
            output.push_str(&boundary);
            output.push_str(
                "\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n",
            );
            push_normalized_body(&mut output, html);
            output.push_str("\r\n--");
            output.push_str(&boundary);
            output.push_str("--\r\n");
        }
        (Some(text), _) if !text.is_empty() => {
            output.push_str(
                "Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n",
            );
            push_normalized_body(&mut output, text);
            output.push_str("\r\n");
        }
        (_, Some(html)) if !html.is_empty() => {
            output.push_str(
                "Content-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n",
            );
            push_normalized_body(&mut output, html);
            output.push_str("\r\n");
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

fn addresses(
    input: &[application_ports::outbound_mail::MailAddress],
) -> Result<Vec<String>, ()> {
    input
        .iter()
        .map(|address| {
            validate_address(address.as_str())?;
            Ok(address.as_str().to_owned())
        })
        .collect()
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

fn validate_header_value(value: &str) -> Result<(), ()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_reference_header(value: &str) -> Result<(), ()> {
    if value.len() > 8 * 1024 {
        return Err(());
    }
    validate_header_value(value)
}

fn push_header(output: &mut String, name: &str, value: &str) {
    output.push_str(name);
    output.push_str(": ");
    output.push_str(value);
    output.push_str("\r\n");
}

fn push_normalized_body(output: &mut String, value: &str) {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    for (index, line) in normalized.split('\n').enumerate() {
        if index > 0 {
            output.push_str("\r\n");
        }
        output.push_str(line);
    }
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

#[cfg(test)]
mod tests {
    use super::{RenderContext, render_mime};
    use application_ports::outbound_mail::{MailAddress, MailBody, MailRecipients, MailSubject};

    fn recipients() -> Result<MailRecipients, Box<dyn std::error::Error>> {
        Ok(MailRecipients::new(
            vec![MailAddress::parse("to@example.com")?],
            vec![MailAddress::parse("cc@example.com")?],
            vec![MailAddress::parse("bcc@example.com")?],
        )?)
    }

    #[test]
    fn multipart_render_is_deterministic_and_bcc_is_envelope_only()
    -> Result<(), Box<dyn std::error::Error>> {
        let recipients = recipients()?;
        let subject = MailSubject::parse("Subject")?;
        let body = MailBody::new(
            Some("hello\nworld".to_owned()),
            Some("<b>hello</b>".to_owned()),
        )?;
        let context = RenderContext {
            sender: "sender@example.com",
            recipients: &recipients,
            subject: Some(&subject),
            fallback_subject: None,
            in_reply_to: Some("<source@example.com>"),
            references: Some("<root@example.com> <source@example.com>"),
        };
        let first = render_mime(&context, &body)
            .map_err(|()| std::io::Error::other("render failed"))?;
        let second = render_mime(&context, &body)
            .map_err(|()| std::io::Error::other("render failed"))?;
        assert_eq!(first.bytes, second.bytes);
        let text = String::from_utf8(first.bytes)?;
        assert!(text.contains("\r\n"));
        assert!(!text.contains("Bcc:"));
        assert_eq!(first.envelope_recipients.len(), 3);
        Ok(())
    }

    #[test]
    fn header_injection_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let recipients = recipients()?;
        let body = MailBody::new(Some("body".to_owned()), None)?;
        let context = RenderContext {
            sender: "sender@example.com",
            recipients: &recipients,
            subject: None,
            fallback_subject: Some("ok\r\nBcc: attacker@example.com"),
            in_reply_to: None,
            references: None,
        };
        assert!(render_mime(&context, &body).is_err());
        Ok(())
    }
}
