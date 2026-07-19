const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export type UserSwap = {
  tx_hash: string;
  ledger: number;
  created_at: number;
  status: string;
  function_name: string;
  token_in: string | null;
  token_out: string | null;
  amount_in: string;
  amount_out: string | null;
  is_split: boolean;
};

export async function fetchUserSwaps(user: string, limit = 20): Promise<UserSwap[]> {
  const qs = new URLSearchParams({ user, limit: String(limit) });
  const resp = await fetch(`${API_URL}/api/v1/swaps?${qs}`);
  const json = await resp.json();
  if (!resp.ok || !json.success) {
    throw new Error(json.error || `swaps HTTP ${resp.status}`);
  }
  return json.data?.swaps ?? [];
}

export const SWAP_SUCCESS_EVENT = 'lumagg:swap-success';
