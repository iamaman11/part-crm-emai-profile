export type MembershipRole = 'TENANT_OWNER' | 'MEMBER';

export interface ActorSession {
  tenantId: string;
  actorId: string;
  role: MembershipRole;
}

export interface MutationReceipt {
  resultCode: string;
  resourceId: string;
  aggregateVersion: number;
}

export interface ClientProjection {
  clientId: string;
  kind: 'PERSON' | 'ORGANIZATION';
  displayName: string;
  status: string;
  version: number;
}

export interface ProfileProjection {
  profileId: string;
  status: string;
  version: number;
  linkedClientId: string | null;
}

export interface MailboxBindingProjection {
  bindingId: string;
  provider: 'GMAIL_API' | 'IMAP' | 'BROWSER_FALLBACK';
  status: 'ACTIVE' | 'REVOKED';
  version: number;
}

export interface MailboxJobProjection {
  jobId: string;
  status: 'PENDING' | 'RUNNING' | 'SUCCEEDED' | 'RETRY_PENDING' | 'FAILED';
  attempt: number;
  maxAttempts: number;
  nextRunAtMs: number;
  providerStatus: string | null;
  boundedItemCount: number;
  version: number;
}

export interface GenerationProjection {
  generationId: string;
  metadataDigest: string;
  containerDigest: string;
  status: 'REGISTERED' | 'VERIFIED' | 'QUARANTINED';
  version: number;
  verificationReference: string | null;
}

export interface CoordinatorProjection {
  tenant_id: string;
  profile_id: string;
  status: 'idle' | 'active' | 'draining' | 'dirty' | 'uncertain';
  version: number;
  sequence: number;
  next_epoch: number;
  active_session_id: string | null;
  active_device_id: string | null;
  active_epoch: number | null;
  idle_expires_at_ms: number | null;
  hard_expires_at_ms: number | null;
  drain_deadline_ms: number | null;
  pending_launch_intent_id: string | null;
  pending_intent_expires_at_ms: number | null;
}

export interface CoordinatorResponse {
  outcome: string;
  version: number;
  sequence: number;
  replayed: boolean;
  fencing_token: string | null;
  epoch: number | null;
  projection: CoordinatorProjection;
}

export type ProblemCode =
  | 'not_found'
  | 'forbidden'
  | 'invalid_request'
  | 'invalid_state'
  | 'version_conflict'
  | 'lease_conflict'
  | 'replay_rejected'
  | 'dependency_unavailable'
  | 'integrity_failure'
  | 'internal_failure'
  | 'conflict';

export interface ProblemPayload {
  type: string;
  title: string;
  status: number;
  code: ProblemCode;
  correlation_id: string;
}
