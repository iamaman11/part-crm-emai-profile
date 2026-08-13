use application_ports::CommandExecutionEvidence;
use application_ports::outbound_mail::{
    MailAddress, MailBody, MailRecipients, OutboundMailIntent, OutboundMailOperation,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AuditEventId, ClientId, CorrelationId, IdempotencyKey, MailboxBindingId,
    OutboxEventId, TenantId, TenantScope, UnixMillis,
};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(crate) fn actor() -> TestResult<ActorContext> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_c4_outbound")?),
        ActorId::parse("actor_c4_outbound")?,
        CorrelationId::parse("correlation_c4_outbound")?,
    ))
}

pub(crate) fn intent() -> TestResult<OutboundMailIntent> {
    let recipients = MailRecipients::new(
        vec![MailAddress::parse("client@example.com")?],
        Vec::new(),
        Vec::new(),
    )?;
    Ok(OutboundMailIntent::new(
        ClientId::parse("client_c4_outbound")?,
        MailboxBindingId::parse("binding_c4_outbound")?,
        OutboundMailOperation::New { recipients },
        None,
        MailBody::new(Some("message".to_owned()), None)?,
    ))
}

pub(crate) fn evidence(
    key: &str,
    digest: &str,
    suffix: &str,
) -> TestResult<CommandExecutionEvidence> {
    Ok(CommandExecutionEvidence::new(
        IdempotencyKey::parse(key)?,
        digest,
        AuditEventId::parse(format!("audit-{suffix}-c4"))?,
        OutboxEventId::parse(format!("outbox-{suffix}-c4"))?,
        UnixMillis::new(1_000),
        UnixMillis::new(86_401_000),
    ))
}
