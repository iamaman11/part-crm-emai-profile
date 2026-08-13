use super::{RenderContext, render_mime};
use application_ports::outbound_mail::{MailAddress, MailBody, MailRecipients, MailSubject};

fn recipients() -> Result<MailRecipients, Box<dyn std::error::Error>> {
    Ok(MailRecipients::new(
        vec![MailAddress::parse("to@example.com")?],
        vec![MailAddress::parse("cc@example.com")?],
        vec![MailAddress::parse("bcc@example.com")?],
    )?)
}

fn render(body: MailBody, subject: &MailSubject) -> Result<super::RenderedMessage, std::io::Error> {
    let recipients = recipients().map_err(|_| std::io::Error::other("recipients"))?;
    let context = RenderContext {
        sender: "sender@example.com",
        recipients: &recipients,
        subject: Some(subject),
        fallback_subject: None,
        in_reply_to: Some("<source@example.com>"),
        references: Some("<root@example.com> <source@example.com>"),
    };
    render_mime(&context, &body).map_err(|()| std::io::Error::other("render failed"))
}

#[test]
fn multipart_render_is_deterministic_encoded_and_bcc_is_envelope_only()
-> Result<(), Box<dyn std::error::Error>> {
    let subject = MailSubject::parse("Привет")?;
    let body = MailBody::new(
        Some("plain body".to_owned()),
        Some("<b>html body</b>".to_owned()),
    )?;
    let first = render(body.clone(), &subject)?;
    let second = render(body, &subject)?;
    assert_eq!(first.bytes, second.bytes);
    let text = String::from_utf8(first.bytes)?;
    assert!(text.contains("multipart/alternative"));
    assert!(text.contains("Content-Transfer-Encoding: base64"));
    assert!(text.contains("=?UTF-8?B?"));
    assert!(!text.contains("plain body"));
    assert!(!text.contains("Bcc:"));
    assert_eq!(first.envelope_recipients.len(), 3);
    Ok(())
}

#[test]
fn text_only_and_html_only_are_base64_encoded() -> Result<(), Box<dyn std::error::Error>> {
    let subject = MailSubject::parse("Subject")?;
    let text = render(
        MailBody::new(Some("plain body".to_owned()), None)?,
        &subject,
    )?;
    let html = render(
        MailBody::new(None, Some("<b>html body</b>".to_owned()))?,
        &subject,
    )?;
    let text = String::from_utf8(text.bytes)?;
    let html = String::from_utf8(html.bytes)?;
    assert!(text.contains("Content-Type: text/plain"));
    assert!(html.contains("Content-Type: text/html"));
    assert!(!text.contains("plain body"));
    assert!(!html.contains("html body"));
    Ok(())
}

#[test]
fn long_ascii_subject_is_folded_as_encoded_words() -> Result<(), Box<dyn std::error::Error>> {
    let subject = MailSubject::parse("a".repeat(200))?;
    let message = render(MailBody::new(Some("body".to_owned()), None)?, &subject)?;
    let text = String::from_utf8(message.bytes)?;
    assert!(text.contains("Subject: =?UTF-8?B?"));
    assert!(text.contains("\r\n "));
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
