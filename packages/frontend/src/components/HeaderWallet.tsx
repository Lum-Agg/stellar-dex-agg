'use client';

import { useEffect, useRef, useState } from 'react';
import { useWallet } from '@/lib/wallet-context';

export function HeaderWallet() {
  const { address, connecting, connect, disconnect } = useWallet();
  const [showMenu, setShowMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showMenu) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, [showMenu]);

  if (address) {
    return (
      <div className="relative z-50" ref={menuRef}>
        <button
          onClick={() => setShowMenu(!showMenu)}
          className="flex items-center gap-2 bg-slate-900/80 border border-white/15 hover:border-white/30 rounded-xl px-3 py-2 transition-colors"
        >
          <span className="w-2 h-2 rounded-full bg-green-400" />
          <span className="text-sm font-mono text-slate-300">
            {address.slice(0, 4)}...{address.slice(-4)}
          </span>
        </button>

        {showMenu && (
          <div className="absolute right-0 top-full mt-2 z-[60] bg-slate-900 border border-white/15 rounded-xl shadow-2xl overflow-hidden min-w-[220px]">
            <div className="px-4 py-3 text-xs text-slate-500 font-mono break-all border-b border-white/10">
              {address}
            </div>
            <button
              onClick={() => { disconnect(); setShowMenu(false); }}
              className="w-full px-4 py-3 text-left text-sm text-red-300 hover:bg-red-500/10 transition-colors"
            >
              Disconnect
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <button
      onClick={connect}
      disabled={connecting}
      className="px-4 py-2 bg-gradient-to-r from-blue-600 to-violet-500 hover:from-blue-500 hover:to-violet-400 disabled:opacity-50 rounded-xl text-sm font-medium transition-all shadow-lg shadow-blue-900/30"
    >
      {connecting ? 'Connecting...' : 'Connect'}
    </button>
  );
}
