// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/client_mail_send_api.rs
// Generated through: scripts/generate-frontend-contracts.py
// Regenerate with: python scripts/generate-c7-contracts.py

export const ClientMailSendOperationDtoValues = ["NEW", "REPLY", "REPLY_ALL", "FORWARD"] as const;
export type ClientMailSendOperationDto = (typeof ClientMailSendOperationDtoValues)[number];

export interface ClientMailSendReceiptDto {
  attemptCount: number;
  intentId: string;
  replayed: boolean;
  state: ClientMailSendStateDto;
}

export interface ClientMailSendRequestDto {
  bcc: ReadonlyArray<string>;
  cc: ReadonlyArray<string>;
  htmlBody: string | null;
  mailboxBindingId: string;
  operation: ClientMailSendOperationDto;
  sourceProviderReference: string | null;
  subject: string | null;
  textBody: string | null;
  to: ReadonlyArray<string>;
}

export const ClientMailSendStateDtoValues = ["PENDING", "DISPATCHING", "RETRYABLE", "SENT", "AMBIGUOUS", "REJECTED"] as const;
export type ClientMailSendStateDto = (typeof ClientMailSendStateDtoValues)[number];
