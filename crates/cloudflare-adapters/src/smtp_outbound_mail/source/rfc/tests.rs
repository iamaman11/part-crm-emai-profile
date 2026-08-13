use super::{
    SourceHeaders, parse_addresses, reference_chain, reply_all_recipients, reply_recipients,
};

fn source_headers() -> SourceHeaders {
    SourceHeaders {
        subject: Some("Subject".to_owned()),
        from: Some("Alice <alice@example.com>".to_owned()),
        reply_to: None,
        to: Some("sender@example.com, Bob <bob@example.com>".to_owned()),
        cc: Some("Carol <carol@example.com>".to_owned()),
        message_id: Some("<source@example.com>".to_owned()),
        references: Some("<root@example.com>".to_owned()),
    }
}

#[test]
fn reply_fixture_targets_reply_to_or_from_only() -> Result<(), Box<dyn std::error::Error>> {
    let recipients = reply_recipients(&source_headers())
        .map_err(|_| std::io::Error::other("reply recipients"))?;
    assert_eq!(recipients.to().len(), 1);
    assert_eq!(recipients.to()[0].as_str(), "alice@example.com");
    assert!(recipients.cc().is_empty());
    assert!(recipients.bcc().is_empty());
    Ok(())
}

#[test]
fn reply_all_fixture_excludes_sender_and_deduplicates() -> Result<(), Box<dyn std::error::Error>> {
    let recipients = reply_all_recipients(&source_headers(), "sender@example.com")
        .map_err(|_| std::io::Error::other("reply-all recipients"))?;
    let to: Vec<&str> = recipients.to().iter().map(|value| value.as_str()).collect();
    let cc: Vec<&str> = recipients.cc().iter().map(|value| value.as_str()).collect();
    assert_eq!(to, vec!["alice@example.com", "bob@example.com"]);
    assert_eq!(cc, vec!["carol@example.com"]);
    assert!(recipients.bcc().is_empty());
    Ok(())
}

#[test]
fn parser_and_reference_chain_are_bounded_and_stable() -> Result<(), Box<dyn std::error::Error>> {
    let values =
        parse_addresses("\"Doe, Jane\" <jane@example.com>, bob@example.com, jane@example.com")
            .map_err(|_| std::io::Error::other("parse"))?;
    assert_eq!(values.len(), 2);
    let references = reference_chain(Some("<root@example.com>"), "<source@example.com>")
        .map_err(|_| std::io::Error::other("refs"))?;
    assert_eq!(references, "<root@example.com> <source@example.com>");
    Ok(())
}
