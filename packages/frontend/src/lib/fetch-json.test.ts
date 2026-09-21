import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchJson } from './fetch-json';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('fetchJson', () => {
  it('returns parsed JSON for a successful response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify({ ok: true }), { status: 200 })),
    );

    await expect(fetchJson<{ ok: boolean }>('/test')).resolves.toEqual({ ok: true });
  });

  it('reports HTTP and invalid JSON responses', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response('unavailable', { status: 503 }))
      .mockResolvedValueOnce(new Response('not-json', { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    await expect(fetchJson('/http-error')).rejects.toThrow('HTTP 503');
    await expect(fetchJson('/invalid-json')).rejects.toThrow('invalid response');
  });

  it('distinguishes timeout from caller cancellation', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init?: RequestInit) => {
        return new Promise((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('Aborted', 'AbortError'));
          });
        });
      }),
    );

    await expect(fetchJson('/timeout', {}, 5)).rejects.toThrow('timed out');

    const controller = new AbortController();
    const request = fetchJson('/cancelled', { signal: controller.signal }, 1_000);
    controller.abort();
    await expect(request).rejects.toMatchObject({ name: 'AbortError' });
  });
});
