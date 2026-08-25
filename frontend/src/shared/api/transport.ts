export interface TransportRequest {
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  path: string;
  tenantId: string;
  body?: unknown;
  idempotencyKey?: string;
  signal?: AbortSignal;
}

export interface TransportResponse {
  status: number;
  headers: Headers;
  bytes: Uint8Array;
}

const MAX_RESPONSE_BYTES = 256 * 1024;

function ensureApiPath(path: string): void {
  if (!path.startsWith('/api/v1/') || path.includes('://')) {
    throw new TypeError('API requests must use a same-origin /api/v1/ path');
  }
}

export async function executeTransport(request: TransportRequest): Promise<TransportResponse> {
  ensureApiPath(request.path);

  const headers = new Headers({
    Accept: 'application/json, application/problem+json',
    'X-Tenant-Id': request.tenantId,
    'X-Correlation-Id': `corr_${crypto.randomUUID().replaceAll('-', '')}`,
  });

  if (request.body !== undefined) {
    headers.set('Content-Type', 'application/json');
  }

  if (request.idempotencyKey !== undefined) {
    headers.set('Idempotency-Key', request.idempotencyKey);
  }

  const response = await fetch(request.path, {
    method: request.method,
    headers,
    credentials: 'same-origin',
    redirect: 'error',
    body: request.body === undefined ? undefined : JSON.stringify(request.body),
    signal: request.signal,
  });

  const contentLength = response.headers.get('content-length');
  if (contentLength !== null && Number(contentLength) > MAX_RESPONSE_BYTES) {
    throw new TypeError('API response exceeded the allowed size');
  }

  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength > MAX_RESPONSE_BYTES) {
    throw new TypeError('API response exceeded the allowed size');
  }

  return {
    status: response.status,
    headers: response.headers,
    bytes,
  };
}
