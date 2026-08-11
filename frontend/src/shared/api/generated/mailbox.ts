// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/mailbox_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface BindBrowserMailboxExecutionRequestDto {
  profileId: string;
  requestDigest: string;
}

export interface BrowserExecutionBindingReceiptDto {
  bindingId: string;
  profileId: string;
  replayed: boolean;
}

export interface CreateMailboxBindingRequestDto {
  bindingId: string;
  provider: MailboxProviderDto;
  requestDigest: string;
  secretHandle: string;
}

export interface CreateMailboxJobRequestDto {
  cursor: string | null;
  delayMs: number;
  jobId: string;
  maxAttempts: number;
  requestDigest: string;
}

export interface MailboxBindingProjectionDto {
  bindingId: string;
  provider: MailboxProviderDto;
  status: MailboxBindingStatusDto;
  version: number;
}

export const MailboxBindingStatusDtoValues = ["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"] as const;
export type MailboxBindingStatusDto = (typeof MailboxBindingStatusDtoValues)[number];

export interface MailboxJobProjectionDto {
  attempt: number;
  boundedItemCount: number;
  jobId: string;
  maxAttempts: number;
  nextRunAtMs: number;
  providerStatus: string | null;
  status: MailboxJobStatusDto;
  version: number;
}

export const MailboxJobStatusDtoValues = ["SCHEDULED", "QUEUED", "RUNNING", "RETRY_PENDING", "AUTH_REQUIRED", "SUSPENDED", "SUCCEEDED", "FAILED"] as const;
export type MailboxJobStatusDto = (typeof MailboxJobStatusDtoValues)[number];

export const MailboxProviderDtoValues = ["GMAIL_API", "IMAP", "BROWSER_FALLBACK"] as const;
export type MailboxProviderDto = (typeof MailboxProviderDtoValues)[number];

export interface RevokeMailboxBindingRequestDto {
  expectedBindingVersion: number;
  requestDigest: string;
}

export interface RunMailboxJobRequestDto {
  expectedJobVersion: number;
  requestDigest: string;
}
