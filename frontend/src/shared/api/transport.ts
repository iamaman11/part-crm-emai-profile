export type TransportMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export interface TransportRequest {
  method: TransportMethod;
  path: string;
  headers: Headers;
  body?: Uint8Array;
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

  const init: RequestInit = {
    method: request.method,
    headers: request.headers,
    credentials: 'same-origin',
    redirect: 'error',
  };
  if (request.body !== undefined) init.body = request.body;
  if (request.signal !== undefined) init.signal = request.signal;

  const response = await fetch(request.path, init);
  const contentLength = response.headers.get('content-length');
  if (contentLength !== null) {
    const advertised = Number(contentLength);
    if (!Number.isFinite(advertised) || advertised < 0 || advertised > MAX_RESPONSE_BYTES) {
      throw new TypeError('API response exceeded the allowed size');
    }
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
