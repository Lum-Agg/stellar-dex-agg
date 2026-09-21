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
  fetchTokenBalance,
  type BalanceMap,
  type TrustlineMap,
} from '@/lib/balance';
import { useWallet } from '@/lib/wallet-context';

/** Floor for lazy `/api/v1/balance` calls per token. */
const MIN_BALANCE_FETCH_MS = 1000;
/** Avoid duplicate batch RPC calls when focus and visibility events fire together. */
const PASSIVE_REFRESH_MIN_MS = 2000;

export interface EnsureBalanceOptions {
  /** Bypass cache / throttle (e.g. right after ChangeTrust). */
  force?: boolean;
}

export interface AccountBalancesState {
  balances: BalanceMap;
  hasTrustline: TrustlineMap;
  tokensQueried: string[];
  loading: boolean;
  ready: boolean;
  lastUpdatedAt: number | null;
  refresh: () => Promise<void>;
  getBalance: (tokenId: string) => bigint | null;
  getHasTrustline: (tokenId: string) => boolean | null;
  /**
   * Mark trustline locally after a successful ChangeTrust.
   * Survives batch refresh until the API confirms `true` (avoids RPC lag flicker).
   */
  markHasTrustline: (tokenId: string, value?: boolean) => void;
  ensureBalance: (tokenId: string, opts?: EnsureBalanceOptions) => Promise<bigint | null>;
}

const AccountBalancesContext = createContext<AccountBalancesState>({
  balances: {},
  hasTrustline: {},
  tokensQueried: [],
  loading: false,
  ready: false,
  lastUpdatedAt: null,
  refresh: async () => {},
  getBalance: () => null,
  getHasTrustline: () => null,
  markHasTrustline: () => {},
  ensureBalance: async () => null,
});

export function useAccountBalances() {
  return useContext(AccountBalancesContext);
}

function mergeTrustlines(api: TrustlineMap, overrides: TrustlineMap): TrustlineMap {
  return { ...api, ...overrides };
}

export function AccountBalancesProvider({ children }: { children: ReactNode }) {
  const { address } = useWallet();
  const [balances, setBalances] = useState<BalanceMap>({});
  const [hasTrustline, setHasTrustline] = useState<TrustlineMap>({});
  const [tokensQueried, setTokensQueried] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [ready, setReady] = useState(false);
  const [lastUpdatedAt, setLastUpdatedAt] = useState<number | null>(null);
  const requestId = useRef(0);
  const lazyInflight = useRef<Map<string, Promise<bigint | null>>>(new Map());
  const lastFetchAt = useRef<Map<string, number>>(new Map());
  const lazyDone = useRef<Set<string>>(new Set());
  /** Local wins over API until chain/RPC catches up after ChangeTrust. */
  const trustlineOverrides = useRef<TrustlineMap>({});
  const balancesRef = useRef(balances);
  const hasTrustlineRef = useRef(hasTrustline);
  const addressRef = useRef(address);
  const balanceAccountRef = useRef(address);
  const lastPassiveRefreshAt = useRef(0);
  balancesRef.current = balances;
  hasTrustlineRef.current = hasTrustline;
  addressRef.current = address;

  const refresh = useCallback(async () => {
    if (!address) {
      setBalances({});
      setHasTrustline({});
      setTokensQueried([]);
      setReady(false);
      setLastUpdatedAt(null);
      lazyInflight.current.clear();
      lastFetchAt.current.clear();
      lazyDone.current.clear();
      trustlineOverrides.current = {};
      return;
    }

    const id = ++requestId.current;
    setLoading(true);
    // Keep previous ready/trustline visible while refreshing — avoids wiping
    // optimistic ChangeTrust marks and button flicker.
    try {
      // Parallel: common unlocks UI fast; catalog fills the rest.
      const commonPromise = fetchAccountBalances(address, 'common');
      // Attach the fallback immediately so an early catalog rejection is handled
      // while the smaller common request is still in flight.
      const catalogPromise = fetchAccountBalances(address, 'catalog').catch(() => null);

      const common = await commonPromise;
      if (id !== requestId.current) return;

      for (const [tokenId, value] of Object.entries(trustlineOverrides.current)) {
        if (value === true && common.hasTrustline[tokenId] === true) {
          delete trustlineOverrides.current[tokenId];
        }
      }

      setBalances(common.balances);
      setHasTrustline(mergeTrustlines(common.hasTrustline, trustlineOverrides.current));
      setTokensQueried(common.tokensQueried);
      setReady(true);
      setLastUpdatedAt(Date.now());
      setLoading(false);

      const catalog = await catalogPromise;
      if (id !== requestId.current || catalog === null) return;

      for (const [tokenId, value] of Object.entries(trustlineOverrides.current)) {
        if (value === true && catalog.hasTrustline[tokenId] === true) {
          delete trustlineOverrides.current[tokenId];
        }
      }

      setBalances({ ...common.balances, ...catalog.balances });
      setHasTrustline(
        mergeTrustlines(
          { ...common.hasTrustline, ...catalog.hasTrustline },
          trustlineOverrides.current,
        ),
      );
      setTokensQueried(catalog.tokensQueried);
      setLastUpdatedAt(Date.now());
    } catch {
      if (id === requestId.current) {
        setBalances({});
        setHasTrustline({ ...trustlineOverrides.current });
        setTokensQueried([]);
        setReady(false);
        setLoading(false);
      }
    }
  }, [address]);

  useEffect(() => {
    // Every cache below is account-scoped. Reset it even when switching
    // directly between two connected accounts without a disconnect event.
    requestId.current += 1;
    balanceAccountRef.current = address;
    lazyInflight.current.clear();
    lastFetchAt.current.clear();
    lazyDone.current.clear();
    trustlineOverrides.current = {};
    setBalances({});
    setHasTrustline({});
    setTokensQueried([]);
    setReady(false);
    setLastUpdatedAt(null);
    setLoading(false);
    void refresh();
  }, [address, refresh]);

  useEffect(() => {
    const refreshAfterExternalActivity = () => {
      if (document.visibilityState === 'hidden' || !addressRef.current) return;

      const now = Date.now();
      if (now - lastPassiveRefreshAt.current < PASSIVE_REFRESH_MIN_MS) return;
      lastPassiveRefreshAt.current = now;
      void refresh();
    };

    window.addEventListener('focus', refreshAfterExternalActivity);
    window.addEventListener('pageshow', refreshAfterExternalActivity);
    document.addEventListener('visibilitychange', refreshAfterExternalActivity);

    return () => {
      window.removeEventListener('focus', refreshAfterExternalActivity);
      window.removeEventListener('pageshow', refreshAfterExternalActivity);
      document.removeEventListener('visibilitychange', refreshAfterExternalActivity);
    };
  }, [refresh]);

  const getBalance = useCallback(
    (tokenId: string) => {
      if (balanceAccountRef.current !== address) return null;
      if (balances[tokenId] !== undefined) return balances[tokenId];
      if (!ready) return null;
      return null;
    },
    [address, balances, ready],
  );

  const getHasTrustline = useCallback(
    (tokenId: string) => {
      if (balanceAccountRef.current !== address) return null;
      if (trustlineOverrides.current[tokenId] !== undefined) {
        return trustlineOverrides.current[tokenId];
      }
      if (hasTrustline[tokenId] !== undefined) return hasTrustline[tokenId];
      if (!ready) return null;
      return null;
    },
    [address, hasTrustline, ready],
  );

  const markHasTrustline = useCallback((tokenId: string, value: boolean = true) => {
    trustlineOverrides.current[tokenId] = value;
    setHasTrustline((prev) => ({ ...prev, [tokenId]: value }));
  }, []);

  const ensureBalance = useCallback(async (tokenId: string, opts?: EnsureBalanceOptions) => {
    const account = addressRef.current?.trim();
    if (!account || !tokenId) return null;

    const force = opts?.force === true;
    const cacheMatchesAccount = balanceAccountRef.current?.trim() === account;
    const cached = cacheMatchesAccount ? balancesRef.current[tokenId] : undefined;
    const override = cacheMatchesAccount ? trustlineOverrides.current[tokenId] : undefined;
    const cachedTrustline =
      override !== undefined
        ? override
        : cacheMatchesAccount
          ? hasTrustlineRef.current[tokenId]
          : undefined;

    if (!force && cached !== undefined && cachedTrustline !== undefined) {
      return cached;
    }

    if (!force && lazyDone.current.has(tokenId) && cached !== undefined) {
      return cached;
    }

    const now = Date.now();
    const last = lastFetchAt.current.get(tokenId) ?? 0;
    if (!force && now - last < MIN_BALANCE_FETCH_MS) {
      const inflight = lazyInflight.current.get(tokenId);
      if (inflight) return inflight;
      return cached ?? null;
    }

    const inflight = lazyInflight.current.get(tokenId);
    if (inflight) return inflight;

    lastFetchAt.current.set(tokenId, now);

    const task = (async () => {
      const result = await fetchTokenBalance(account, tokenId);
      if (addressRef.current?.trim() !== account) return null;
      lazyDone.current.add(tokenId);
      if (result === null) return cached ?? null;

      setBalances((prev) => ({ ...prev, [tokenId]: result.balance }));

      if (result.hasTrustline === true) {
        delete trustlineOverrides.current[tokenId];
        setHasTrustline((prev) => ({ ...prev, [tokenId]: true }));
      } else if (result.hasTrustline === false) {
        // Do not clobber a post-ChangeTrust override (Soroban RPC often lags).
        if (trustlineOverrides.current[tokenId] !== true) {
          setHasTrustline((prev) => ({ ...prev, [tokenId]: false }));
        }
      }

      return result.balance;
    })().finally(() => {
      if (lazyInflight.current.get(tokenId) === task) {
        lazyInflight.current.delete(tokenId);
      }
    });

    lazyInflight.current.set(tokenId, task);
    return task;
  }, []);

  return (
    <AccountBalancesContext.Provider
      value={{
        balances,
        hasTrustline,
        tokensQueried,
        loading,
        ready,
        lastUpdatedAt,
        refresh,
        getBalance,
        getHasTrustline,
        markHasTrustline,
        ensureBalance,
      }}
    >
      {children}
    </AccountBalancesContext.Provider>
  );
}
