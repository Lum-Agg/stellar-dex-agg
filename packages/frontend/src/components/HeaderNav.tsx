'use client';

import { useEffect, useId, useRef, useState } from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';

const LINKS = [
  { href: '/', label: 'Swap', match: (path: string) => path === '/' },
  {
    href: '/portfolio',
    label: 'Portfolio',
    match: (path: string) => path.startsWith('/portfolio'),
  },
  { href: '/docs', label: 'Docs', match: (path: string) => path.startsWith('/docs') },
  { href: '/stats', label: 'Stats', match: (path: string) => path.startsWith('/stats') },
  {
    href: '/arbitrage',
    label: 'Arbitrage',
    match: (path: string) => path.startsWith('/arbitrage'),
  },
] as const;

export function HeaderNav() {
  const pathname = usePathname() || '/';
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setOpen(false);
  }, [pathname]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      {/* Desktop */}
      <nav className="hidden sm:flex items-center gap-5 md:gap-7 text-[16px] sm:text-[17px] font-medium text-[var(--text-secondary)]">
        {LINKS.map((link) => {
          const active = link.match(pathname);
          return (
            <Link
              key={link.href}
              href={link.href}
              className={`transition-colors ${
                active ? 'text-[var(--text-primary)]' : 'hover:text-[var(--text-primary)]'
              }`}
              aria-current={active ? 'page' : undefined}
            >
              {link.label}
            </Link>
          );
        })}
      </nav>

      {/* Mobile */}
      <button
        type="button"
        className="sm:hidden inline-flex items-center justify-center w-10 h-10 -ml-1 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.04] transition-colors"
        aria-label={open ? 'Close menu' : 'Open menu'}
        aria-expanded={open}
        aria-controls={menuId}
        onClick={() => setOpen((v) => !v)}
      >
        <MenuIcon open={open} />
      </button>

      {open && (
        <nav
          id={menuId}
          className="sm:hidden absolute left-0 top-[calc(100%+0.5rem)] z-50 min-w-[11rem] rounded-xl border border-white/10 bg-[var(--bg-0)] py-1.5 shadow-xl shadow-black/40"
        >
          {LINKS.map((link) => {
            const active = link.match(pathname);
            return (
              <Link
                key={link.href}
                href={link.href}
                className={`block px-4 py-2.5 text-[15px] font-medium transition-colors ${
                  active
                    ? 'text-[var(--text-primary)] bg-white/[0.04]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03]'
                }`}
                aria-current={active ? 'page' : undefined}
                onClick={() => setOpen(false)}
              >
                {link.label}
              </Link>
            );
          })}
        </nav>
      )}
    </div>
  );
}

function MenuIcon({ open }: { open: boolean }) {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden>
      {open ? (
        <path strokeWidth="2" strokeLinecap="round" d="M6 6l12 12M18 6L6 18" />
      ) : (
        <path strokeWidth="2" strokeLinecap="round" d="M4 7h16M4 12h16M4 17h16" />
      )}
    </svg>
  );
}
