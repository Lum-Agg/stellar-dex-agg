'use client';

import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react';
import { StellarWalletsKit } from '@creit.tech/stellar-wallets-kit';
import { KitEventType, Networks } from '@creit.tech/stellar-wallets-kit/types';
import { FreighterModule } from '@creit.tech/stellar-wallets-kit/modules/freighter';
import { xBullModule } from '@creit.tech/stellar-wallets-kit/modules/xbull';
import { LobstrModule } from '@creit.tech/stellar-wallets-kit/modules/lobstr';
import { AccountBalancesProvider } from '@/lib/account-balances-context';

export interface SignTxOptions {
  /** Defaults to Networks.PUBLIC (mainnet Instant). Pass Networks.TESTNET for Limit. */
  networkPassphrase?: string;
}

export interface WalletState {
  address: string | null;
  connecting: boolean;
  connect: () => void;
  disconnect: () => void;
  signTx: (xdr: string, opts?: SignTxOptions) => Promise<string>;
}

const WalletContext = createContext<WalletState>({
  address: null,
  connecting: false,
  connect: () => {},
  disconnect: () => {},
  signTx: async () => '',
});

export function useWallet() {
  return useContext(WalletContext);
}

let kitInitialized = false;

function ensureKit() {
  if (kitInitialized) return;
  StellarWalletsKit.init({
    // Must be the full passphrase, not the shorthand "public" (Freighter rejects signing otherwise).
    network: Networks.PUBLIC,
    modules: [new FreighterModule(), new xBullModule(), new LobstrModule()],
  });
  kitInitialized = true;
}

/** True when kit still has a persisted connected session (not explicitly disconnected). */
async function hasStoredSession(): Promise<boolean> {
  try {
    ensureKit();
    const { address } = await StellarWalletsKit.getAddress();
    return Boolean(address);
  } catch {
    return false;
  }
}

/**
 * Prefer the wallet's current active account over the kit's persisted address.
 * `getAddress()` only reads kit memory/localStorage; `fetchAddress()` asks the
 * extension (e.g. Freighter) and updates kit state.
 */
async function syncAddressFromWallet(): Promise<string | null> {
  try {
    ensureKit();
    if (!(await hasStoredSession())) return null;
    const { address } = await StellarWalletsKit.fetchAddress();
    return address || null;
  } catch {
    return null;
  }
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  // Restore session from the wallet's live active account (not stale localStorage).
  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const live = await syncAddressFromWallet();
      if (!cancelled) setAddress(live);
    })();

    ensureKit();
    const offState = StellarWalletsKit.on(KitEventType.STATE_UPDATED, (event) => {
      setAddress(event.payload.address ?? null);
    });
    const offDisconnect = StellarWalletsKit.on(KitEventType.DISCONNECT, () => {
      setAddress(null);
    });

    // If the user switches accounts in the extension while this tab is open,
    // re-sync when they come back to the page.
    const resync = () => {
      if (document.visibilityState === 'hidden') return;
      void (async () => {
        const live = await syncAddressFromWallet();
        if (!cancelled) setAddress(live);
      })();
    };
    window.addEventListener('focus', resync);
    document.addEventListener('visibilitychange', resync);

    return () => {
      cancelled = true;
      offState();
      offDisconnect();
      window.removeEventListener('focus', resync);
      document.removeEventListener('visibilitychange', resync);
    };
  }, []);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      ensureKit();
      const { address: addr } = await StellarWalletsKit.authModal();
      if (addr) {
        setAddress(addr);
      }
    } catch (err: unknown) {
      console.error('Wallet connect error:', err);
    } finally {
      setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await StellarWalletsKit.disconnect();
    } catch {}
    setAddress(null);
  }, []);

  const signTx = useCallback(
    async (xdr: string, opts?: SignTxOptions): Promise<string> => {
      ensureKit();
      // Re-sync right before signing so we never sign for a stale stored address.
      let signer = address;
      try {
        const live = await StellarWalletsKit.fetchAddress();
        if (live.address) {
          signer = live.address;
          setAddress(live.address);
        }
      } catch {
        // Fall back to React state if the wallet is temporarily unavailable.
      }
      const { signedTxXdr } = await StellarWalletsKit.signTransaction(xdr, {
        networkPassphrase: opts?.networkPassphrase ?? Networks.PUBLIC,
        address: signer ?? undefined,
      });
      return signedTxXdr;
    },
    [address],
  );

  return (
    <WalletContext.Provider value={{ address, connecting, connect, disconnect, signTx }}>
      <AccountBalancesProvider>{children}</AccountBalancesProvider>
    </WalletContext.Provider>
  );
}
