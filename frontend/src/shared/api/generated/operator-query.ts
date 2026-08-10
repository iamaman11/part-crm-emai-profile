// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/bin/export_operator_query.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface MailboxListItemDto {
  bindingId: string;
  provider: MailboxProvider;
  status: MailboxStatus;
  version: number;
}

export interface MailboxListPageDto {
  mailboxes: ReadonlyArray<MailboxListItemDto>;
  nextCursor: string | null;
}

export const MailboxProviderValues = ["GMAIL_API", "IMAP", "BROWSER_FALLBACK"] as const;
export type MailboxProvider = (typeof MailboxProviderValues)[number];

export const MailboxStatusValues = ["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"] as const;
export type MailboxStatus = (typeof MailboxStatusValues)[number];

export interface MemberListItemDto {
  actorId: string;
  role: MembershipRole;
  status: MembershipStatus;
}

export interface MemberListPageDto {
  members: ReadonlyArray<MemberListItemDto>;
  nextCursor: string | null;
}

export const MembershipRoleValues = ["TENANT_OWNER", "MEMBER"] as const;
export type MembershipRole = (typeof MembershipRoleValues)[number];

export const MembershipStatusValues = ["ACTIVE", "SUSPENDED", "REVOKED"] as const;
export type MembershipStatus = (typeof MembershipStatusValues)[number];

export interface ProfileListItemDto {
  activeGenerationId: string | null;
  linkedClientId: string | null;
  profileId: string;
  status: ProfileStatus;
  version: number;
}

export interface ProfileListPageDto {
  nextCursor: string | null;
  profiles: ReadonlyArray<ProfileListItemDto>;
}

export const ProfileStatusValues = ["DRAFT", "QUARANTINED", "READY", "IN_USE", "DIRTY_LOCAL", "SYNCING", "SUSPENDED", "DELETING", "DELETED"] as const;
export type ProfileStatus = (typeof ProfileStatusValues)[number];
