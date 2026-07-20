'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchUserSwaps, SWAP_SUCCESS_EVENT, type UserSwap } from '@/lib/swaps';
import { useWallet } from '@/lib/wallet-context';

export function useSwapHistory() {
  const { address } = useWallet();
  const [swaps, setSwaps] = useState<UserSwap[]>([]);
  const [loading, setLoading] = useState(false);
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
        const data = await fetchUserSwaps(address);
        if (gen !== fetchGenRef.current) return;
        setSwaps(data);
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

  useEffect(() => {
    fetchGenRef.current += 1;
    if (!address) {
      setSwaps([]);
      setUnavailable(false);
      setRefetchError(false);
      setLoading(false);
      return;
    }

    setSwaps([]);
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

  return { swaps, loading, unavailable, refetchError };
}
