/**
 * Local IDE stub for DefiLlama `utils/fetchURL`.
 */

export default async function fetchURL<T = unknown>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`fetchURL failed: ${res.status} ${url}`);
  }
  return res.json() as Promise<T>;
}
