#![forbid(unsafe_code)]

pub mod generation_commit;
pub mod jobs;
pub mod queries;

pub use generation_commit::{
    DeviceGenerationCommitOperationError, DeviceGenerationCommitServices,
    execute_commit_dirty_generation,
};
pub use jobs::{
    ApplyDeviceJobOutcomeCommand, CancelDeviceJobCommand, ClaimDeviceJobCommand,
    DeviceJobOperationError, DeviceJobOutcome, ExpireDeviceClaimCommand, HeartbeatDeviceJobCommand,
    IssueDeviceJobCommand, ResumeDeviceJobCommand, execute_apply_device_job_outcome,
    execute_cancel_device_job, execute_claim_device_job, execute_expire_device_claim,
    execute_heartbeat_device_job, execute_issue_device_job, execute_resume_device_job,
};
pub use queries::{
    DeviceJobQueryError, ListClaimableDeviceJobsRequest, MAX_CLAIMABLE_DEVICE_JOBS,
    execute_list_claimable_device_jobs,
};
