'use client';

import { useEffect, useId, useRef, useState } from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { DOCUMENTATION_URL } from '@/lib/site';

const PRIMARY_LINKS = [
  { href: '/', label: 'Swap', match: (path: string) => path === '/' },
  {
    href: '/portfolio',
    label: 'Portfolio',
    match: (path: string) => path.startsWith('/portfolio'),
  },
] as const;

const SECONDARY_LINKS = [
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
  const [docsOpen, setDocsOpen] = useState(false);
  const menuId = useId();
  const docsMenuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setOpen(false);
    setDocsOpen(false);
  }, [pathname]);

  useEffect(() => {
    if (!open && !docsOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
        setDocsOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false);
        setDocsOpen(false);
      }
    };
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [docsOpen, open]);

  const docsActive = pathname.startsWith('/docs');

  return (
    <div ref={rootRef} className="relative">
      {/* Desktop */}
      <nav className="hidden sm:flex items-center gap-5 md:gap-7 text-[16px] sm:text-[17px] font-medium text-[var(--text-secondary)]">
        {PRIMARY_LINKS.map((link) => {
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

        <div className="relative">
          <button
            type="button"
            className={`inline-flex items-center gap-1 transition-colors ${
              docsActive || docsOpen
                ? 'text-[var(--text-primary)]'
                : 'hover:text-[var(--text-primary)]'
            }`}
            aria-expanded={docsOpen}
            aria-controls={docsMenuId}
            aria-current={docsActive ? 'page' : undefined}
            onClick={() => setDocsOpen((value) => !value)}
          >
            Docs
            <ChevronIcon open={docsOpen} />
          </button>

          {docsOpen && (
            <div
              id={docsMenuId}
              className="absolute left-1/2 top-[calc(100%+0.75rem)] z-50 w-64 -translate-x-1/2 rounded-xl border border-white/10 bg-[var(--bg-0)] p-1.5 shadow-xl shadow-black/40"
            >
              <Link
                href="/docs"
                className="block rounded-lg px-3 py-2.5 hover:bg-white/[0.04] transition-colors"
              >
                <span className="block text-[15px] font-semibold text-[var(--text-primary)]">
                  API Docs
                </span>
                <span className="mt-0.5 block text-[12px] font-normal text-[var(--text-muted)]">
                  Quickstart and live API reference
                </span>
              </Link>
              <a
                href={DOCUMENTATION_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="block rounded-lg px-3 py-2.5 hover:bg-white/[0.04] transition-colors"
                onClick={() => setDocsOpen(false)}
              >
                <span className="flex items-center gap-1 text-[15px] font-semibold text-[var(--text-primary)]">
                  Complete Docs
                  <ExternalLinkIcon />
                </span>
                <span className="mt-0.5 block text-[12px] font-normal text-[var(--text-muted)]">
                  Integration, deployment, and operations
                </span>
              </a>
            </div>
          )}
        </div>

        {SECONDARY_LINKS.map((link) => {
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
          {PRIMARY_LINKS.map((link) => {
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

          <div className="border-y border-white/[0.06] py-1">
            <span className="block px-4 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]">
              Docs
            </span>
            <Link
              href="/docs"
              className={`block px-4 py-2 text-[15px] font-medium transition-colors ${
                docsActive
                  ? 'text-[var(--text-primary)] bg-white/[0.04]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03]'
              }`}
              aria-current={docsActive ? 'page' : undefined}
              onClick={() => setOpen(false)}
            >
              API Docs
            </Link>
            <a
              href={DOCUMENTATION_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1 px-4 py-2 text-[15px] font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03] transition-colors"
              onClick={() => setOpen(false)}
            >
              Complete Docs
              <ExternalLinkIcon />
            </a>
          </div>

          {SECONDARY_LINKS.map((link) => {
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

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      className={`h-3.5 w-3.5 transition-transform ${open ? 'rotate-180' : ''}`}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      aria-hidden
    >
      <path d="m5 7.5 5 5 5-5" strokeWidth="1.75" strokeLinecap="round" />
    </svg>
  );
}

function ExternalLinkIcon() {
  return (
    <svg className="h-3 w-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" aria-hidden>
      <path d="M6 3h7v7M13 3 6.5 9.5" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M11 9.5V13H3V5h3.5" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
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
