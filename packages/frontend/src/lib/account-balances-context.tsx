'use client';

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  fetchAccountBalances,
  fetchTokenBalanceStroops,
  type BalanceMap,
} from '@/lib/balance';
import { useWallet } from '@/lib/wallet-context';

export interface AccountBalancesState {
  balances: BalanceMap;
  tokensQueried: string[];
  loading: boolean;
  ready: boolean;
  refresh: () => Promise<void>;
  getBalance: (tokenId: string) => bigint | null;
  /** Fetch one token if not loaded in the common batch (no-op when cached). */
  ensureBalance: (tokenId: string) => Promise<bigint | null>;
}

const AccountBalancesContext = createContext<AccountBalancesState>({
  balances: {},
  tokensQueried: [],
  loading: false,
  ready: false,
  refresh: async () => {},
  getBalance: () => null,
  ensureBalance: async () => null,
});

export function useAccountBalances() {
  return useContext(AccountBalancesContext);
}

export function AccountBalancesProvider({ children }: { children: ReactNode }) {
  const { address } = useWallet();
  const [balances, setBalances] = useState<BalanceMap>({});
  const [tokensQueried, setTokensQueried] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [ready, setReady] = useState(false);
  const requestId = useRef(0);
  const lazyInflight = useRef<Map<string, Promise<bigint | null>>>(new Map());

  const refresh = useCallback(async () => {
    if (!address) {
      setBalances({});
      setTokensQueried([]);
      setReady(false);
      lazyInflight.current.clear();
      return;
    }

    const id = ++requestId.current;
    setLoading(true);
    setReady(false);
    lazyInflight.current.clear();
    try {
      const payload = await fetchAccountBalances(address);
      if (id === requestId.current) {
        setBalances(payload.balances);
        setTokensQueried(payload.tokensQueried);
        setReady(true);
      }
    } catch {
      if (id === requestId.current) {
        setBalances({});
        setTokensQueried([]);
        setReady(false);
      }
    } finally {
      if (id === requestId.current) {
        setLoading(false);
      }
    }
  }, [address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const getBalance = useCallback(
    (tokenId: string) => {
      if (!ready) return null;
      if (balances[tokenId] !== undefined) return balances[tokenId];
      return null;
    },
    [balances, ready]
  );

  const ensureBalance = useCallback(
    async (tokenId: string) => {
      if (!address) return null;

      const cached = balances[tokenId];
      if (cached !== undefined) return cached;

      const inflight = lazyInflight.current.get(tokenId);
      if (inflight) return inflight;

      const task = (async () => {
        const amount = await fetchTokenBalanceStroops(address, tokenId);
        if (amount === null) return null;
        setBalances((prev) => ({ ...prev, [tokenId]: amount }));
        return amount;
      })().finally(() => {
        lazyInflight.current.delete(tokenId);
      });

      lazyInflight.current.set(tokenId, task);
      return task;
    },
    [address, balances]
  );

  return (
    <AccountBalancesContext.Provider
      value={{ balances, tokensQueried, loading, ready, refresh, getBalance, ensureBalance }}
    >
      {children}
    </AccountBalancesContext.Provider>
  );
}
