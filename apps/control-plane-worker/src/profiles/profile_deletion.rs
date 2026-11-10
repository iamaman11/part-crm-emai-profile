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
use profile_platform_primitives::ProfileId;
use worker::{Date, Env, Request, Response, Result};

const MAX_EXACT_R2_DELETES_PER_REQUEST: usize = 16;

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
        return problem(
            correlation,
            409,
            "profile_delete_conflict",
            "Profile Delete Conflict",
        );
    }
    let r2 = generation_object_retention(env)?;
    let mut processed = 0;
    while processed < MAX_EXACT_R2_DELETES_PER_REQUEST {
        let Some(object) = journal
            .next_pending(actor.tenant_scope(), &profile_id)
            .await?
        else {
            break;
        };
        if r2
            .delete_frozen_profile_exact(actor.tenant_scope(), &object)
            .await
            .is_err()
        {
            // D1 remains DELETING with a frozen inventory. A later exact
            // request can re-HEAD absent after a lost successful DELETE reply.
            return problem(
                correlation,
                503,
                "profile_delete_r2_unavailable",
                "Profile Delete Unavailable",
            );
        }
        if journal
            .record_absent_exact(actor.tenant_scope(), &object, now_ms())
            .await
            .is_err()
        {
            return problem(
                correlation,
                503,
                "profile_delete_receipt_unavailable",
                "Profile Delete Unavailable",
            );
        }
        processed += 1;
    }
    let remaining = journal
        .pending_count(actor.tenant_scope(), &profile_id)
        .await?;
    let complete = if remaining == 0 {
        match journal
            .complete(actor, &profile_id, body.expected_profile_version, now_ms())
            .await
        {
            Ok(complete) => complete,
            Err(_) => {
                // Do not disguise a failed durable D1 completion as a healthy
                // 202 with zero pending objects. DELETING stays replayable.
                return problem(
                    correlation,
                    503,
                    "profile_delete_completion_unavailable",
                    "Profile Delete Unavailable",
                );
            }
        }
    } else {
        false
    };
    let receipt = ProfileDeleteResponse {
        profile_id: profile_id.as_str().to_owned(),
        status: if complete {
            ProfileStatusDto::Deleted
        } else {
            ProfileStatusDto::Deleting
        },
        pending_objects: remaining,
    };
    Response::from_json(&receipt)
        .map(|response| response.with_status(if complete { 200 } else { 202 }))
}
