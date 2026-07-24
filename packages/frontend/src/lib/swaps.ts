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

export type UserSwapsPage = {
  swaps: UserSwap[];
  nextCursor: string | null;
};

export async function fetchUserSwaps(
  user: string,
  opts?: { limit?: number; cursor?: string | null },
): Promise<UserSwapsPage> {
  const qs = new URLSearchParams({
    user,
    limit: String(opts?.limit ?? 20),
  });
  if (opts?.cursor) qs.set('cursor', opts.cursor);
  const resp = await fetch(`${API_URL}/api/v1/swaps?${qs}`);
  const json = await resp.json();
  if (!resp.ok || !json.success) {
    throw new Error(json.error || `swaps HTTP ${resp.status}`);
  }
  return {
    swaps: json.data?.swaps ?? [],
    nextCursor: json.data?.next_cursor ?? null,
  };
}

export const SWAP_SUCCESS_EVENT = 'lumagg:swap-success';
