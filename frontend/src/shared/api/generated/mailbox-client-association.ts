// GENERATED FILE — DO NOT EDIT.
// Canonical Rust source: crates/control-plane-contract/src/mailbox_client_association_api.rs
// Generated through: scripts/generate-mailbox-client-association-contract.py
// Regenerate with: python scripts/generate-mailbox-client-association-contract.py

export interface ChangeMailboxClientAssociationRequestDto {
  clientId: string | null;
  expectedRelationshipVersion: number;
  requestDigest: string;
}

export interface MailboxClientAssociationMutationReceiptDto {
  bindingId: string;
  clientId: string | null;
  relationshipVersion: number;
  replayed: boolean;
  resultCode: "bound" | "rebound" | "unbound";
}

export interface MailboxClientAssociationProjectionDto {
  bindingId: string;
  canManage: boolean;
  clientId: string | null;
  mailboxExecutable: boolean;
  relationshipVersion: number;
}
