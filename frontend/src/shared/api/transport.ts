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

export class NetworkError extends Error {
  constructor() {
    super('API network request failed');
    this.name = 'NetworkError';
  }
}

export class TimeoutError extends Error {
  constructor() {
    super('API request timed out');
    this.name = 'TimeoutError';
  }
}

export class AbortedError extends Error {
  constructor() {
    super('API request was aborted');
    this.name = 'AbortedError';
  }
}

export class ResponseTooLargeError extends Error {
  constructor() {
    super('API response exceeded the allowed size');
    this.name = 'ResponseTooLargeError';
  }
}

const MAX_RESPONSE_BYTES = 256 * 1024;

function ensureApiPath(path: string): void {
  if (!path.startsWith('/api/v1/') || path.includes('://')) {
    throw new TypeError('API requests must use a same-origin /api/v1/ path');
  }
}

function failureName(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null) return undefined;
  const name = Reflect.get(error, 'name');
  return typeof name === 'string' ? name : undefined;
}

function transportFailure(error: unknown, signal: AbortSignal | undefined): Error {
  if (failureName(error) === 'TimeoutError') return new TimeoutError();
  if (signal?.aborted === true || failureName(error) === 'AbortError') return new AbortedError();
  return new NetworkError();
}

export async function executeTransport(request: TransportRequest): Promise<TransportResponse> {
  ensureApiPath(request.path);

  const init: RequestInit = {
    method: request.method,
    headers: request.headers,
    credentials: 'same-origin',
    redirect: 'error',
  };
  if (request.body !== undefined) {
    const body = new Uint8Array(request.body.byteLength);
    body.set(request.body);
    init.body = body.buffer;
  }
  if (request.signal !== undefined) init.signal = request.signal;

  let response: Response;
  try {
    response = await fetch(request.path, init);
  } catch (error) {
    throw transportFailure(error, request.signal);
  }

  const contentLength = response.headers.get('content-length');
  if (contentLength !== null) {
    const advertised = Number(contentLength);
    if (!Number.isFinite(advertised) || advertised < 0 || advertised > MAX_RESPONSE_BYTES) {
      throw new ResponseTooLargeError();
    }
  }

  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await response.arrayBuffer());
  } catch (error) {
    throw transportFailure(error, request.signal);
  }
  if (bytes.byteLength > MAX_RESPONSE_BYTES) {
    throw new ResponseTooLargeError();
  }

  return {
    status: response.status,
    headers: response.headers,
    bytes,
  };
}
