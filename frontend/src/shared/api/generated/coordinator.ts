// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/coordinator_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export type CoordinatorCommandDto =
  | { device_id: string; expires_in_ms: number; launch_intent_id: string; type: "issue_launch_intent" }
  | { device_id: string; launch_intent_id: string; session_id: string; type: "claim" }
  | { epoch: number; fencing_token: string; session_id: string; type: "heartbeat" }
  | { disposition: CoordinatorReleaseDispositionDto; epoch: number; fencing_token: string; session_id: string; type: "release" }
  | { type: "begin_drain" }
  | { type: "mark_recovered" };

export interface CoordinatorCommandRequestDto {
  command: CoordinatorCommandDto;
  expected_version: number;
  idempotency_key: string;
  sequence: number;
}

export const CoordinatorOutcomeDtoValues = ["snapshot", "launch_intent_issued", "lease_claimed", "heartbeat_accepted", "released", "drain_started", "timed_out", "launch_intent_expired", "recovered", "no_change"] as const;
export type CoordinatorOutcomeDto = (typeof CoordinatorOutcomeDtoValues)[number];

export interface CoordinatorProjectionDto {
  active_device_id: string | null;
  active_epoch: number | null;
  active_session_id: string | null;
  drain_deadline_ms: number | null;
  hard_expires_at_ms: number | null;
  idle_expires_at_ms: number | null;
  next_epoch: number;
  pending_intent_expires_at_ms: number | null;
  pending_launch_intent_id: string | null;
  profile_id: string;
  sequence: number;
  status: CoordinatorStatusDto;
  tenant_id: string;
  version: number;
}

export const CoordinatorReleaseDispositionDtoValues = ["clean", "dirty", "uncertain"] as const;
export type CoordinatorReleaseDispositionDto = (typeof CoordinatorReleaseDispositionDtoValues)[number];

export interface CoordinatorResponseDto {
  epoch: number | null;
  fencing_token: string | null;
  outcome: CoordinatorOutcomeDto;
  projection: CoordinatorProjectionDto;
  replayed: boolean;
  sequence: number;
  version: number;
}

export const CoordinatorStatusDtoValues = ["idle", "active", "draining", "dirty", "uncertain"] as const;
export type CoordinatorStatusDto = (typeof CoordinatorStatusDtoValues)[number];
