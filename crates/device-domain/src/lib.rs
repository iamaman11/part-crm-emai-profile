#![forbid(unsafe_code)]

pub mod claim;
pub mod id;
pub mod job;
pub mod target;

pub use claim::{DeviceClaim, DeviceClaimError};
pub use id::{DeviceClaimId, DeviceJobId};
pub use job::{DeviceJob, DeviceJobError, DeviceJobStatus};
pub use target::DeviceJobTarget;
