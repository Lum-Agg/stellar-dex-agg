const DEFAULT_TIMEOUT_MS = 12_000;

export async function fetchJson<T>(
  url: string,
  init: RequestInit = {},
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  let timedOut = false;
  const abortFromCaller = () => controller.abort(init.signal?.reason);
  if (init.signal?.aborted) {
    abortFromCaller();
  } else {
    init.signal?.addEventListener('abort', abortFromCaller, { once: true });
  }
  const timer = globalThis.setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    if (!response.ok) {
      throw new Error(`Request failed with HTTP ${response.status}`);
    }
    try {
      return (await response.json()) as T;
    } catch {
      throw new Error('The server returned an invalid response');
    }
  } catch (error) {
    if (timedOut && error instanceof DOMException && error.name === 'AbortError') {
      throw new Error(`Request timed out after ${Math.round(timeoutMs / 1000)} seconds`);
    }
    throw error;
  } finally {
    globalThis.clearTimeout(timer);
    init.signal?.removeEventListener('abort', abortFromCaller);
  }
}
