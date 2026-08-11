export type {
  ActorSession,
  ClientProjection,
  MembershipRole,
  MutationReceipt,
  ProblemCode,
  ProblemPayload,
} from './generated/control-plane';

export type {
  GenerationProjectionDto as GenerationProjection,
  GenerationStatusDto,
  ProfileAssignmentRequest,
  ProfileCreateRequestDto as ProfileCreateRequest,
  ProfileGenerationVersionRequest,
  ProfileGrantRequestDto as ProfileGrantRequest,
  ProfileGrantRoleDto,
  ProfileProjectionDto as ProfileProjection,
  ProfileStatusDto,
  QuarantineGenerationRequest,
  RegisterGenerationRequest,
  VerifyGenerationRequest,
} from './generated/profile-generation';

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
