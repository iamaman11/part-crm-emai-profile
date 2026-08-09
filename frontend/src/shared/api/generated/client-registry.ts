// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/client_registry_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

import type { ClientProjection } from './control-plane';

export interface ClientActivityProjection {
  action: string;
  auditEventId: string;
  occurredAtMs: number;
  resourceId: string;
  resourceType: string;
  resultCode: string;
}

export interface ClientArchiveRequest {
  expectedClientVersion: number;
  requestDigest: string;
}

export interface ClientAssignmentProjection {
  assignedAtMs: number;
  assignmentId: string;
  closedAtMs: number | null;
  profileId: string;
  reason: string;
  status: ClientAssignmentStatus;
}

export const ClientAssignmentStatusValues = ["ACTIVE", "CLOSED"] as const;
export type ClientAssignmentStatus = (typeof ClientAssignmentStatusValues)[number];

export interface ClientContactArchiveRequest {
  expectedClientVersion: number;
  kind: ClientContactKind;
  requestDigest: string;
}

export const ClientContactKindValues = ["EMAIL", "PHONE", "URL"] as const;
export type ClientContactKind = (typeof ClientContactKindValues)[number];

export interface ClientContactProjection {
  contactPointId: string;
  kind: ClientContactKind;
  status: ClientContactStatus;
}

export const ClientContactStatusValues = ["ACTIVE", "ARCHIVED"] as const;
export type ClientContactStatus = (typeof ClientContactStatusValues)[number];

export interface ClientContactUpsertRequest {
  expectedClientVersion: number;
  kind: ClientContactKind;
  requestDigest: string;
  value: string;
}

export interface ClientHistoryProjection {
  activity: ReadonlyArray<ClientActivityProjection>;
  assignments: ReadonlyArray<ClientAssignmentProjection>;
  contacts: ReadonlyArray<ClientContactProjection>;
}

export interface ClientListProjection {
  clients: ReadonlyArray<ClientProjection>;
}

export interface ClientMergeRequest {
  expectedSourceVersion: number;
  expectedTargetVersion: number;
  reason: string;
  requestDigest: string;
  targetClientId: string;
}

export interface ClientUpdateRequest {
  displayName: string;
  expectedClientVersion: number;
  requestDigest: string;
}
