const SUBMIT_VIA_STORAGE_KEY = 'lumagg.submitViaOfficialRpc';

export type SubmitVia = 'lumagg' | 'official';
export type SubmitNetwork = 'public' | 'testnet';

export function getSubmitViaPreference(): SubmitVia {
  if (typeof window === 'undefined') return 'official';
  try {
    return localStorage.getItem(SUBMIT_VIA_STORAGE_KEY) === 'lumagg' ? 'lumagg' : 'official';
  } catch {
    return 'official';
  }
}

export function setSubmitViaPreference(via: SubmitVia): void {
  if (typeof window === 'undefined') return;
  try {
    if (via === 'lumagg') localStorage.setItem(SUBMIT_VIA_STORAGE_KEY, 'lumagg');
    else localStorage.removeItem(SUBMIT_VIA_STORAGE_KEY);
  } catch {
    // Ignore storage quota and private-mode failures.
  }
}
