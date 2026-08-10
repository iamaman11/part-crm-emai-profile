use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceJobTarget {
    tenant_id: TenantId,
    device_id: DeviceId,
    profile_id: ProfileId,
    generation_id: GenerationId,
}

impl DeviceJobTarget {
    #[must_use]
    pub const fn new(
        tenant_id: TenantId,
        device_id: DeviceId,
        profile_id: ProfileId,
        generation_id: GenerationId,
    ) -> Self {
        Self {
            tenant_id,
            device_id,
            profile_id,
            generation_id,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }
}
