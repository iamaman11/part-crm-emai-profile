import { requestJson } from './client';
import type {
  ClientMailSearchInput,
  MailMessageBodyDto,
  MailMessageSearchPageDto,
  MailboxMessageReferenceDto,
} from './generated/query-mail';

function segment(value: string): string {
  if (!value || value.includes('/') || value.includes('\\')) {
    throw new TypeError('Opaque identifiers cannot contain path separators');
  }
  return encodeURIComponent(value);
}

export function searchClientMail(
  tenantId: string,
  clientId: string,
  input: ClientMailSearchInput,
  signal?: AbortSignal,
): Promise<MailMessageSearchPageDto | undefined> {
  return requestJson<MailMessageSearchPageDto>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/mail/search`,
    { tenantId, method: 'POST', body: input, signal },
  );
}

export function getClientMailMessage(
  tenantId: string,
  clientId: string,
  reference: MailboxMessageReferenceDto,
  signal?: AbortSignal,
): Promise<MailMessageBodyDto | undefined> {
  return requestJson<MailMessageBodyDto>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/mail/message`,
    { tenantId, method: 'POST', body: reference, signal },
  );
}
