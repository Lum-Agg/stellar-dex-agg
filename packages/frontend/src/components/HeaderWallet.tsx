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
          className="flex items-center gap-2 bg-zinc-900 border border-white/[0.08] hover:border-white/[0.14] rounded-md px-3 py-1.5 transition-colors"
        >
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
          <span className="text-[13px] font-mono text-zinc-300">
            {address.slice(0, 4)}...{address.slice(-4)}
          </span>
        </button>

        {showMenu && (
          <div className="absolute right-0 top-full mt-2 z-[60] bg-[#141419] border border-white/[0.1] rounded-lg shadow-xl overflow-hidden min-w-[220px]">
            <div className="px-4 py-3 text-[11px] text-zinc-500 font-mono break-all border-b border-white/[0.06]">
              {address}
            </div>
            <button
              onClick={() => { disconnect(); setShowMenu(false); }}
              className="w-full px-4 py-2.5 text-left text-[13px] text-red-400 hover:bg-red-500/[0.06] transition-colors"
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
      className="btn-primary px-4 py-1.5 text-[13px] disabled:opacity-50"
    >
      {connecting ? 'Connecting...' : 'Connect'}
    </button>
  );
}
