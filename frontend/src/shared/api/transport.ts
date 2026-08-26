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
const REQUEST_TIMEOUT_MS = 30_000;

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

function transportFailure(error: unknown, timedOut: boolean, callerSignal: AbortSignal | undefined): Error {
  if (timedOut) return new TimeoutError();
  if (callerSignal?.aborted === true || failureName(error) === 'AbortError') return new AbortedError();
  return new NetworkError();
}

async function boundedResponseBytes(
  response: Response,
  timedOut: () => boolean,
  callerSignal: AbortSignal | undefined,
): Promise<Uint8Array> {
  if (response.body === null) return new Uint8Array();

  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > MAX_RESPONSE_BYTES) {
        await reader.cancel();
        throw new ResponseTooLargeError();
      }
      chunks.push(value);
    }
  } catch (error) {
    if (error instanceof ResponseTooLargeError) throw error;
    throw transportFailure(error, timedOut(), callerSignal);
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

export async function executeTransport(request: TransportRequest): Promise<TransportResponse> {
  ensureApiPath(request.path);

  const controller = new AbortController();
  let timedOut = false;
  const abortForCaller = () => controller.abort();
  if (request.signal?.aborted === true) controller.abort();
  else request.signal?.addEventListener('abort', abortForCaller, { once: true });
  const timeout = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, REQUEST_TIMEOUT_MS);

  const init: RequestInit = {
    method: request.method,
    headers: request.headers,
    credentials: 'same-origin',
    redirect: 'error',
    signal: controller.signal,
  };
  if (request.body !== undefined) {
    const body = new Uint8Array(request.body.byteLength);
    body.set(request.body);
    init.body = body.buffer;
  }
  try {
    let response: Response;
    try {
      response = await fetch(request.path, init);
    } catch (error) {
      throw transportFailure(error, timedOut, request.signal);
    }

    const contentLength = response.headers.get('content-length');
    if (contentLength !== null) {
      const advertised = Number(contentLength);
      if (!Number.isFinite(advertised) || advertised < 0 || advertised > MAX_RESPONSE_BYTES) {
        throw new ResponseTooLargeError();
      }
    }

    const bytes = await boundedResponseBytes(response, () => timedOut, request.signal);

    return {
      status: response.status,
      headers: response.headers,
      bytes,
    };
  } finally {
    clearTimeout(timeout);
    request.signal?.removeEventListener('abort', abortForCaller);
  }
}
