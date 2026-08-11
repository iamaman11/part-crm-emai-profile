// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/bin/export_operator_query.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface MailboxListItemDto {
  bindingId: string;
  provider: OperatorMailboxProvider;
  status: OperatorMailboxStatus;
  version: number;
}

export interface MailboxListPageDto {
  mailboxes: ReadonlyArray<MailboxListItemDto>;
  nextCursor: string | null;
}

export interface MemberListItemDto {
  actorId: string;
  role: OperatorMembershipRole;
  status: OperatorMembershipStatus;
}

export interface MemberListPageDto {
  members: ReadonlyArray<MemberListItemDto>;
  nextCursor: string | null;
}

export const OperatorMailboxProviderValues = ["GMAIL_API", "IMAP", "BROWSER_FALLBACK"] as const;
export type OperatorMailboxProvider = (typeof OperatorMailboxProviderValues)[number];

export const OperatorMailboxStatusValues = ["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"] as const;
export type OperatorMailboxStatus = (typeof OperatorMailboxStatusValues)[number];

export const OperatorMembershipRoleValues = ["TENANT_OWNER", "MEMBER"] as const;
export type OperatorMembershipRole = (typeof OperatorMembershipRoleValues)[number];

export const OperatorMembershipStatusValues = ["ACTIVE", "SUSPENDED", "REVOKED"] as const;
export type OperatorMembershipStatus = (typeof OperatorMembershipStatusValues)[number];

export const OperatorProfileStatusValues = ["DRAFT", "QUARANTINED", "READY", "IN_USE", "DIRTY_LOCAL", "SYNCING", "SUSPENDED", "DELETING", "DELETED"] as const;
export type OperatorProfileStatus = (typeof OperatorProfileStatusValues)[number];

export interface ProfileListItemDto {
  activeGenerationId: string | null;
  linkedClientId: string | null;
  profileId: string;
  status: OperatorProfileStatus;
  version: number;
}

export interface ProfileListPageDto {
  nextCursor: string | null;
  profiles: ReadonlyArray<ProfileListItemDto>;
}
