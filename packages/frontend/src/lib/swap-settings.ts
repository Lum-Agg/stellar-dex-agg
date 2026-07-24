/** Persisted Instant-swap routing / slippage settings (Jupiter-style). */

export const SWAP_SETTINGS_STORAGE_KEY = 'lumagg.swapSettings';

export const SLIPPAGE_PRESETS = [0.5, 1.0] as const;
export const SLIPPAGE_MIN = 0.01;
export const SLIPPAGE_MAX = 50;

export const MAX_HOPS_MIN = 1;
export const MAX_HOPS_MAX = 4;
export const MAX_SPLITS_MIN = 1;
export const MAX_SPLITS_MAX = 5;

export interface SwapSettings {
  slippage: number;
  maxHops: number;
  maxSplits: number;
}

export const DEFAULT_SWAP_SETTINGS: SwapSettings = {
  slippage: 1.0,
  maxHops: 3,
  maxSplits: 3,
};

export function parseSlippageInput(raw: string): number | null {
  if (!raw || raw === '.') return null;
  const n = Number(raw);
  if (!Number.isFinite(n)) return null;
  if (n < SLIPPAGE_MIN || n > SLIPPAGE_MAX) return null;
  return n;
}

function clampInt(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min;
  return Math.min(max, Math.max(min, Math.round(n)));
}

export function normalizeSwapSettings(raw: Partial<SwapSettings> | null | undefined): SwapSettings {
  const slippage =
    typeof raw?.slippage === 'number' && Number.isFinite(raw.slippage)
      ? Math.min(SLIPPAGE_MAX, Math.max(SLIPPAGE_MIN, raw.slippage))
      : DEFAULT_SWAP_SETTINGS.slippage;
  return {
    slippage,
    maxHops: clampInt(raw?.maxHops ?? DEFAULT_SWAP_SETTINGS.maxHops, MAX_HOPS_MIN, MAX_HOPS_MAX),
    maxSplits: clampInt(
      raw?.maxSplits ?? DEFAULT_SWAP_SETTINGS.maxSplits,
      MAX_SPLITS_MIN,
      MAX_SPLITS_MAX,
    ),
  };
}

export function loadSwapSettings(): SwapSettings {
  if (typeof window === 'undefined') return DEFAULT_SWAP_SETTINGS;
  try {
    const raw = window.localStorage.getItem(SWAP_SETTINGS_STORAGE_KEY);
    if (!raw) return DEFAULT_SWAP_SETTINGS;
    return normalizeSwapSettings(JSON.parse(raw) as Partial<SwapSettings>);
  } catch {
    return DEFAULT_SWAP_SETTINGS;
  }
}

export function saveSwapSettings(settings: SwapSettings): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(
      SWAP_SETTINGS_STORAGE_KEY,
      JSON.stringify(normalizeSwapSettings(settings)),
    );
  } catch {
    // ignore quota / private mode
  }
}

export function formatSlippageLabel(slippage: number): string {
  if (Number.isInteger(slippage)) return `${slippage}%`;
  const trimmed = String(slippage).replace(/\.?0+$/, '');
  return `${trimmed}%`;
}
