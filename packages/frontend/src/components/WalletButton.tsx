'use client';

import { useState, useCallback } from 'react';

export function WalletButton({ onConnect }: { onConnect?: (publicKey: string) => void }) {
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const connect = useCallback(async () => {
    setConnecting(true);
    setError(null);

    try {
      const freighterApi = await import('@stellar/freighter-api');

      const isConnected = await freighterApi.isConnected();
      if (!isConnected) {
        setError('Install Freighter wallet extension');
        return;
      }

      const result = await freighterApi.requestAccess();
      const address =
        typeof result === 'string'
          ? result
          : (result as any)?.address || (result as any)?.publicKey || null;

      if (address && address.startsWith('G')) {
        setPublicKey(address);
        onConnect?.(address);
      } else {
        setError('Could not get wallet address');
      }
    } catch (err: any) {
      setError(err?.message || 'Connection failed');
    } finally {
      setConnecting(false);
    }
  }, [onConnect]);

  if (publicKey) {
    return (
      <button
        onClick={() => setPublicKey(null)}
        className="w-full py-3.5 bg-[var(--surface-raised)] border border-[var(--border)] hover:border-[var(--border-strong)] rounded-xl font-[family-name:var(--font-mono)] text-sm text-[var(--text-secondary)] transition-colors flex items-center justify-center gap-2"
      >
        <span className="w-2 h-2 rounded-full bg-[var(--accent)]" />
        {publicKey.slice(0, 6)}…{publicKey.slice(-4)}
      </button>
    );
  }

  return (
    <div>
      <button
        onClick={connect}
        disabled={connecting}
        className="btn-primary w-full py-3.5 disabled:opacity-50"
      >
        {connecting ? 'Connecting…' : 'Connect Wallet'}
      </button>
      {error && <p className="text-red-400/80 text-xs mt-2 text-center">{error}</p>}
    </div>
  );
}
