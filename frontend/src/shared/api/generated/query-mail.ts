// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/bin/export_query_mail.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-frontend-contracts.py

export interface ClientMailSearchInput {
  cursor: string | null;
  limit: number;
  mailboxBindingId: string;
  term: string | null;
}

export interface MailMessageBodyDto {
  htmlBody: string | null;
  summary: MailMessageSummaryDto;
  textBody: string | null;
}

export interface MailMessageSearchPageDto {
  messages: ReadonlyArray<MailMessageSummaryDto>;
  nextCursor: string | null;
}

export interface MailMessageSummaryDto {
  receivedAtMs: number;
  reference: MailboxMessageReferenceDto;
  sender: string | null;
  subject: string | null;
}

export interface MailboxMessageReferenceDto {
  mailboxBindingId: string;
  providerReference: string;
}
