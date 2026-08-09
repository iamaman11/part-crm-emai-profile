use core::fmt;
use profile_platform_primitives::{OpaqueId, ParseOpaqueIdError};

macro_rules! define_device_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(OpaqueId);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ParseOpaqueIdError> {
                OpaqueId::parse(value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

define_device_id!(DeviceJobId);
define_device_id!(DeviceClaimId);

#[cfg(test)]
mod tests {
    use super::{DeviceClaimId, DeviceJobId};

    #[test]
    fn device_job_and_claim_ids_are_opaque_and_path_safe() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            DeviceJobId::parse("devjob_01JDEVICE")?.as_str(),
            "devjob_01JDEVICE"
        );
        assert_eq!(
            DeviceClaimId::parse("devclaim_01JDEVICE")?.as_str(),
            "devclaim_01JDEVICE"
        );
        assert!(DeviceJobId::parse("../../device-job").is_err());
        assert!(DeviceClaimId::parse("claim@example.com").is_err());
        Ok(())
    }
}
