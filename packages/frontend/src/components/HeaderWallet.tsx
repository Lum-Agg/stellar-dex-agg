'use client';

import { useState } from 'react';
import { useWallet } from '@/lib/wallet-context';

export function HeaderWallet() {
  const { address, connecting, connect, disconnect } = useWallet();
  const [showMenu, setShowMenu] = useState(false);

  if (address) {
    return (
      <div className="relative">
        <button
          onClick={() => setShowMenu(!showMenu)}
          className="flex items-center gap-2 bg-[#1a1b23] border border-white/10 hover:border-white/20 rounded-xl px-3 py-2 transition-colors"
        >
          <span className="w-2 h-2 rounded-full bg-green-400" />
          <span className="text-sm font-mono text-gray-300">
            {address.slice(0, 4)}...{address.slice(-4)}
          </span>
        </button>

        {showMenu && (
          <>
            <div className="fixed inset-0 z-40" onClick={() => setShowMenu(false)} />
            <div className="absolute right-0 top-full mt-2 z-50 bg-[#1a1b23] border border-white/10 rounded-xl shadow-xl overflow-hidden min-w-[200px]">
              <div className="px-4 py-3 text-xs text-gray-500 font-mono break-all border-b border-white/5">
                {address}
              </div>
              <button
                onClick={() => { disconnect(); setShowMenu(false); }}
                className="w-full px-4 py-3 text-left text-sm text-red-400 hover:bg-red-400/10 transition-colors"
              >
                Disconnect
              </button>
            </div>
          </>
        )}
      </div>
    );
  }

  return (
    <button
      onClick={connect}
      disabled={connecting}
      className="px-4 py-2 bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-500 hover:to-purple-500 disabled:opacity-50 rounded-xl text-sm font-medium transition-all"
    >
      {connecting ? 'Connecting...' : 'Connect'}
    </button>
  );
}
