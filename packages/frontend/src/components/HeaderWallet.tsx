'use client';

import { useEffect, useId, useRef, useState } from 'react';
import { useWallet } from '@/lib/wallet-context';

export function HeaderWallet() {
  const { address, connecting, error, connect, disconnect } = useWallet();
  const [showMenu, setShowMenu] = useState(false);
  const menuId = useId();
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!showMenu) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        setShowMenu(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setShowMenu(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [showMenu]);

  if (address) {
    return (
      <div className="relative z-50" ref={menuRef}>
        <button
          ref={triggerRef}
          type="button"
          onClick={() => setShowMenu(!showMenu)}
          className="flex items-center gap-1.5 sm:gap-2 rounded-full bg-[var(--surface-raised)] px-3 sm:px-4 py-2 transition-colors hover:bg-[var(--surface)]"
          aria-expanded={showMenu}
          aria-controls={menuId}
          aria-haspopup="menu"
          aria-label="Wallet menu"
        >
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent)]" />
          <span className="text-[12px] sm:text-[14px] font-[family-name:var(--font-mono)] text-[var(--text-primary)]">
            {address.slice(0, 4)}…{address.slice(-4)}
          </span>
        </button>

        {showMenu && (
          <div
            id={menuId}
            role="menu"
            className="absolute right-0 top-full mt-2 z-[60] bg-[var(--surface)] border border-[var(--border)] rounded-xl overflow-hidden min-w-[220px]"
          >
            <div className="px-4 py-3 text-[11px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] break-all border-b border-[var(--border)]">
              {address}
            </div>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                disconnect();
                setShowMenu(false);
              }}
              className="w-full px-4 py-2.5 text-left text-[13px] text-red-400/90 hover:bg-red-500/[0.06] transition-colors"
            >
              Disconnect
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={connect}
        disabled={connecting}
        className="rounded-full bg-[var(--accent)] px-3.5 sm:px-5 py-2.5 text-[13px] sm:text-[15px] font-semibold text-[var(--accent-contrast)] hover:bg-[var(--accent-hover)] disabled:opacity-50 transition-colors whitespace-nowrap"
      >
        {connecting ? (
          'Connecting…'
        ) : (
          <>
            <span className="sm:hidden">Connect</span>
            <span className="hidden sm:inline">Connect Wallet</span>
          </>
        )}
      </button>
      {error && (
        <div
          role="alert"
          className="absolute right-0 top-full z-[60] mt-2 w-64 rounded-xl border border-red-500/20 bg-[var(--surface)] px-3 py-2.5 text-[12px] leading-relaxed text-red-300 shadow-xl shadow-black/40"
        >
          {error}
        </div>
      )}
    </div>
  );
}
