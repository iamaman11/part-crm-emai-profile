#![forbid(unsafe_code)]

pub mod authorization;
pub mod claim;
pub mod id;
pub mod job;
pub mod pairing;
pub mod target;

pub use authorization::{
    DeviceApplicationSession, DeviceApplicationSessionError, DeviceProofMessageError,
    device_proof_message_v1,
};
pub use claim::{DeviceClaim, DeviceClaimError, DeviceClaimSnapshot};
pub use id::{DeviceClaimId, DeviceJobId};
pub use job::{DeviceJob, DeviceJobError, DeviceJobSnapshot, DeviceJobStatus};
pub use pairing::{
    DEVICE_PROOF_NONCE_BYTES, DeviceAuthorizationError, DevicePairingError,
    DevicePairingTransaction, DeviceProofChallenge, DeviceProofError, DevicePublicKey,
    DevicePublicKeyAlgorithm, RegisteredDeviceCredential,
};
pub use target::DeviceJobTarget;
