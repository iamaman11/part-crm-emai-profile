#![forbid(unsafe_code)]

mod assignment;
mod client;
mod contact_point;
mod merge;

pub use assignment::{
    AssignmentError, AssignmentRole, AssignmentStatus, PrimaryAssignmentTransition,
    ProfileClientAssignment, plan_primary_reassignment,
};
pub use client::{ClientError, ClientKind, ClientRecord, ClientStatus};
pub use contact_point::{
    ContactKind, ContactNormalizationVersion, ContactProtectionError, ContactProtectionVersion,
    ContactStatus, ContactValueError, EncryptedContactValue, EncryptionKeyVersion,
    ExactLookupHmacInput, ExactLookupToken, LookupKeyVersion, NormalizedContactValue,
    ProtectedContactPoint, exact_lookup_hmac_input, normalize_contact_value,
};
pub use merge::{ClientMergeError, ClientMergePlan, ClientMergeState, merge_clients};
