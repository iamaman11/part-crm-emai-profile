// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/profile_generation_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface GenerationProjectionDto {
  containerDigest: string;
  generationId: string;
  metadataDigest: string;
  status: GenerationStatusDto;
  verificationReference: string | null;
  version: number;
}

export const GenerationStatusDtoValues = ["REGISTERED", "VERIFIED", "QUARANTINED"] as const;
export type GenerationStatusDto = (typeof GenerationStatusDtoValues)[number];

export interface ProfileAssignmentRequest {
  assignmentId: string;
  clientId: string;
  expectedProfileVersion: number;
  reason: string;
  requestDigest: string;
}

export interface ProfileCreateRequestDto {
  profileId: string;
  requestDigest: string;
}

export interface ProfileGenerationVersionRequest {
  expectedProfileVersion: number;
  requestDigest: string;
}

export interface ProfileGrantRequestDto {
  expectedProfileVersion: number;
  reason: string;
  requestDigest: string;
  role: ProfileGrantRoleDto;
}

export const ProfileGrantRoleDtoValues = ["PROFILE_VIEWER", "PROFILE_OPERATOR"] as const;
export type ProfileGrantRoleDto = (typeof ProfileGrantRoleDtoValues)[number];

export interface ProfileProjectionDto {
  linkedClientId: string | null;
  profileId: string;
  status: ProfileStatusDto;
  version: number;
}

export const ProfileStatusDtoValues = ["DRAFT", "QUARANTINED", "READY", "IN_USE", "DIRTY_LOCAL", "SYNCING", "SUSPENDED", "DELETING", "DELETED"] as const;
export type ProfileStatusDto = (typeof ProfileStatusDtoValues)[number];

export interface QuarantineGenerationRequest {
  expectedGenerationVersion: number;
  requestDigest: string;
}

export interface RegisterGenerationRequest {
  containerDigest: string;
  generationId: string;
  metadataDigest: string;
  objectKey: string;
  requestDigest: string;
}

export interface VerifyGenerationRequest {
  expectedGenerationVersion: number;
  requestDigest: string;
  verificationReference: string;
}
