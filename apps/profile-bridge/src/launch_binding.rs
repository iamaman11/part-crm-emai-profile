use crate::operator_flow::OperatorEnrollment;
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, LaunchIntentId, ProfileId,
    TenantId, TenantScope,
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchBindingError {
    InvalidProjection,
    DeviceMismatch,
}

impl fmt::Display for LaunchBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProjection => "Bridge launch redemption projection is invalid",
            Self::DeviceMismatch => "Bridge launch redemption is bound to another device",
        })
    }
}

impl std::error::Error for LaunchBindingError {}

/// Convert an already decoded canonical Bridge redemption projection into the operator's trusted
/// primitives. This function owns no JSON/wire schema: the Control Plane contract remains the sole
/// wire owner. The locally resolved device identity is rechecked before any operator enrollment is
/// created so a network response cannot redirect the shipping Bridge to another device binding.
#[allow(clippy::too_many_arguments)]
pub fn bind_operator_enrollment(
    tenant_id: &str,
    actor_id: &str,
    profile_id: &str,
    generation_id: &str,
    redeemed_device_id: &str,
    launch_intent_id: &str,
    local_device_id: &DeviceId,
    correlation_id: CorrelationId,
) -> Result<OperatorEnrollment, LaunchBindingError> {
    let tenant_id =
        TenantId::parse(tenant_id.to_owned()).map_err(|_| LaunchBindingError::InvalidProjection)?;
    let actor_id =
        ActorId::parse(actor_id.to_owned()).map_err(|_| LaunchBindingError::InvalidProjection)?;
    let profile_id = ProfileId::parse(profile_id.to_owned())
        .map_err(|_| LaunchBindingError::InvalidProjection)?;
    let generation_id = GenerationId::parse(generation_id.to_owned())
        .map_err(|_| LaunchBindingError::InvalidProjection)?;
    let redeemed_device_id = DeviceId::parse(redeemed_device_id.to_owned())
        .map_err(|_| LaunchBindingError::InvalidProjection)?;
    let launch_intent_id = LaunchIntentId::parse(launch_intent_id.to_owned())
        .map_err(|_| LaunchBindingError::InvalidProjection)?;

    if &redeemed_device_id != local_device_id {
        return Err(LaunchBindingError::DeviceMismatch);
    }

    Ok(OperatorEnrollment::new(
        ActorContext::new(TenantScope::new(tenant_id), actor_id, correlation_id),
        profile_id,
        generation_id,
        launch_intent_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::{LaunchBindingError, bind_operator_enrollment};
    use profile_platform_primitives::{CorrelationId, DeviceId};

    #[test]
    fn exact_redeemed_device_produces_typed_operator_enrollment()
    -> Result<(), Box<dyn std::error::Error>> {
        let device_id = DeviceId::parse("device_01JBRIDGEBIND")?;
        let enrollment = bind_operator_enrollment(
            "tenant_01JBRIDGEBIND",
            "actor_01JBRIDGEBIND",
            "profile_01JBRIDGEBIND",
            "generation_01JBRIDGEBIND",
            device_id.as_str(),
            "launch_01JBRIDGEBIND",
            &device_id,
            CorrelationId::parse("corr_01JBRIDGEBIND")?,
        )?;
        assert_eq!(
            enrollment.actor().tenant_scope().tenant_id().as_str(),
            "tenant_01JBRIDGEBIND"
        );
        assert_eq!(enrollment.profile_id().as_str(), "profile_01JBRIDGEBIND");
        assert_eq!(
            enrollment.generation_id().as_str(),
            "generation_01JBRIDGEBIND"
        );
        assert_eq!(
            enrollment.launch_intent_id().as_str(),
            "launch_01JBRIDGEBIND"
        );
        Ok(())
    }

    #[test]
    fn response_for_another_device_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let local = DeviceId::parse("device_01JBRIDGEBIND")?;
        let result = bind_operator_enrollment(
            "tenant_01JBRIDGEBIND",
            "actor_01JBRIDGEBIND",
            "profile_01JBRIDGEBIND",
            "generation_01JBRIDGEBIND",
            "device_02JBRIDGEBIND",
            "launch_01JBRIDGEBIND",
            &local,
            CorrelationId::parse("corr_01JBRIDGEBIND")?,
        );
        assert_eq!(result, Err(LaunchBindingError::DeviceMismatch));
        Ok(())
    }

    #[test]
    fn malformed_network_identity_never_crosses_into_operator()
    -> Result<(), Box<dyn std::error::Error>> {
        let local = DeviceId::parse("device_01JBRIDGEBIND")?;
        let result = bind_operator_enrollment(
            "not a tenant",
            "actor_01JBRIDGEBIND",
            "profile_01JBRIDGEBIND",
            "generation_01JBRIDGEBIND",
            local.as_str(),
            "launch_01JBRIDGEBIND",
            &local,
            CorrelationId::parse("corr_01JBRIDGEBIND")?,
        );
        assert_eq!(result, Err(LaunchBindingError::InvalidProjection));
        Ok(())
    }
}
