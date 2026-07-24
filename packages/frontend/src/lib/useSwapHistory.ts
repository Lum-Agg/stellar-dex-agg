'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchUserSwaps, SWAP_SUCCESS_EVENT, type UserSwap } from '@/lib/swaps';
import { useWallet } from '@/lib/wallet-context';

const PAGE_SIZE = 20;

export function useSwapHistory() {
  const { address } = useWallet();
  const [swaps, setSwaps] = useState<UserSwap[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [unavailable, setUnavailable] = useState(false);
  const [refetchError, setRefetchError] = useState(false);
  const fetchGenRef = useRef(0);
  const hasSwapsRef = useRef(false);

  useEffect(() => {
    hasSwapsRef.current = swaps.length > 0;
  }, [swaps]);

  const loadSwaps = useCallback(
    async (opts?: { refetch?: boolean }) => {
      if (!address) return;

      const gen = ++fetchGenRef.current;
      setLoading(true);
      if (!opts?.refetch) {
        setUnavailable(false);
        setRefetchError(false);
      }

      try {
        const data = await fetchUserSwaps(address, { limit: PAGE_SIZE });
        if (gen !== fetchGenRef.current) return;
        setSwaps(data.swaps);
        setNextCursor(data.nextCursor);
        setUnavailable(false);
        setRefetchError(false);
      } catch {
        if (gen !== fetchGenRef.current) return;
        if (hasSwapsRef.current) {
          setRefetchError(true);
        } else {
          setUnavailable(true);
        }
      } finally {
        if (gen === fetchGenRef.current) {
          setLoading(false);
        }
      }
    },
    [address],
  );

  const loadMore = useCallback(async () => {
    if (!address || !nextCursor || loadingMore || loading) return;

    const gen = fetchGenRef.current;
    setLoadingMore(true);
    try {
      const data = await fetchUserSwaps(address, { limit: PAGE_SIZE, cursor: nextCursor });
      if (gen !== fetchGenRef.current) return;
      setSwaps((prev) => {
        const seen = new Set(prev.map((s) => `${s.tx_hash}:${s.ledger}`));
        const appended = data.swaps.filter((s) => !seen.has(`${s.tx_hash}:${s.ledger}`));
        return [...prev, ...appended];
      });
      setNextCursor(data.nextCursor);
      setRefetchError(false);
    } catch {
      if (gen !== fetchGenRef.current) return;
      setRefetchError(true);
    } finally {
      if (gen === fetchGenRef.current) {
        setLoadingMore(false);
      }
    }
  }, [address, nextCursor, loadingMore, loading]);

  useEffect(() => {
    fetchGenRef.current += 1;
    if (!address) {
      setSwaps([]);
      setNextCursor(null);
      setUnavailable(false);
      setRefetchError(false);
      setLoading(false);
      setLoadingMore(false);
      return;
    }

    setSwaps([]);
    setNextCursor(null);
    setUnavailable(false);
    setRefetchError(false);
    void loadSwaps();
  }, [address, loadSwaps]);

  useEffect(() => {
    if (!address) return;

    let timeout: ReturnType<typeof setTimeout> | undefined;
    const refetchAfterIndexing = () => {
      timeout = setTimeout(() => void loadSwaps({ refetch: true }), 2_000);
    };

    window.addEventListener(SWAP_SUCCESS_EVENT, refetchAfterIndexing);
    return () => {
      window.removeEventListener(SWAP_SUCCESS_EVENT, refetchAfterIndexing);
      if (timeout) clearTimeout(timeout);
    };
  }, [address, loadSwaps]);

  return {
    swaps,
    loading,
    loadingMore,
    hasMore: Boolean(nextCursor),
    loadMore,
    unavailable,
    refetchError,
  };
}
