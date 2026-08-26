import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AbortedError,
  NetworkError,
  ResponseTooLargeError,
  TimeoutError,
  executeTransport,
} from '../shared/api/transport';

const request = () => ({ method: 'GET' as const, path: '/api/v1/health', headers: new Headers() });

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('executeTransport', () => {
  it('classifies fetch rejection as a network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('network unavailable')));

    await expect(executeTransport(request())).rejects.toBeInstanceOf(NetworkError);
  });

  it('preserves caller cancellation as an aborted request', async () => {
    vi.stubGlobal('fetch', vi.fn((_path: string, init: RequestInit) => new Promise((_resolve, reject) => {
      init.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
    })));
    const controller = new AbortController();
    const pending = executeTransport({ ...request(), signal: controller.signal });
    controller.abort();

    await expect(pending).rejects.toBeInstanceOf(AbortedError);
  });

  it('owns a bounded timeout independently of caller cancellation', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('fetch', vi.fn((_path: string, init: RequestInit) => new Promise((_resolve, reject) => {
      init.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
    })));
    const pending = executeTransport(request());
    const timedOut = expect(pending).rejects.toBeInstanceOf(TimeoutError);
    await vi.advanceTimersByTimeAsync(30_000);

    await timedOut;
  });

  it('stops a chunked response once its bounded raw-byte limit is exceeded', async () => {
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array(256 * 1024));
        controller.enqueue(new Uint8Array(1));
      },
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(stream)));

    await expect(executeTransport(request())).rejects.toBeInstanceOf(ResponseTooLargeError);
  });
});
