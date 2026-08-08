// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/public_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface ActorSession {
  actorId: string;
  role: MembershipRole;
  tenantId: string;
}

export interface ClientCreateRequest {
  clientId: string;
  displayName: string;
  kind: ClientKind;
  requestDigest: string;
}

export interface ClientGrantRequest {
  expectedClientVersion: number;
  reason: string;
  requestDigest: string;
  role: ClientGrantRole;
}

export const ClientGrantRoleValues = ["CLIENT_VIEWER", "CLIENT_EDITOR"] as const;
export type ClientGrantRole = (typeof ClientGrantRoleValues)[number];

export const ClientKindValues = ["PERSON", "ORGANIZATION"] as const;
export type ClientKind = (typeof ClientKindValues)[number];

export interface ClientProjection {
  clientId: string;
  displayName: string;
  kind: ClientKind;
  status: ClientStatus;
  version: number;
}

export const ClientStatusValues = ["ACTIVE", "ARCHIVED", "MERGED"] as const;
export type ClientStatus = (typeof ClientStatusValues)[number];

export const MembershipRoleValues = ["TENANT_OWNER", "MEMBER"] as const;
export type MembershipRole = (typeof MembershipRoleValues)[number];

export interface MutationReceipt {
  aggregateVersion: number;
  resourceId: string;
  resultCode: string;
}

export const ProblemCodeValues = ["not_found", "forbidden", "invalid_request", "invalid_state", "version_conflict", "lease_conflict", "replay_rejected", "dependency_unavailable", "integrity_failure", "internal_failure", "conflict"] as const;
export type ProblemCode = (typeof ProblemCodeValues)[number];

export interface ProblemPayload {
  code: ProblemCode;
  correlation_id: string;
  status: number;
  title: string;
  type: string;
}
