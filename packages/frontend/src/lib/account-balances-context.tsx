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
import { fetchAccountBalances, fetchTokenBalance, type BalanceMap, type TrustlineMap } from '@/lib/balance';
import { useWallet } from '@/lib/wallet-context';

export interface AccountBalancesState {
  balances: BalanceMap;
  hasTrustline: TrustlineMap;
  tokensQueried: string[];
  loading: boolean;
  ready: boolean;
  refresh: () => Promise<void>;
  getBalance: (tokenId: string) => bigint | null;
  getHasTrustline: (tokenId: string) => boolean | null;
  /** Fetch one token if not loaded in the common batch (no-op when cached). */
  ensureBalance: (tokenId: string) => Promise<bigint | null>;
}

const AccountBalancesContext = createContext<AccountBalancesState>({
  balances: {},
  hasTrustline: {},
  tokensQueried: [],
  loading: false,
  ready: false,
  refresh: async () => {},
  getBalance: () => null,
  getHasTrustline: () => null,
  ensureBalance: async () => null,
});

export function useAccountBalances() {
  return useContext(AccountBalancesContext);
}

export function AccountBalancesProvider({ children }: { children: ReactNode }) {
  const { address } = useWallet();
  const [balances, setBalances] = useState<BalanceMap>({});
  const [hasTrustline, setHasTrustline] = useState<TrustlineMap>({});
  const [tokensQueried, setTokensQueried] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [ready, setReady] = useState(false);
  const requestId = useRef(0);
  const lazyInflight = useRef<Map<string, Promise<bigint | null>>>(new Map());

  const refresh = useCallback(async () => {
    if (!address) {
      setBalances({});
      setHasTrustline({});
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
        setHasTrustline(payload.hasTrustline);
        setTokensQueried(payload.tokensQueried);
        setReady(true);
      }
    } catch {
      if (id === requestId.current) {
        setBalances({});
        setHasTrustline({});
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
    [balances, ready],
  );

  const getHasTrustline = useCallback(
    (tokenId: string) => {
      if (!ready) return null;
      if (hasTrustline[tokenId] !== undefined) return hasTrustline[tokenId];
      return null;
    },
    [hasTrustline, ready],
  );

  const ensureBalance = useCallback(
    async (tokenId: string) => {
      if (!address) return null;

      const cached = balances[tokenId];
      const cachedTrustline = hasTrustline[tokenId];
      if (cached !== undefined && cachedTrustline !== undefined) return cached;

      const inflight = lazyInflight.current.get(tokenId);
      if (inflight) return inflight;

      const task = (async () => {
        const result = await fetchTokenBalance(address, tokenId);
        if (result === null) return null;
        setBalances((prev) => ({ ...prev, [tokenId]: result.balance }));
        if (result.hasTrustline !== null) {
          setHasTrustline((prev) => ({ ...prev, [tokenId]: result.hasTrustline as boolean }));
        }
        return result.balance;
      })().finally(() => {
        lazyInflight.current.delete(tokenId);
      });

      lazyInflight.current.set(tokenId, task);
      return task;
    },
    [address, balances, hasTrustline],
  );

  return (
    <AccountBalancesContext.Provider
      value={{
        balances,
        hasTrustline,
        tokensQueried,
        loading,
        ready,
        refresh,
        getBalance,
        getHasTrustline,
        ensureBalance,
      }}
    >
      {children}
    </AccountBalancesContext.Provider>
  );
}
