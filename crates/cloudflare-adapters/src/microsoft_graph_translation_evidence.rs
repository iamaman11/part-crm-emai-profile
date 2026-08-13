#![cfg(test)]

mod exact_adapter_source {
    include!("microsoft_graph_mail_query.rs");

    use application_ports::query::{QueryPageRequest, QueryPageSize};
    use application_ports::query_mail_provider::{
        MailSearchTerm, SearchClientMailboxMessagesRequest,
    };
    use mailbox_domain::{MailboxBinding, MailboxProvider};
    use profile_platform_primitives::{MailboxBindingId, SecretHandle, TenantId};

    fn graph_binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JC3GGRAPH")?,
            MailboxBindingId::parse("mailbox_01JC3GGRAPH")?,
            MailboxProvider::MicrosoftGraph,
            SecretHandle::parse("secret_01JC3GGRAPH")?,
        ))
    }

    #[test]
    fn deterministic_list_fixture_translates_to_provider_neutral_summary_and_preserves_next_link()
    -> Result<(), Box<dyn std::error::Error>> {
        const NEXT_LINK: &str =
            "https://graph.microsoft.com/v1.0/me/messages?$skiptoken=opaque-state%2B1";
        let fixture = format!(
            r#"{{
                "@odata.nextLink":"{NEXT_LINK}",
                "value":[{{
                    "id":"AAMk-fixture-list",
                    "subject":"Quarterly planning",
                    "from":{{"emailAddress":{{"name":"Alice Example","address":"alice@example.com"}}}},
                    "receivedDateTime":"2026-08-13T08:30:00Z"
                }}]
            }}"#
        );
        let page: GraphMessageListResponse = serde_json::from_str(&fixture)?;
        assert_eq!(page.next_link.as_deref(), Some(NEXT_LINK));
        assert_eq!(page.value.len(), 1);

        let summary = summary_from_graph(&graph_binding()?, &page.value[0])?;
        assert_eq!(summary.reference().binding_id().as_str(), "mailbox_01JC3GGRAPH");
        assert_eq!(
            summary.reference().provider_reference(),
            "graph:AAMk-fixture-list"
        );
        assert_eq!(summary.subject(), Some("Quarterly planning"));
        assert_eq!(summary.sender(), Some("alice@example.com"));
        assert_eq!(summary.received_at().value(), 1_786_609_800_000);
        Ok(())
    }

    #[test]
    fn deterministic_get_fixture_translates_text_body_without_graph_dto_leakage()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = r#"{
            "id":"AAMk-fixture-get",
            "subject":"Follow up",
            "from":{"emailAddress":{"name":"Bob Example","address":"bob@example.com"}},
            "receivedDateTime":"2026-08-13T09:15:00Z",
            "body":{"contentType":"text","content":"Provider body fixture"}
        }"#;
        let mut message: GraphMessage = serde_json::from_str(fixture)?;
        let summary = summary_from_graph(&graph_binding()?, &message)?;
        let (text_body, html_body) = graph_body(message.body.take())?;
        let translated = MailMessageBody::new(summary, text_body, html_body)?;

        assert_eq!(
            translated.summary().reference().provider_reference(),
            "graph:AAMk-fixture-get"
        );
        assert_eq!(translated.summary().subject(), Some("Follow up"));
        assert_eq!(translated.summary().sender(), Some("bob@example.com"));
        assert_eq!(translated.text_body(), Some("Provider body fixture"));
        assert_eq!(translated.html_body(), None);
        Ok(())
    }

    #[test]
    fn deterministic_search_and_get_endpoints_are_bounded_and_provider_specific()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = SearchClientMailboxMessagesRequest::new(
            Some(MailSearchTerm::parse("subject:quarterly plan")?),
            QueryPageRequest::new(QueryPageSize::new(100)?, None),
        );
        let endpoint = initial_search_endpoint(&request, MAX_GRAPH_QUERY_PAGE_SIZE);
        assert_eq!(
            endpoint,
            "https://graph.microsoft.com/v1.0/me/messages?$select=id,subject,from,receivedDateTime&$top=25&$search=%22subject%3Aquarterly%20plan%22"
        );
        assert_eq!(
            message_endpoint("AAMk/id+fixture"),
            "https://graph.microsoft.com/v1.0/me/messages/AAMk%2Fid%2Bfixture?$select=id,subject,from,receivedDateTime,body"
        );
        Ok(())
    }
}
