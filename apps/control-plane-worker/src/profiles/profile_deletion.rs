//! User-owned profile DELETE: one D1 command, one frozen object inventory,
//! and the existing exact-key R2 effect. No prefix deletes or browser control.
use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::composition::{generation_object_retention, profile_deletion};
use control_plane_contract::profile_generation_api::{
    ProfileDeleteRequest, ProfileDeleteResponse, ProfileStatusDto,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ProfileId, TenantId, TenantScope};
use worker::{Date, Env, Error, Request, Response, Result};

pub async fn delete_profile(
    request: &mut Request,
    env: &Env,
    tenant_id: &str,
    profile_id: &str,
) -> Result<Response> {
    let Some(session) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let actor = session.actor();
    let correlation = actor.correlation_id().as_str();
    // Destructive action needs the tenant owner, not an arbitrary profile
    // viewer or operator. The D1 command independently rechecks membership.
    if membership_role(&session) != MembershipRole::TenantOwner {
        return problem(correlation, 403, "forbidden", "Forbidden");
    }
    let profile_id = match ProfileId::parse(profile_id) {
        Ok(value) => value,
        Err(_) => return neutral_not_found(correlation),
    };
    let body = match request.json::<ProfileDeleteRequest>().await {
        Ok(value) if value.expected_profile_version > 0 => value,
        _ => return problem(correlation, 400, "invalid_request", "Invalid Request"),
    };
    let now_ms = || Date::now().as_millis();
    let journal = profile_deletion(env)?;
    if journal
        .status(actor.tenant_scope(), &profile_id)
        .await?
        .is_none()
    {
        return neutral_not_found(correlation);
    }
    if journal
        .begin_or_replay(actor, &profile_id, body.expected_profile_version, now_ms())
        .await
        .is_err()
    {
        return problem(correlation, 409, "conflict", "Profile Delete Conflict");
    }
    // HTTP only freezes the command and returns its current projection.
    // The existing minute cron is the sole exact R2 DELETE executor: no
    // duplicate R2 side-effect loop and no browser/process ownership.
    let pending = journal
        .pending_count(actor.tenant_scope(), &profile_id)
        .await?;
    // No R2 objects means no R2 effects to schedule. The existing D1
    // completion trigger alone decides whether the pre-signed PUT fence has
    // drained; otherwise the existing minute cron continues this Delete.
    if pending == 0 {
        let _ = journal
            .complete(actor, &profile_id, body.expected_profile_version, now_ms())
            .await;
    }
    let complete = journal
        .status(actor.tenant_scope(), &profile_id)
        .await?
        .as_deref()
        == Some("DELETED");
    Response::from_json(&ProfileDeleteResponse {
        profile_id: profile_id.as_str().to_owned(),
        status: if complete {
            ProfileStatusDto::Deleted
        } else {
            ProfileStatusDto::Deleting
        },
        pending_objects: pending,
    })
    .map(|response| response.with_status(if complete { 200 } else { 202 }))
}

/// Existing Worker cron resumes only already-authorized, immutable D1 Delete
/// commands. It creates no fresh profile Delete, browser owner or GC service.
/// Bound the work per tick; a partial or failed exact R2 effect stays DELETING.
pub(crate) async fn resume_due_frozen_deletes(env: &Env) -> Result<()> {
    const MAX_OBJECTS_PER_PROFILE_PER_TICK: usize = 16;
    let journal = profile_deletion(env)?;
    let frozen = journal.due_frozen_deletes(Date::now().as_millis()).await?;
    let r2 = generation_object_retention(env)?;
    for command in frozen {
        let scope = TenantScope::new(
            TenantId::parse(&command.tenant_id)
                .map_err(|_| Error::RustError("invalid frozen delete tenant".to_owned()))?,
        );
        let profile_id = ProfileId::parse(&command.profile_id)
            .map_err(|_| Error::RustError("invalid frozen delete profile".to_owned()))?;
        let expected_version = u64::try_from(command.expected_profile_version)
            .map_err(|_| Error::RustError("invalid frozen delete version".to_owned()))?;
        for _ in 0..MAX_OBJECTS_PER_PROFILE_PER_TICK {
            let Some(object) = journal.next_pending(&scope, &profile_id).await? else {
                break;
            };
            if r2
                .delete_frozen_profile_exact(&scope, &object)
                .await
                .is_err()
            {
                worker::console_warn!("SCHEDULED_DELETE_R2_REPLAY_PENDING");
                break;
            }
            if journal
                .record_absent_exact(&scope, &object, Date::now().as_millis())
                .await
                .is_err()
            {
                worker::console_warn!("SCHEDULED_DELETE_D1_RECEIPT_PENDING");
                break;
            }
        }
        if journal.pending_count(&scope, &profile_id).await? == 0
            && journal
                .complete_frozen(
                    &scope,
                    &profile_id,
                    &command.command_actor_id,
                    expected_version,
                    Date::now().as_millis(),
                )
                .await
                .is_err()
        {
            worker::console_warn!("SCHEDULED_DELETE_D1_COMPLETION_PENDING");
        }
    }
    Ok(())
}
