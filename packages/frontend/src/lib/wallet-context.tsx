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

async function readStoredAddress(): Promise<string | null> {
  try {
    ensureKit();
    const { address } = await StellarWalletsKit.getAddress();
    return address || null;
  } catch {
    return null;
  }
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  // Restore kit session after full page loads (kit already persists address in localStorage).
  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const stored = await readStoredAddress();
      if (!cancelled && stored) setAddress(stored);
    })();

    ensureKit();
    const offState = StellarWalletsKit.on(KitEventType.STATE_UPDATED, (event) => {
      setAddress(event.payload.address ?? null);
    });
    const offDisconnect = StellarWalletsKit.on(KitEventType.DISCONNECT, () => {
      setAddress(null);
    });

    return () => {
      cancelled = true;
      offState();
      offDisconnect();
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
      const { signedTxXdr } = await StellarWalletsKit.signTransaction(xdr, {
        networkPassphrase: opts?.networkPassphrase ?? Networks.PUBLIC,
        address: address ?? undefined,
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
