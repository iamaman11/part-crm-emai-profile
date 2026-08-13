use application_ports::outbound_mail::OutboundMailIntent;
use application_ports::query_mail_provider::MailboxMessageReference;
use cloudflare_adapters::cloud_mail_query::CloudMailboxQueryAdapter;
use cloudflare_adapters::d1_client_mail_eligibility::D1ClientMailboxEligibilityRepository;
use cloudflare_adapters::d1_query::D1QueryRepository;
use profile_platform_primitives::{ActorContext, ClientId};
use use_cases_query::get_client_mailbox_message;
use worker::{Env, Result};

pub(super) async fn is_accessible(
    env: &Env,
    actor: &ActorContext,
    client_id: &ClientId,
    eligibility: &D1ClientMailboxEligibilityRepository,
    intent: &OutboundMailIntent,
) -> Result<bool> {
    let Some(source) = intent.operation().source() else {
        return Ok(true);
    };
    let reference = match MailboxMessageReference::new(
        intent.binding_id().clone(),
        source.provider_reference().as_str().to_owned(),
    ) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let authorization = D1QueryRepository::new(catalog(env)?);
    let provider =
        CloudMailboxQueryAdapter::new(env, catalog(env)?, catalog(env)?, actor, client_id);
    Ok(matches!(
        get_client_mailbox_message(
            actor,
            &authorization,
            eligibility,
            &provider,
            client_id,
            &reference,
        )
        .await,
        Ok(Some(_))
    ))
}

fn catalog(env: &Env) -> Result<worker::d1::D1Database> {
    env.d1(control_plane_contract::D1_CATALOG_BINDING)
}
