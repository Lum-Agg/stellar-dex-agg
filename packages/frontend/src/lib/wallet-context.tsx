'use client';

import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useRef,
  type ReactNode,
} from 'react';
import { AccountBalancesProvider } from '@/lib/account-balances-context';
import { isWalletModalDismissal } from '@/lib/wallet-errors';

const ACTIVE_ADDRESS_KEY = '@StellarWalletsKit/activeAddress';

export interface SignTxOptions {
  /** Defaults to Networks.PUBLIC (mainnet Instant). Pass Networks.TESTNET for Limit. */
  networkPassphrase?: string;
}

export interface WalletState {
  address: string | null;
  connecting: boolean;
  error: string | null;
  connect: () => void;
  disconnect: () => void;
  signTx: (xdr: string, opts?: SignTxOptions) => Promise<string>;
}

const WalletContext = createContext<WalletState>({
  address: null,
  connecting: false,
  error: null,
  connect: () => {},
  disconnect: () => {},
  signTx: async () => '',
});

export function useWallet() {
  return useContext(WalletContext);
}

async function createWalletRuntime() {
  const [kitModule, typesModule, freighterModule, xbullModule, lobstrModule] = await Promise.all([
    import('@creit.tech/stellar-wallets-kit'),
    import('@creit.tech/stellar-wallets-kit/types'),
    import('@creit.tech/stellar-wallets-kit/modules/freighter'),
    import('@creit.tech/stellar-wallets-kit/modules/xbull'),
    import('@creit.tech/stellar-wallets-kit/modules/lobstr'),
  ]);
  const kit = kitModule.StellarWalletsKit;
  kit.init({
    // Must be the full passphrase, not the shorthand "public" (Freighter rejects signing otherwise).
    network: typesModule.Networks.PUBLIC,
    modules: [
      new freighterModule.FreighterModule(),
      new xbullModule.xBullModule(),
      new lobstrModule.LobstrModule(),
    ],
  });
  return {
    kit,
    events: typesModule.KitEventType,
    networks: typesModule.Networks,
  };
}

type WalletRuntime = Awaited<ReturnType<typeof createWalletRuntime>>;

let walletRuntime: WalletRuntime | null = null;
let walletRuntimeRequest: Promise<WalletRuntime> | null = null;

function loadWalletRuntime(): Promise<WalletRuntime> {
  if (walletRuntime) return Promise.resolve(walletRuntime);
  if (walletRuntimeRequest) return walletRuntimeRequest;

  walletRuntimeRequest = createWalletRuntime()
    .then((runtime) => {
      walletRuntime = runtime;
      return runtime;
    })
    .finally(() => {
      walletRuntimeRequest = null;
    });
  return walletRuntimeRequest;
}

function readPersistedAddress(): string | null {
  try {
    const address = globalThis.localStorage?.getItem(ACTIVE_ADDRESS_KEY) ?? null;
    return address?.startsWith('G') ? address : null;
  } catch {
    return null;
  }
}

/**
 * Read the kit's persisted address without contacting the wallet extension.
 * `fetchAddress()` can open a wallet permission prompt, so it must not run as
 * part of passive page focus or visibility handling.
 */
async function syncAddressFromWallet(runtime: WalletRuntime): Promise<string | null> {
  try {
    const { address } = await runtime.kit.getAddress();
    return address || null;
  } catch {
    return null;
  }
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const removeKitListenersRef = useRef<(() => void) | null>(null);

  const attachKitListeners = useCallback((runtime: WalletRuntime) => {
    if (removeKitListenersRef.current) return;
    const offState = runtime.kit.on(runtime.events.STATE_UPDATED, (event) => {
      setAddress(event.payload.address ?? null);
    });
    const offDisconnect = runtime.kit.on(runtime.events.DISCONNECT, () => {
      setAddress(null);
    });
    removeKitListenersRef.current = () => {
      offState();
      offDisconnect();
      removeKitListenersRef.current = null;
    };
  }, []);

  // Anonymous visitors do not need the wallet SDK. Returning users still restore
  // their persisted session and initialize the selected wallet module immediately.
  useEffect(() => {
    let cancelled = false;
    const persistedAddress = readPersistedAddress();
    if (persistedAddress) {
      setAddress(persistedAddress);
      void loadWalletRuntime()
        .then(async (runtime) => {
          if (cancelled) return;
          attachKitListeners(runtime);
          const restoredAddress = await syncAddressFromWallet(runtime);
          if (!cancelled) setAddress(restoredAddress);
        })
        .catch(() => {
          if (!cancelled) {
            setAddress(null);
            setError('Could not restore the previous wallet session. Please reconnect.');
          }
        });
    }

    const resync = () => {
      if (document.visibilityState === 'hidden' || !walletRuntime) return;
      void (async () => {
        const live = await syncAddressFromWallet(walletRuntime);
        if (!cancelled) setAddress(live);
      })();
    };
    const syncStorage = (event: StorageEvent) => {
      if (event.key === ACTIVE_ADDRESS_KEY) setAddress(readPersistedAddress());
    };
    window.addEventListener('focus', resync);
    document.addEventListener('visibilitychange', resync);
    window.addEventListener('storage', syncStorage);

    return () => {
      cancelled = true;
      removeKitListenersRef.current?.();
      window.removeEventListener('focus', resync);
      document.removeEventListener('visibilitychange', resync);
      window.removeEventListener('storage', syncStorage);
    };
  }, [attachKitListeners]);

  const connect = useCallback(async () => {
    setConnecting(true);
    setError(null);
    try {
      const runtime = await loadWalletRuntime();
      attachKitListeners(runtime);
      const { address: addr } = await runtime.kit.authModal();
      if (addr) {
        setAddress(addr);
      }
    } catch (err: unknown) {
      if (!isWalletModalDismissal(err)) {
        setError('Could not connect to the wallet. Please try again.');
      }
    } finally {
      setConnecting(false);
    }
  }, [attachKitListeners]);

  const disconnect = useCallback(async () => {
    try {
      const runtime = await loadWalletRuntime();
      attachKitListeners(runtime);
      await runtime.kit.disconnect();
    } catch {}
    setAddress(null);
    setError(null);
  }, [attachKitListeners]);

  const signTx = useCallback(
    async (xdr: string, opts?: SignTxOptions): Promise<string> => {
      const runtime = await loadWalletRuntime();
      attachKitListeners(runtime);
      // Re-sync right before signing so we never sign for a stale stored address.
      let signer = address;
      try {
        const live = await runtime.kit.fetchAddress();
        if (live.address) {
          signer = live.address;
          setAddress(live.address);
        }
      } catch {
        // Fall back to React state if the wallet is temporarily unavailable.
      }
      const { signedTxXdr } = await runtime.kit.signTransaction(xdr, {
        networkPassphrase: opts?.networkPassphrase ?? runtime.networks.PUBLIC,
        address: signer ?? undefined,
      });
      return signedTxXdr;
    },
    [address, attachKitListeners],
  );

  return (
    <WalletContext.Provider value={{ address, connecting, error, connect, disconnect, signTx }}>
      <AccountBalancesProvider>{children}</AccountBalancesProvider>
    </WalletContext.Provider>
  );
}
