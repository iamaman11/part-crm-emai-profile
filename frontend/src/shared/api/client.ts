import type { ProblemCode, ProblemPayload } from './types';

const MAX_RESPONSE_BYTES = 256 * 1024;
const JSON_MEDIA_TYPES = ['application/json', 'application/problem+json'];
const PROBLEM_CODES = new Set<ProblemCode>([
  'not_found',
  'forbidden',
  'invalid_request',
  'invalid_state',
  'version_conflict',
  'lease_conflict',
  'replay_rejected',
  'dependency_unavailable',
  'integrity_failure',
  'internal_failure',
  'conflict',
]);

export class ApiProblem extends Error {
  readonly status: number;
  readonly code: ProblemCode;
  readonly correlationId: string;
  readonly problemType: string;

  constructor(payload: ProblemPayload) {
    super(payload.title);
    this.name = 'ApiProblem';
    this.status = payload.status;
    this.code = payload.code;
    this.correlationId = payload.correlation_id;
    this.problemType = payload.type;
  }
}

export interface ApiRequestOptions {
  tenantId: string;
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  body?: unknown;
  idempotencyKey?: string;
  signal?: AbortSignal;
}

function opaqueRequestId(prefix: 'corr' | 'idem'): string {
  return `${prefix}_${crypto.randomUUID().replaceAll('-', '')}`;
}

function ensureApiPath(path: string): void {
  if (!path.startsWith('/api/v1/') || path.includes('://')) {
    throw new TypeError('API requests must use a same-origin /api/v1/ path');
  }
}

function isJsonContentType(contentType: string): boolean {
  const normalized = contentType.split(';', 1)[0]?.trim().toLowerCase() ?? '';
  return JSON_MEDIA_TYPES.includes(normalized);
}

async function boundedText(response: Response): Promise<string> {
  const advertised = response.headers.get('content-length');
  if (advertised !== null) {
    const bytes = Number(advertised);
    if (!Number.isFinite(bytes) || bytes < 0 || bytes > MAX_RESPONSE_BYTES) {
      throw new TypeError('API response exceeded the allowed size');
    }
  }
  const text = await response.text();
  if (new TextEncoder().encode(text).byteLength > MAX_RESPONSE_BYTES) {
    throw new TypeError('API response exceeded the allowed size');
  }
  return text;
}

function isProblemPayload(value: unknown): value is ProblemPayload {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.type === 'string' &&
    typeof candidate.title === 'string' &&
    typeof candidate.status === 'number' &&
    typeof candidate.code === 'string' &&
    PROBLEM_CODES.has(candidate.code as ProblemCode) &&
    typeof candidate.correlation_id === 'string'
  );
}

export async function sha256Hex(value: unknown): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function newIdempotencyKey(): string {
  return opaqueRequestId('idem');
}

export async function requestJson<T>(
  path: string,
  options: ApiRequestOptions,
): Promise<T | undefined> {
  ensureApiPath(path);
  const method = options.method ?? 'GET';
  const headers = new Headers({
    Accept: 'application/json, application/problem+json',
    'X-Tenant-Id': options.tenantId,
    'X-Correlation-Id': opaqueRequestId('corr'),
  });
  if (options.body !== undefined) headers.set('Content-Type', 'application/json');
  if (options.idempotencyKey !== undefined) headers.set('Idempotency-Key', options.idempotencyKey);

  const init: RequestInit = {
    method,
    headers,
    credentials: 'same-origin',
    redirect: 'error',
  };
  if (options.body !== undefined) init.body = JSON.stringify(options.body);
  if (options.signal !== undefined) init.signal = options.signal;

  const response = await fetch(path, init);
  if (response.status === 204) return undefined;

  const contentType = response.headers.get('content-type') ?? '';
  if (!isJsonContentType(contentType)) {
    if (!response.ok) {
      throw new ApiProblem({
        type: 'urn:part-crm:problem:internal-failure',
        title: 'Request failed',
        status: response.status,
        code: 'internal_failure',
        correlation_id: response.headers.get('x-correlation-id') ?? 'corr_unknown',
      });
    }
    throw new TypeError('API response did not use a supported JSON media type');
  }

  const text = await boundedText(response);
  let payload: unknown;
  try {
    payload = JSON.parse(text);
  } catch {
    throw new TypeError('API response contained invalid JSON');
  }

  if (!response.ok) {
    if (isProblemPayload(payload)) throw new ApiProblem(payload);
    throw new ApiProblem({
      type: 'urn:part-crm:problem:internal-failure',
      title: 'Request failed',
      status: response.status,
      code: 'internal_failure',
      correlation_id: response.headers.get('x-correlation-id') ?? 'corr_unknown',
    });
  }
  return payload as T;
}
