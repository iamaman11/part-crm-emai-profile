use super::{forward_context, parse_reference};
use application_ports::outbound_mail::{MailAddress, MailRecipients};

#[test]
fn forward_fixture_preserves_explicit_envelope_only() -> Result<(), Box<dyn std::error::Error>> {
    let recipients = MailRecipients::new(
        vec![MailAddress::parse("to@example.com")?],
        vec![MailAddress::parse("cc@example.com")?],
        vec![MailAddress::parse("bcc@example.com")?],
    )?;
    let context = forward_context(&recipients);
    assert_eq!(context.recipients.to()[0].as_str(), "to@example.com");
    assert_eq!(context.recipients.cc()[0].as_str(), "cc@example.com");
    assert_eq!(context.recipients.bcc()[0].as_str(), "bcc@example.com");
    assert!(context.fallback_subject.is_none());
    assert!(context.in_reply_to.is_none());
    assert!(context.references.is_none());
    Ok(())
}

#[test]
fn standards_provider_reference_is_exact_and_bounded() {
    assert_eq!(parse_reference("imap:42:7"), Ok((42, 7)));
    assert!(parse_reference("gmail:42:7").is_err());
    assert!(parse_reference("imap:0:7").is_err());
    assert!(parse_reference("imap:42:0").is_err());
}
